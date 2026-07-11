use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletAccount {
    pub user_id: i64,
    pub currency: String,
    pub balance_minor: i64,
    pub entries: Vec<WalletLedgerEntry>,
}

impl WalletAccount {
    pub fn new(user_id: i64, currency: impl Into<String>) -> Self {
        Self {
            user_id,
            currency: currency.into(),
            balance_minor: 0,
            entries: Vec::new(),
        }
    }

    pub fn credit(
        &mut self,
        kind: WalletLedgerKind,
        amount_minor: i64,
        reference: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<&WalletLedgerEntry, WalletError> {
        if amount_minor <= 0 {
            return Err(WalletError::InvalidAmount);
        }

        let reference = reference.into();
        self.ensure_unique_reference(&reference)?;
        self.balance_minor += amount_minor;
        self.push_entry(kind, amount_minor, reference, now)
    }

    pub fn debit(
        &mut self,
        amount_minor: i64,
        reference: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<&WalletLedgerEntry, WalletError> {
        if amount_minor <= 0 {
            return Err(WalletError::InvalidAmount);
        }

        if self.balance_minor < amount_minor {
            return Err(WalletError::InsufficientFunds);
        }

        let reference = reference.into();
        self.ensure_unique_reference(&reference)?;
        self.balance_minor -= amount_minor;
        self.push_entry(
            WalletLedgerKind::BookingDebit,
            -amount_minor,
            reference,
            now,
        )
    }

    pub fn ledger_sum(&self) -> i64 {
        self.entries.iter().map(|entry| entry.amount_minor).sum()
    }

    fn push_entry(
        &mut self,
        kind: WalletLedgerKind,
        amount_minor: i64,
        reference: String,
        created_at: DateTime<Utc>,
    ) -> Result<&WalletLedgerEntry, WalletError> {
        self.entries.push(WalletLedgerEntry {
            id: Uuid::new_v4(),
            kind,
            amount_minor,
            balance_after_minor: self.balance_minor,
            currency: self.currency.clone(),
            reference,
            created_at,
        });

        self.entries.last().ok_or(WalletError::LedgerWriteFailed)
    }

    fn ensure_unique_reference(&self, reference: &str) -> Result<(), WalletError> {
        if self
            .entries
            .iter()
            .any(|entry| entry.reference == reference)
        {
            return Err(WalletError::DuplicateReference);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalletLedgerKind {
    TopUp,
    BookingDebit,
    RefundCredit,
    AdminAdjustment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletLedgerEntry {
    pub id: Uuid,
    pub kind: WalletLedgerKind,
    pub amount_minor: i64,
    pub balance_after_minor: i64,
    pub currency: String,
    pub reference: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WalletError {
    #[error("wallet amount must be positive")]
    InvalidAmount,
    #[error("wallet has insufficient funds")]
    InsufficientFunds,
    #[error("wallet ledger reference already exists")]
    DuplicateReference,
    #[error("wallet ledger write failed")]
    LedgerWriteFailed,
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn ledger_sum_matches_balance_after_credit_and_debit() {
        let now = Utc.with_ymd_and_hms(2026, 7, 9, 12, 0, 0).unwrap();
        let mut wallet = WalletAccount::new(1, "KES");

        wallet
            .credit(WalletLedgerKind::TopUp, 100_000, "topup-1", now)
            .unwrap();
        wallet.debit(25_000, "booking-1", now).unwrap();

        assert_eq!(wallet.balance_minor, 75_000);
        assert_eq!(wallet.ledger_sum(), wallet.balance_minor);
    }

    #[test]
    fn duplicate_reference_is_rejected_for_idempotency() {
        let now = Utc.with_ymd_and_hms(2026, 7, 9, 12, 0, 0).unwrap();
        let mut wallet = WalletAccount::new(1, "KES");

        wallet
            .credit(WalletLedgerKind::RefundCredit, 10_000, "refund-1", now)
            .unwrap();

        let err = wallet
            .credit(WalletLedgerKind::RefundCredit, 10_000, "refund-1", now)
            .expect_err("duplicate should fail");

        assert_eq!(err, WalletError::DuplicateReference);
        assert_eq!(wallet.balance_minor, 10_000);
    }
}
