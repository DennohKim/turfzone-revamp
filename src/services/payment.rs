use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentEvent {
    pub reference: String,
    pub provider_event_id: Option<String>,
    pub amount_minor: i64,
    pub currency: String,
    pub paid_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentEventResult {
    Applied {
        booking_reference: String,
    },
    Duplicate {
        booking_reference: String,
    },
    AmountMismatch {
        expected_minor: i64,
        received_minor: i64,
    },
    CurrencyMismatch {
        expected: String,
        received: String,
    },
    UnknownReference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PaymentRecord {
    amount_minor: i64,
    currency: String,
    succeeded: bool,
}

#[derive(Debug, Default, Clone)]
pub struct PaymentStateMachine {
    records: HashMap<String, PaymentRecord>,
}

impl PaymentStateMachine {
    pub fn register_pending(
        &mut self,
        reference: impl Into<String>,
        amount_minor: i64,
        currency: impl Into<String>,
    ) {
        self.records.insert(
            reference.into(),
            PaymentRecord {
                amount_minor,
                currency: currency.into(),
                succeeded: false,
            },
        );
    }

    pub fn apply_success(&mut self, event: PaymentEvent) -> PaymentEventResult {
        let Some(record) = self.records.get_mut(&event.reference) else {
            return PaymentEventResult::UnknownReference;
        };

        if record.amount_minor != event.amount_minor {
            return PaymentEventResult::AmountMismatch {
                expected_minor: record.amount_minor,
                received_minor: event.amount_minor,
            };
        }

        if record.currency != event.currency {
            return PaymentEventResult::CurrencyMismatch {
                expected: record.currency.clone(),
                received: event.currency,
            };
        }

        if record.succeeded {
            return PaymentEventResult::Duplicate {
                booking_reference: event.reference,
            };
        }

        record.succeeded = true;
        PaymentEventResult::Applied {
            booking_reference: event.reference,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn duplicate_payment_success_is_idempotent() {
        let mut machine = PaymentStateMachine::default();
        let paid_at = Utc.with_ymd_and_hms(2026, 7, 9, 12, 0, 0).unwrap();
        let event = PaymentEvent {
            reference: "booking-1".to_owned(),
            provider_event_id: Some("evt-1".to_owned()),
            amount_minor: 50_000,
            currency: "KES".to_owned(),
            paid_at,
        };

        machine.register_pending("booking-1", 50_000, "KES");

        assert_eq!(
            machine.apply_success(event.clone()),
            PaymentEventResult::Applied {
                booking_reference: "booking-1".to_owned()
            }
        );
        assert_eq!(
            machine.apply_success(event),
            PaymentEventResult::Duplicate {
                booking_reference: "booking-1".to_owned()
            }
        );
    }

    #[test]
    fn mismatched_amount_is_rejected() {
        let mut machine = PaymentStateMachine::default();
        machine.register_pending("booking-1", 50_000, "KES");

        let result = machine.apply_success(PaymentEvent {
            reference: "booking-1".to_owned(),
            provider_event_id: None,
            amount_minor: 40_000,
            currency: "KES".to_owned(),
            paid_at: Utc::now(),
        });

        assert_eq!(
            result,
            PaymentEventResult::AmountMismatch {
                expected_minor: 50_000,
                received_minor: 40_000
            }
        );
    }
}
