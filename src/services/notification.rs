use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationAttempt {
    pub id: Uuid,
    pub idempotency_key: String,
    pub user_id: i64,
    pub channel: NotificationChannel,
    pub subject: Option<String>,
    pub body: String,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub status: NotificationDeliveryStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationChannel {
    Email,
    Sms,
    Whatsapp,
    InApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationDeliveryStatus {
    Queued,
    Sent,
    Failed,
}

#[derive(Debug, Default, Clone)]
pub struct NotificationQueue {
    attempts: Vec<NotificationAttempt>,
    keys: HashSet<String>,
}

impl NotificationQueue {
    pub fn enqueue(
        &mut self,
        idempotency_key: impl Into<String>,
        user_id: i64,
        channel: NotificationChannel,
        subject: Option<String>,
        body: impl Into<String>,
        scheduled_for: Option<DateTime<Utc>>,
    ) -> Result<&NotificationAttempt, NotificationError> {
        let idempotency_key = idempotency_key.into();
        let body = body.into();

        if body.trim().is_empty() {
            return Err(NotificationError::EmptyBody);
        }

        if !self.keys.insert(idempotency_key.clone()) {
            return Err(NotificationError::DuplicateNotification);
        }

        self.attempts.push(NotificationAttempt {
            id: Uuid::new_v4(),
            idempotency_key,
            user_id,
            channel,
            subject,
            body,
            scheduled_for,
            status: NotificationDeliveryStatus::Queued,
        });

        self.attempts
            .last()
            .ok_or(NotificationError::QueueWriteFailed)
    }

    pub fn due(&self, now: DateTime<Utc>) -> Vec<&NotificationAttempt> {
        self.attempts
            .iter()
            .filter(|attempt| attempt.status == NotificationDeliveryStatus::Queued)
            .filter(|attempt| {
                attempt
                    .scheduled_for
                    .is_none_or(|scheduled_for| scheduled_for <= now)
            })
            .collect()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NotificationError {
    #[error("notification body cannot be empty")]
    EmptyBody,
    #[error("notification idempotency key already exists")]
    DuplicateNotification,
    #[error("notification queue write failed")]
    QueueWriteFailed,
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;

    #[test]
    fn enqueues_due_notifications_idempotently() {
        let now = Utc.with_ymd_and_hms(2026, 7, 9, 12, 0, 0).unwrap();
        let mut queue = NotificationQueue::default();

        queue
            .enqueue(
                "booking-confirmed-1",
                1,
                NotificationChannel::Sms,
                None,
                "Booking confirmed",
                Some(now - Duration::minutes(1)),
            )
            .unwrap();

        assert_eq!(queue.due(now).len(), 1);
        assert_eq!(
            queue
                .enqueue(
                    "booking-confirmed-1",
                    1,
                    NotificationChannel::Sms,
                    None,
                    "Booking confirmed",
                    None,
                )
                .unwrap_err(),
            NotificationError::DuplicateNotification
        );
    }
}
