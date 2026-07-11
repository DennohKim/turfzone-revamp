use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayoutRecord {
    pub id: Uuid,
    pub manager_id: i64,
    pub amount_minor: i64,
    pub currency: String,
    pub provider_reference: Option<String>,
    pub status: PayoutStatus,
    pub created_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayoutStatus {
    Pending,
    Processing,
    Paid,
    Failed,
}

#[derive(Debug, Default, Clone)]
pub struct PayoutLedger {
    payouts: HashMap<Uuid, PayoutRecord>,
    provider_references: HashMap<String, Uuid>,
}

impl PayoutLedger {
    pub fn record_pending(
        &mut self,
        manager_id: i64,
        amount_minor: i64,
        currency: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<PayoutRecord, PayoutError> {
        if amount_minor <= 0 {
            return Err(PayoutError::InvalidAmount);
        }

        let payout = PayoutRecord {
            id: Uuid::new_v4(),
            manager_id,
            amount_minor,
            currency: currency.into(),
            provider_reference: None,
            status: PayoutStatus::Pending,
            created_at: now,
            settled_at: None,
        };
        self.payouts.insert(payout.id, payout.clone());
        Ok(payout)
    }

    pub fn mark_paid(
        &mut self,
        payout_id: Uuid,
        provider_reference: impl Into<String>,
        settled_at: DateTime<Utc>,
    ) -> Result<PayoutRecord, PayoutError> {
        let provider_reference = provider_reference.into();
        if self.provider_references.contains_key(&provider_reference) {
            return Err(PayoutError::DuplicateProviderReference);
        }

        let payout = self
            .payouts
            .get_mut(&payout_id)
            .ok_or(PayoutError::UnknownPayout)?;
        payout.status = PayoutStatus::Paid;
        payout.provider_reference = Some(provider_reference.clone());
        payout.settled_at = Some(settled_at);
        self.provider_references
            .insert(provider_reference, payout_id);
        Ok(payout.clone())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PayoutError {
    #[error("payout amount must be positive")]
    InvalidAmount,
    #[error("unknown payout")]
    UnknownPayout,
    #[error("provider payout reference already exists")]
    DuplicateProviderReference,
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn payout_provider_reference_is_idempotency_key() {
        let now = Utc.with_ymd_and_hms(2026, 7, 9, 12, 0, 0).unwrap();
        let mut ledger = PayoutLedger::default();
        let first = ledger.record_pending(1, 45_000, "KES", now).unwrap();
        let second = ledger.record_pending(1, 35_000, "KES", now).unwrap();

        ledger.mark_paid(first.id, "settlement-1", now).unwrap();
        assert_eq!(
            ledger
                .mark_paid(second.id, "settlement-1", now)
                .unwrap_err(),
            PayoutError::DuplicateProviderReference
        );
    }
}
