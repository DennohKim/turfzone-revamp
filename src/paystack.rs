use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha512;
use thiserror::Error;

type HmacSha512 = Hmac<Sha512>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaystackChargeRequest {
    pub email: Option<String>,
    pub phone: String,
    pub amount_minor: i64,
    pub currency: String,
    pub reference: String,
    pub split_code: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaystackCardInitializeRequest {
    pub email: String,
    pub amount_minor: i64,
    pub currency: String,
    pub reference: String,
    pub split_code: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaystackWebhookEnvelope {
    pub event: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaystackRefundRequest {
    pub transaction_reference: String,
    pub amount_minor: i64,
    pub currency: String,
    pub merchant_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaystackSubaccountRequest {
    pub business_name: String,
    pub settlement_bank: Option<String>,
    pub account_number: Option<String>,
    pub mobile_money_number: Option<String>,
    pub percentage_charge_bps: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaystackSplitRequest {
    pub name: String,
    pub currency: String,
    pub subaccount_code: String,
    pub platform_commission_bps: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaystackChargeSuccess {
    pub reference: String,
    pub amount_minor: i64,
    pub currency: String,
    pub channel: String,
    pub paid_at: Option<String>,
}

pub fn mpesa_charge_payload(request: PaystackChargeRequest) -> Result<Value, PaystackRequestError> {
    if request.currency != "KES" {
        return Err(PaystackRequestError::UnsupportedCurrency);
    }

    if request.amount_minor <= 0 {
        return Err(PaystackRequestError::InvalidAmount);
    }

    let phone = normalize_kenyan_phone(&request.phone)?;
    let email = request
        .email
        .unwrap_or_else(|| format!("{}@turfzone.local", phone.trim_start_matches('+')));

    Ok(json!({
        "email": email,
        "amount": request.amount_minor,
        "currency": request.currency,
        "reference": request.reference,
        "mobile_money": { "phone": phone, "provider": "mpesa" },
        "channels": ["mobile_money"],
        "split_code": request.split_code,
        "metadata": request.metadata,
    }))
}

pub fn card_initialize_payload(
    request: PaystackCardInitializeRequest,
) -> Result<Value, PaystackRequestError> {
    if request.currency != "KES" {
        return Err(PaystackRequestError::UnsupportedCurrency);
    }

    if request.amount_minor <= 0 {
        return Err(PaystackRequestError::InvalidAmount);
    }

    Ok(json!({
        "email": request.email,
        "amount": request.amount_minor,
        "currency": request.currency,
        "reference": request.reference,
        "channels": ["card"],
        "split_code": request.split_code,
        "metadata": request.metadata,
    }))
}

pub fn refund_payload(request: PaystackRefundRequest) -> Result<Value, PaystackRequestError> {
    if request.amount_minor <= 0 {
        return Err(PaystackRequestError::InvalidAmount);
    }

    if request.currency != "KES" {
        return Err(PaystackRequestError::UnsupportedCurrency);
    }

    Ok(json!({
        "transaction": request.transaction_reference,
        "amount": request.amount_minor,
        "merchant_note": request.merchant_note,
    }))
}

pub fn subaccount_payload(
    request: PaystackSubaccountRequest,
) -> Result<Value, PaystackRequestError> {
    if request.percentage_charge_bps < 0 || request.percentage_charge_bps > 10_000 {
        return Err(PaystackRequestError::InvalidCommissionRate);
    }

    match (&request.account_number, &request.mobile_money_number) {
        (Some(_), Some(_)) | (None, None) => {
            Err(PaystackRequestError::InvalidSettlementDestination)
        }
        (Some(account_number), None) => Ok(json!({
            "business_name": request.business_name,
            "settlement_bank": request.settlement_bank,
            "account_number": account_number,
            "percentage_charge": bps_to_percentage(request.percentage_charge_bps),
        })),
        (None, Some(mobile_money_number)) => Ok(json!({
            "business_name": request.business_name,
            "mobile_money_number": normalize_kenyan_phone(mobile_money_number)?,
            "percentage_charge": bps_to_percentage(request.percentage_charge_bps),
        })),
    }
}

pub fn split_payload(request: PaystackSplitRequest) -> Result<Value, PaystackRequestError> {
    if request.currency != "KES" {
        return Err(PaystackRequestError::UnsupportedCurrency);
    }

    if request.platform_commission_bps < 0 || request.platform_commission_bps > 10_000 {
        return Err(PaystackRequestError::InvalidCommissionRate);
    }

    Ok(json!({
        "name": request.name,
        "type": "percentage",
        "currency": request.currency,
        "subaccounts": [{
            "subaccount": request.subaccount_code,
            "share": 100.0 - bps_to_percentage(request.platform_commission_bps),
        }],
        "bearer_type": "account",
    }))
}

pub fn parse_charge_success(
    envelope: &PaystackWebhookEnvelope,
) -> Result<PaystackChargeSuccess, PaystackWebhookError> {
    if envelope.event != "charge.success" {
        return Err(PaystackWebhookError::UnsupportedEvent(
            envelope.event.clone(),
        ));
    }

    let reference = envelope
        .data
        .get("reference")
        .and_then(Value::as_str)
        .ok_or(PaystackWebhookError::MissingField("reference"))?
        .to_owned();
    let amount_minor = envelope
        .data
        .get("amount")
        .and_then(Value::as_i64)
        .ok_or(PaystackWebhookError::MissingField("amount"))?;
    let currency = envelope
        .data
        .get("currency")
        .and_then(Value::as_str)
        .ok_or(PaystackWebhookError::MissingField("currency"))?
        .to_owned();
    let channel = envelope
        .data
        .get("channel")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let paid_at = envelope
        .data
        .get("paid_at")
        .and_then(Value::as_str)
        .map(str::to_owned);

    Ok(PaystackChargeSuccess {
        reference,
        amount_minor,
        currency,
        channel,
        paid_at,
    })
}

pub fn verify_webhook_signature(
    raw_body: &[u8],
    signature_hex: &str,
    secret_key: &str,
) -> Result<(), PaystackSignatureError> {
    let expected = hex::decode(signature_hex).map_err(|_| PaystackSignatureError::Malformed)?;
    let mut mac = HmacSha512::new_from_slice(secret_key.as_bytes())
        .map_err(|_| PaystackSignatureError::InvalidKey)?;
    mac.update(raw_body);
    mac.verify_slice(&expected)
        .map_err(|_| PaystackSignatureError::Mismatch)
}

pub fn normalize_kenyan_phone(phone: &str) -> Result<String, PhoneError> {
    let stripped: String = phone.chars().filter(|ch| !ch.is_whitespace()).collect();

    if stripped.starts_with("+2547") && stripped.len() == 13 {
        return Ok(stripped);
    }

    if stripped.starts_with("2547") && stripped.len() == 12 {
        return Ok(format!("+{stripped}"));
    }

    if stripped.starts_with("07") && stripped.len() == 10 {
        return Ok(format!("+254{}", &stripped[1..]));
    }

    Err(PhoneError::InvalidKenyanMobile)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PaystackSignatureError {
    #[error("paystack signature is malformed")]
    Malformed,
    #[error("paystack secret key is invalid")]
    InvalidKey,
    #[error("paystack signature mismatch")]
    Mismatch,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PaystackRequestError {
    #[error("only KES is supported at launch")]
    UnsupportedCurrency,
    #[error("paystack amount must be positive")]
    InvalidAmount,
    #[error("commission basis points must be between 0 and 10000")]
    InvalidCommissionRate,
    #[error("manager must configure exactly one settlement destination")]
    InvalidSettlementDestination,
    #[error(transparent)]
    Phone(#[from] PhoneError),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PaystackWebhookError {
    #[error("unsupported paystack webhook event: {0}")]
    UnsupportedEvent(String),
    #[error("paystack webhook missing field: {0}")]
    MissingField(&'static str),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PhoneError {
    #[error("expected Kenyan mobile number in +2547... format")]
    InvalidKenyanMobile,
}

fn bps_to_percentage(bps: i64) -> f64 {
    bps as f64 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_paystack_hmac_sha512_signature() {
        let body = br#"{"event":"charge.success","data":{"reference":"booking-1"}}"#;
        let secret = "sk_test_secret";
        let mut mac = HmacSha512::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert!(verify_webhook_signature(body, &signature, secret).is_ok());
        assert_eq!(
            verify_webhook_signature(body, &signature, "wrong-secret").unwrap_err(),
            PaystackSignatureError::Mismatch
        );
    }

    #[test]
    fn normalizes_kenyan_phone_numbers() {
        assert_eq!(
            normalize_kenyan_phone("0712345678").unwrap(),
            "+254712345678"
        );
        assert_eq!(
            normalize_kenyan_phone("254712345678").unwrap(),
            "+254712345678"
        );
        assert_eq!(
            normalize_kenyan_phone("+254712345678").unwrap(),
            "+254712345678"
        );
    }

    #[test]
    fn builds_mpesa_charge_payload() {
        let payload = mpesa_charge_payload(PaystackChargeRequest {
            email: None,
            phone: "0712345678".to_owned(),
            amount_minor: 50_000,
            currency: "KES".to_owned(),
            reference: "booking-1".to_owned(),
            split_code: Some("SPL_test".to_owned()),
            metadata: json!({ "booking_id": "booking-1" }),
        })
        .unwrap();

        assert_eq!(payload["amount"], 50_000);
        assert_eq!(payload["mobile_money"]["phone"], "+254712345678");
        assert_eq!(payload["channels"][0], "mobile_money");
    }

    #[test]
    fn rejects_subaccount_with_two_destinations() {
        let err = subaccount_payload(PaystackSubaccountRequest {
            business_name: "Nairobi Turf".to_owned(),
            settlement_bank: Some("test-bank".to_owned()),
            account_number: Some("123".to_owned()),
            mobile_money_number: Some("0712345678".to_owned()),
            percentage_charge_bps: 1000,
        })
        .unwrap_err();

        assert_eq!(err, PaystackRequestError::InvalidSettlementDestination);
    }

    #[test]
    fn parses_charge_success_webhook() {
        let success = parse_charge_success(&PaystackWebhookEnvelope {
            event: "charge.success".to_owned(),
            data: json!({
                "reference": "booking-1",
                "amount": 50000,
                "currency": "KES",
                "channel": "mobile_money",
                "paid_at": "2026-07-09T12:00:00Z"
            }),
        })
        .unwrap();

        assert_eq!(success.reference, "booking-1");
        assert_eq!(success.amount_minor, 50_000);
        assert_eq!(success.channel, "mobile_money");
    }
}
