use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpeningWindow {
    pub weekday: u32,
    pub opens_at: NaiveTime,
    pub closes_at: NaiveTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailabilityExceptionInput {
    pub date: NaiveDate,
    pub opens_at: Option<NaiveTime>,
    pub closes_at: Option<NaiveTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveBooking {
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub hold_expires_at: Option<DateTime<Utc>>,
    pub confirmed: bool,
}

impl ActiveBooking {
    pub fn blocks_availability(&self, now: DateTime<Utc>) -> bool {
        self.confirmed
            || self
                .hold_expires_at
                .is_some_and(|expires_at| expires_at > now)
    }

    pub fn overlaps(&self, start: NaiveTime, end: NaiveTime) -> bool {
        start < self.end_time && end > self.start_time
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotState {
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub available: bool,
}

pub fn availability_for_date(
    date: NaiveDate,
    slot_minutes: i64,
    opening_windows: &[OpeningWindow],
    exception: Option<&AvailabilityExceptionInput>,
    active_bookings: &[ActiveBooking],
    now: DateTime<Utc>,
) -> Vec<SlotState> {
    if slot_minutes <= 0 {
        return Vec::new();
    }

    let weekday = date.weekday().num_days_from_monday();
    let base_window = opening_windows
        .iter()
        .find(|window| window.weekday == weekday)
        .map(|window| (window.opens_at, window.closes_at));

    let Some((opens_at, closes_at)) = resolve_window(base_window, exception) else {
        return Vec::new();
    };

    let mut slots = Vec::new();
    let mut cursor = opens_at;
    let step = Duration::minutes(slot_minutes);

    loop {
        let (end_time, overflow_days) = cursor.overflowing_add_signed(step);
        if overflow_days != 0 || end_time > closes_at || end_time <= cursor {
            break;
        }

        let available = !active_bookings
            .iter()
            .filter(|booking| booking.blocks_availability(now))
            .any(|booking| booking.overlaps(cursor, end_time));

        slots.push(SlotState {
            start_time: cursor,
            end_time,
            available,
        });
        cursor = end_time;
    }

    slots
}

fn resolve_window(
    base_window: Option<(NaiveTime, NaiveTime)>,
    exception: Option<&AvailabilityExceptionInput>,
) -> Option<(NaiveTime, NaiveTime)> {
    match exception {
        Some(AvailabilityExceptionInput {
            opens_at: None,
            closes_at: None,
            ..
        }) => None,
        Some(AvailabilityExceptionInput {
            opens_at: Some(opens_at),
            closes_at: Some(closes_at),
            ..
        }) => Some((*opens_at, *closes_at)),
        _ => base_window,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(hour: u32, minute: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hour, minute, 0).expect("valid test time")
    }

    #[test]
    fn computes_slots_and_blocks_confirmed_booking() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 13).expect("valid date");
        let now = DateTime::<Utc>::from_naive_utc_and_offset(
            date.and_hms_opt(8, 0, 0).expect("valid datetime"),
            Utc,
        );

        let slots = availability_for_date(
            date,
            60,
            &[OpeningWindow {
                weekday: 0,
                opens_at: t(9, 0),
                closes_at: t(12, 0),
            }],
            None,
            &[ActiveBooking {
                start_time: t(10, 0),
                end_time: t(11, 0),
                hold_expires_at: None,
                confirmed: true,
            }],
            now,
        );

        assert_eq!(slots.len(), 3);
        assert!(slots[0].available);
        assert!(!slots[1].available);
        assert!(slots[2].available);
    }

    #[test]
    fn expired_hold_does_not_block_availability() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 13).expect("valid date");
        let now = DateTime::<Utc>::from_naive_utc_and_offset(
            date.and_hms_opt(8, 0, 0).expect("valid datetime"),
            Utc,
        );

        let slots = availability_for_date(
            date,
            60,
            &[OpeningWindow {
                weekday: 0,
                opens_at: t(9, 0),
                closes_at: t(10, 0),
            }],
            None,
            &[ActiveBooking {
                start_time: t(9, 0),
                end_time: t(10, 0),
                hold_expires_at: Some(now - Duration::minutes(1)),
                confirmed: false,
            }],
            now,
        );

        assert!(slots[0].available);
    }

    #[test]
    fn full_day_exception_closes_field() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 13).expect("valid date");
        let slots = availability_for_date(
            date,
            60,
            &[OpeningWindow {
                weekday: 0,
                opens_at: t(9, 0),
                closes_at: t(12, 0),
            }],
            Some(&AvailabilityExceptionInput {
                date,
                opens_at: None,
                closes_at: None,
            }),
            &[],
            Utc::now(),
        );

        assert!(slots.is_empty());
    }
}
