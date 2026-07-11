use std::env;
use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use umbral_auth::{AuthMailError, AuthMailer, MailKind, OutgoingMail};

const RESEND_EMAILS_URL: &str = "https://api.resend.com/emails";

#[derive(Clone)]
pub struct ResendMailer {
    client: Client,
    api_key: String,
    from: String,
    reply_to: Option<String>,
    endpoint: String,
}

impl fmt::Debug for ResendMailer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResendMailer")
            .field("api_key", &"<redacted>")
            .field("from", &self.from)
            .field("reply_to", &self.reply_to)
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MailerConfigError {
    #[error("RESEND_API_KEY is required outside Dev/Test")]
    MissingApiKey,
    #[error("RESEND_FROM_EMAIL is required when Resend is enabled")]
    MissingFrom,
    #[error("{0} contains invalid newline characters")]
    InvalidHeaderValue(&'static str),
    #[error("failed to initialize the Resend HTTP client")]
    Client,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct ResendEmail {
    from: String,
    to: Vec<String>,
    subject: String,
    html: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_to: Option<String>,
}

impl ResendMailer {
    pub fn from_env(required: bool) -> Result<Option<Self>, MailerConfigError> {
        Self::from_config(
            required,
            nonempty_env("RESEND_API_KEY"),
            nonempty_env("RESEND_FROM_EMAIL"),
            nonempty_env("RESEND_REPLY_TO"),
        )
    }

    fn from_config(
        required: bool,
        api_key: Option<String>,
        from: Option<String>,
        reply_to: Option<String>,
    ) -> Result<Option<Self>, MailerConfigError> {
        let Some(api_key) = api_key else {
            return if required {
                Err(MailerConfigError::MissingApiKey)
            } else {
                Ok(None)
            };
        };
        let from = from.ok_or(MailerConfigError::MissingFrom)?;

        Self::new(api_key, from, reply_to).map(Some)
    }

    fn new(
        api_key: String,
        from: String,
        reply_to: Option<String>,
    ) -> Result<Self, MailerConfigError> {
        validate_header_value("RESEND_API_KEY", &api_key)?;
        validate_header_value("RESEND_FROM_EMAIL", &from)?;
        if let Some(reply_to) = reply_to.as_deref() {
            validate_header_value("RESEND_REPLY_TO", reply_to)?;
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| MailerConfigError::Client)?;

        Ok(Self {
            client,
            api_key,
            from,
            reply_to,
            endpoint: RESEND_EMAILS_URL.to_owned(),
        })
    }

    fn payload(&self, mail: &OutgoingMail) -> ResendEmail {
        ResendEmail {
            from: self.from.clone(),
            to: vec![mail.to.clone()],
            subject: mail.subject.clone(),
            html: mail.html.clone(),
            text: mail.text.clone(),
            reply_to: self.reply_to.clone(),
        }
    }
}

#[async_trait]
impl AuthMailer for ResendMailer {
    async fn send(&self, mail: OutgoingMail) -> Result<(), AuthMailError> {
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .header("Idempotency-Key", idempotency_key(&mail))
            .json(&self.payload(&mail))
            .send()
            .await
            .map_err(|error| {
                tracing::error!(
                    is_timeout = error.is_timeout(),
                    is_connect = error.is_connect(),
                    "Resend email request failed"
                );
                AuthMailError::Send("email provider request failed".to_owned())
            })?;

        if !response.status().is_success() {
            return Err(provider_rejection(response.status()));
        }

        Ok(())
    }
}

fn nonempty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn validate_header_value(name: &'static str, value: &str) -> Result<(), MailerConfigError> {
    if value.contains(['\r', '\n']) {
        return Err(MailerConfigError::InvalidHeaderValue(name));
    }
    Ok(())
}

fn idempotency_key(mail: &OutgoingMail) -> String {
    let mut digest = Sha256::new();
    digest.update(mail.to.as_bytes());
    digest.update(b"\0");

    let kind = match &mail.kind {
        MailKind::EmailVerification { code } => {
            digest.update(code.as_bytes());
            "verify"
        }
        MailKind::PasswordReset { reset_url } => {
            digest.update(reset_url.as_bytes());
            "reset"
        }
        _ => {
            digest.update(mail.subject.as_bytes());
            digest.update(mail.text.as_bytes());
            "auth"
        }
    };

    format!("turfzone-{kind}-{}", hex::encode(digest.finalize()))
}

fn provider_rejection(status: StatusCode) -> AuthMailError {
    tracing::error!(status = status.as_u16(), "Resend rejected auth email");
    AuthMailError::Send(format!(
        "email provider rejected request with status {}",
        status.as_u16()
    ))
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;
    use umbral_auth::{MailKind, OutgoingMail};

    use super::{MailerConfigError, ResendMailer, idempotency_key, provider_rejection};

    fn verification_mail() -> OutgoingMail {
        OutgoingMail {
            to: "player@example.com".to_owned(),
            username: "player1".to_owned(),
            kind: MailKind::EmailVerification {
                code: "483920".to_owned(),
            },
            subject: "Verify your email".to_owned(),
            html: "<p>Your code is <strong>483920</strong></p>".to_owned(),
            text: "Your code is 483920".to_owned(),
        }
    }

    #[test]
    fn builds_resend_payload_with_both_body_formats() {
        let mailer = ResendMailer::new(
            "re_secret".to_owned(),
            "Turfzone <no-reply@turfzone.co.ke>".to_owned(),
            Some("support@turfzone.co.ke".to_owned()),
        )
        .expect("valid mailer");

        let payload = mailer.payload(&verification_mail());

        assert_eq!(payload.from, "Turfzone <no-reply@turfzone.co.ke>");
        assert_eq!(payload.to, ["player@example.com"]);
        assert_eq!(payload.subject, "Verify your email");
        assert!(payload.html.contains("483920"));
        assert!(payload.text.contains("483920"));
        assert_eq!(payload.reply_to.as_deref(), Some("support@turfzone.co.ke"));
    }

    #[test]
    fn idempotency_key_is_stable_without_exposing_the_secret() {
        let mail = verification_mail();

        let first = idempotency_key(&mail);
        let second = idempotency_key(&mail);

        assert_eq!(first, second);
        assert!(first.starts_with("turfzone-verify-"));
        assert!(!first.contains("483920"));
        assert!(first.len() <= 256);
    }

    #[test]
    fn debug_output_redacts_api_key() {
        let mailer = ResendMailer::new(
            "re_do_not_log".to_owned(),
            "Turfzone <no-reply@turfzone.co.ke>".to_owned(),
            None,
        )
        .expect("valid mailer");

        let output = format!("{mailer:?}");

        assert!(!output.contains("re_do_not_log"));
        assert!(output.contains("<redacted>"));
    }

    #[test]
    fn rejects_newlines_in_sender_configuration() {
        let result = ResendMailer::new(
            "re_secret".to_owned(),
            "Turfzone <no-reply@turfzone.co.ke>\nBcc: attacker@example.com".to_owned(),
            None,
        );

        assert_eq!(
            result.expect_err("newline must be rejected"),
            MailerConfigError::InvalidHeaderValue("RESEND_FROM_EMAIL")
        );
    }

    #[test]
    fn requires_api_key_outside_dev_and_test() {
        let result = ResendMailer::from_config(true, None, None, None);

        assert_eq!(
            result.expect_err("production must reject missing API key"),
            MailerConfigError::MissingApiKey
        );
    }

    #[test]
    fn rejects_partial_resend_configuration() {
        let result = ResendMailer::from_config(false, Some("re_secret".to_owned()), None, None);

        assert_eq!(
            result.expect_err("enabled Resend requires a sender"),
            MailerConfigError::MissingFrom
        );
    }

    #[test]
    fn provider_errors_do_not_include_response_bodies() {
        let error = provider_rejection(StatusCode::UNAUTHORIZED).to_string();

        assert_eq!(
            error,
            "failed to send auth email: email provider rejected request with status 401"
        );
    }
}
