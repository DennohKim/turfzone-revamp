use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::availability::ActiveBooking;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookingEngine {
    pub hold_minutes: i64,
    pub platform_commission_bps: i64,
    pub default_currency: String,
}

impl BookingEngine {
    pub fn new(hold_minutes: i64, platform_commission_bps: i64, default_currency: String) -> Self {
        Self {
            hold_minutes,
            platform_commission_bps,
            default_currency,
        }
    }

    pub fn create_hold(
        &self,
        field_id: i64,
        player_id: i64,
        date: NaiveDate,
        start_time: NaiveTime,
        end_time: NaiveTime,
        amount_minor: i64,
        active_bookings: &[ActiveBooking],
        now: DateTime<Utc>,
    ) -> Result<BookingHold, BookingError> {
        if amount_minor <= 0 {
            return Err(BookingError::InvalidAmount);
        }

        if end_time <= start_time {
            return Err(BookingError::InvalidTimeRange);
        }

        if active_bookings
            .iter()
            .filter(|booking| booking.blocks_availability(now))
            .any(|booking| booking.overlaps(start_time, end_time))
        {
            return Err(BookingError::SlotUnavailable);
        }

        let commission_minor = calculate_commission(amount_minor, self.platform_commission_bps)?;
        let manager_share_minor = amount_minor - commission_minor;

        Ok(BookingHold {
            id: Uuid::new_v4(),
            field_id,
            player_id,
            date,
            start_time,
            end_time,
            amount_minor,
            currency: self.default_currency.clone(),
            commission_minor,
            manager_share_minor,
            hold_expires_at: now + Duration::minutes(self.hold_minutes),
        })
    }

    pub fn expire_pending_hold(&self, hold_expires_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        hold_expires_at <= now
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookingHold {
    pub id: Uuid,
    pub field_id: i64,
    pub player_id: i64,
    pub date: NaiveDate,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub amount_minor: i64,
    pub currency: String,
    pub commission_minor: i64,
    pub manager_share_minor: i64,
    pub hold_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancellationPolicy {
    pub cutoff_hours: i64,
    pub refund_percent: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancellationDecision {
    pub allowed: bool,
    pub refund_to_wallet_minor: i64,
}

impl CancellationPolicy {
    pub fn validate(self, min_hours: i64, max_hours: i64) -> Result<Self, BookingError> {
        if self.cutoff_hours < min_hours || self.cutoff_hours > max_hours {
            return Err(BookingError::CancellationPolicyOutOfBounds);
        }

        if !(0..=100).contains(&self.refund_percent) {
            return Err(BookingError::InvalidRefundPercent);
        }

        Ok(self)
    }

    pub fn decide(
        self,
        booking_start: DateTime<Utc>,
        now: DateTime<Utc>,
        amount_minor: i64,
    ) -> Result<CancellationDecision, BookingError> {
        if amount_minor < 0 {
            return Err(BookingError::InvalidAmount);
        }

        let cutoff = booking_start - Duration::hours(self.cutoff_hours);
        let refund_to_wallet_minor = if now <= cutoff {
            amount_minor * self.refund_percent / 100
        } else {
            0
        };

        Ok(CancellationDecision {
            allowed: now < booking_start,
            refund_to_wallet_minor,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BookingError {
    #[error("booking amount must be positive")]
    InvalidAmount,
    #[error("booking end time must be after start time")]
    InvalidTimeRange,
    #[error("slot is already held or booked")]
    SlotUnavailable,
    #[error("commission basis points must be between 0 and 10000")]
    InvalidCommissionRate,
    #[error("cancellation cutoff is outside platform bounds")]
    CancellationPolicyOutOfBounds,
    #[error("refund percent must be between 0 and 100")]
    InvalidRefundPercent,
}

pub fn calculate_commission(amount_minor: i64, commission_bps: i64) -> Result<i64, BookingError> {
    if !(0..=10_000).contains(&commission_bps) {
        return Err(BookingError::InvalidCommissionRate);
    }

    Ok(amount_minor * commission_bps / 10_000)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn t(hour: u32, minute: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hour, minute, 0).expect("valid time")
    }

    #[test]
    fn creates_hold_with_seven_minute_expiry_and_commission_split() {
        let engine = BookingEngine::new(7, 1000, "KES".to_owned());
        let now = Utc.with_ymd_and_hms(2026, 7, 9, 12, 0, 0).unwrap();

        let hold = engine
            .create_hold(
                1,
                2,
                NaiveDate::from_ymd_opt(2026, 7, 10).unwrap(),
                t(18, 0),
                t(19, 0),
                50_000,
                &[],
                now,
            )
            .expect("hold should be created");

        assert_eq!(hold.hold_expires_at, now + Duration::minutes(7));
        assert_eq!(hold.commission_minor, 5_000);
        assert_eq!(hold.manager_share_minor, 45_000);
    }

    #[test]
    fn rejects_overlapping_active_booking() {
        let engine = BookingEngine::new(7, 1000, "KES".to_owned());
        let now = Utc.with_ymd_and_hms(2026, 7, 9, 12, 0, 0).unwrap();

        let err = engine
            .create_hold(
                1,
                2,
                NaiveDate::from_ymd_opt(2026, 7, 10).unwrap(),
                t(18, 0),
                t(19, 0),
                50_000,
                &[ActiveBooking {
                    start_time: t(17, 30),
                    end_time: t(18, 30),
                    hold_expires_at: Some(now + Duration::minutes(3)),
                    confirmed: false,
                }],
                now,
            )
            .expect_err("overlap should be rejected");

        assert_eq!(err, BookingError::SlotUnavailable);
    }

    #[test]
    fn cancellation_refunds_only_before_cutoff() {
        let policy = CancellationPolicy {
            cutoff_hours: 6,
            refund_percent: 100,
        }
        .validate(2, 24)
        .unwrap();
        let start = Utc.with_ymd_and_hms(2026, 7, 10, 18, 0, 0).unwrap();

        let early = policy
            .decide(
                start,
                Utc.with_ymd_and_hms(2026, 7, 10, 11, 59, 0).unwrap(),
                50_000,
            )
            .unwrap();
        let late = policy
            .decide(
                start,
                Utc.with_ymd_and_hms(2026, 7, 10, 12, 1, 0).unwrap(),
                50_000,
            )
            .unwrap();

        assert_eq!(early.refund_to_wallet_minor, 50_000);
        assert_eq!(late.refund_to_wallet_minor, 0);
    }
}
