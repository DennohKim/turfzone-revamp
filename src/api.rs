use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use umbral::web::{HeaderMap, Json, Response};
use uuid::Uuid;

use crate::authz::{RoutePolicy, require_policy};
use crate::paystack::{
    PaystackCardInitializeRequest, PaystackChargeRequest, PaystackRefundRequest,
    PaystackSubaccountRequest, PaystackWebhookEnvelope, card_initialize_payload,
    mpesa_charge_payload, normalize_kenyan_phone, parse_charge_success, refund_payload,
    subaccount_payload, verify_webhook_signature,
};
use crate::services::availability::{
    ActiveBooking, AvailabilityExceptionInput, OpeningWindow, SlotState, availability_for_date,
};
use crate::services::booking::{BookingEngine, CancellationPolicy};
use crate::services::wallet::{WalletAccount, WalletLedgerKind};

#[derive(Debug, Clone, Serialize)]
pub struct ApiMetaResponse {
    pub app: &'static str,
    pub launch_country: &'static str,
    pub currency: &'static str,
    pub wallet_in_mvp: bool,
    pub hold_minutes: i64,
    pub cancellation_cutoff_bounds_hours: (i64, i64),
}

pub async fn meta() -> Json<ApiMetaResponse> {
    let settings = crate::TurfzoneSettings::from_env();

    Json(ApiMetaResponse {
        app: "turfzone",
        launch_country: "KE",
        currency: "KES",
        wallet_in_mvp: true,
        hold_minutes: settings.hold_minutes,
        cancellation_cutoff_bounds_hours: (
            settings.min_cancellation_cutoff_hours,
            settings.max_cancellation_cutoff_hours,
        ),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    fn error(error: impl ToString) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(error.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverySearchRequest {
    pub city: Option<String>,
    pub sport: Option<String>,
    pub date: Option<NaiveDate>,
    pub min_price_minor: Option<i64>,
    pub max_price_minor: Option<i64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub radius_km: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoverySearchPlan {
    pub city: Option<String>,
    pub sport: Option<String>,
    pub date: Option<NaiveDate>,
    pub price_filter: Option<(i64, i64)>,
    pub geo_filter: Option<GeoFilter>,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeoFilter {
    pub latitude: f64,
    pub longitude: f64,
    pub radius_km: f64,
}

pub async fn discovery_search(
    Json(request): Json<DiscoverySearchRequest>,
) -> Json<ApiResponse<DiscoverySearchPlan>> {
    Json(match build_discovery_plan(request) {
        Ok(plan) => ApiResponse::ok(plan),
        Err(error) => ApiResponse::error(error),
    })
}

pub fn build_discovery_plan(
    request: DiscoverySearchRequest,
) -> Result<DiscoverySearchPlan, &'static str> {
    let price_filter = match (request.min_price_minor, request.max_price_minor) {
        (Some(min), Some(max)) if min <= max && min >= 0 => Some((min, max)),
        (None, None) => None,
        _ => return Err("invalid price filter"),
    };

    let geo_filter = match (request.latitude, request.longitude, request.radius_km) {
        (Some(latitude), Some(longitude), Some(radius_km)) if radius_km > 0.0 => Some(GeoFilter {
            latitude,
            longitude,
            radius_km,
        }),
        (None, None, None) => None,
        _ => return Err("latitude, longitude, and radius_km must be supplied together"),
    };

    Ok(DiscoverySearchPlan {
        city: request.city.map(|city| city.trim().to_owned()),
        sport: request.sport.map(|sport| sport.trim().to_owned()),
        date: request.date,
        price_filter,
        geo_filter,
        currency: "KES".to_owned(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityRequest {
    pub date: NaiveDate,
    pub slot_minutes: i64,
    pub opening_windows: Vec<OpeningWindow>,
    pub exception: Option<AvailabilityExceptionInput>,
    pub active_bookings: Vec<ActiveBooking>,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityResponse {
    pub slots: Vec<SlotState>,
}

pub async fn field_availability(
    Json(request): Json<AvailabilityRequest>,
) -> Json<ApiResponse<AvailabilityResponse>> {
    let slots = availability_for_date(
        request.date,
        request.slot_minutes,
        &request.opening_windows,
        request.exception.as_ref(),
        &request.active_bookings,
        request.now,
    );

    Json(ApiResponse::ok(AvailabilityResponse { slots }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBookingHoldRequest {
    pub field_id: i64,
    pub player_id: i64,
    pub date: NaiveDate,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub amount_minor: i64,
    pub active_bookings: Vec<ActiveBooking>,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBookingHoldResponse {
    pub booking_id: Uuid,
    pub amount_minor: i64,
    pub commission_minor: i64,
    pub manager_share_minor: i64,
    pub hold_expires_at: DateTime<Utc>,
    pub currency: String,
}

pub async fn create_booking_hold(
    headers: HeaderMap,
    Json(request): Json<CreateBookingHoldRequest>,
) -> Result<Json<ApiResponse<CreateBookingHoldResponse>>, Response> {
    require_policy(&headers, RoutePolicy::Player, Some(request.player_id)).await?;
    let settings = crate::TurfzoneSettings::from_env();
    let engine = BookingEngine::new(
        settings.hold_minutes,
        settings.platform_commission_bps,
        settings.default_currency,
    );

    let result = engine.create_hold(
        request.field_id,
        request.player_id,
        request.date,
        request.start_time,
        request.end_time,
        request.amount_minor,
        &request.active_bookings,
        request.now,
    );

    Ok(Json(match result {
        Ok(hold) => ApiResponse::ok(CreateBookingHoldResponse {
            booking_id: hold.id,
            amount_minor: hold.amount_minor,
            commission_minor: hold.commission_minor,
            manager_share_minor: hold.manager_share_minor,
            hold_expires_at: hold.hold_expires_at,
            currency: hold.currency,
        }),
        Err(error) => ApiResponse::error(error),
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancellationQuoteRequest {
    pub booking_start: DateTime<Utc>,
    pub now: DateTime<Utc>,
    pub amount_minor: i64,
    pub cutoff_hours: i64,
    pub refund_percent: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancellationQuoteResponse {
    pub allowed: bool,
    pub refund_to_wallet_minor: i64,
}

pub async fn cancellation_quote(
    headers: HeaderMap,
    Json(request): Json<CancellationQuoteRequest>,
) -> Result<Json<ApiResponse<CancellationQuoteResponse>>, Response> {
    require_policy(&headers, RoutePolicy::Player, None).await?;
    let settings = crate::TurfzoneSettings::from_env();
    let policy = CancellationPolicy {
        cutoff_hours: request.cutoff_hours,
        refund_percent: request.refund_percent,
    };

    Ok(Json(
        match policy
            .validate(
                settings.min_cancellation_cutoff_hours,
                settings.max_cancellation_cutoff_hours,
            )
            .and_then(|policy| {
                policy.decide(request.booking_start, request.now, request.amount_minor)
            }) {
            Ok(decision) => ApiResponse::ok(CancellationQuoteResponse {
                allowed: decision.allowed,
                refund_to_wallet_minor: decision.refund_to_wallet_minor,
            }),
            Err(error) => ApiResponse::error(error),
        },
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentInitRequest {
    pub channel: PaymentInitChannel,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub amount_minor: i64,
    pub reference: String,
    pub split_code: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PaymentInitChannel {
    Mpesa,
    Card,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentInitResponse {
    pub provider: String,
    pub channel: PaymentInitChannel,
    pub payload: Value,
}

pub async fn initialize_payment(
    headers: HeaderMap,
    Json(request): Json<PaymentInitRequest>,
) -> Result<Json<ApiResponse<PaymentInitResponse>>, Response> {
    require_policy(&headers, RoutePolicy::Player, None).await?;
    Ok(Json(match build_payment_init_payload(request) {
        Ok(response) => ApiResponse::ok(response),
        Err(error) => ApiResponse::error(error),
    }))
}

pub fn build_payment_init_payload(
    request: PaymentInitRequest,
) -> Result<PaymentInitResponse, String> {
    let payload = match request.channel {
        PaymentInitChannel::Mpesa => mpesa_charge_payload(PaystackChargeRequest {
            email: request.email,
            phone: request.phone.ok_or("phone is required for M-Pesa")?,
            amount_minor: request.amount_minor,
            currency: "KES".to_owned(),
            reference: request.reference,
            split_code: request.split_code,
            metadata: request.metadata,
        })
        .map_err(|error| error.to_string())?,
        PaymentInitChannel::Card => card_initialize_payload(PaystackCardInitializeRequest {
            email: request.email.ok_or("email is required for card payment")?,
            amount_minor: request.amount_minor,
            currency: "KES".to_owned(),
            reference: request.reference,
            split_code: request.split_code,
            metadata: request.metadata,
        })
        .map_err(|error| error.to_string())?,
    };

    Ok(PaymentInitResponse {
        provider: "paystack".to_owned(),
        channel: request.channel,
        payload,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaystackWebhookVerifyRequest {
    pub raw_body: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaystackWebhookVerifyResponse {
    pub verified: bool,
    pub charge_success: Option<Value>,
}

pub async fn verify_paystack_webhook(
    headers: HeaderMap,
    Json(request): Json<PaystackWebhookVerifyRequest>,
) -> Result<Json<ApiResponse<PaystackWebhookVerifyResponse>>, Response> {
    require_policy(&headers, RoutePolicy::Admin, None).await?;
    Ok(verify_paystack_webhook_payload(request))
}

fn verify_paystack_webhook_payload(
    request: PaystackWebhookVerifyRequest,
) -> Json<ApiResponse<PaystackWebhookVerifyResponse>> {
    let settings = crate::TurfzoneSettings::from_env();
    let Some(secret_key) = settings.paystack_secret_key else {
        return Json(ApiResponse::error("PAYSTACK_SECRET_KEY is not configured"));
    };

    if let Err(error) =
        verify_webhook_signature(request.raw_body.as_bytes(), &request.signature, &secret_key)
    {
        return Json(ApiResponse::error(error));
    }

    let envelope = match serde_json::from_str::<PaystackWebhookEnvelope>(&request.raw_body) {
        Ok(envelope) => envelope,
        Err(error) => return Json(ApiResponse::error(error)),
    };

    let charge_success = parse_charge_success(&envelope)
        .ok()
        .and_then(|success| serde_json::to_value(success).ok());

    Json(ApiResponse::ok(PaystackWebhookVerifyResponse {
        verified: true,
        charge_success,
    }))
}

pub async fn paystack_webhook(
    headers: HeaderMap,
    raw_body: String,
) -> Json<ApiResponse<PaystackWebhookVerifyResponse>> {
    let signature = headers
        .get("x-paystack-signature")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let Some(signature) = signature else {
        return Json(ApiResponse::error("missing x-paystack-signature header"));
    };

    verify_paystack_webhook_payload(PaystackWebhookVerifyRequest {
        raw_body,
        signature,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSimulationRequest {
    pub user_id: i64,
    pub starting_balance_minor: i64,
    pub operations: Vec<WalletOperationRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletOperationRequest {
    pub kind: WalletOperationKind,
    pub amount_minor: i64,
    pub reference: String,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WalletOperationKind {
    TopUp,
    BookingDebit,
    RefundCredit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSimulationResponse {
    pub balance_minor: i64,
    pub ledger_sum_minor: i64,
}

pub async fn wallet_simulate(
    headers: HeaderMap,
    Json(request): Json<WalletSimulationRequest>,
) -> Result<Json<ApiResponse<WalletSimulationResponse>>, Response> {
    require_policy(&headers, RoutePolicy::Admin, None).await?;
    Ok(Json(match simulate_wallet(request) {
        Ok(response) => ApiResponse::ok(response),
        Err(error) => ApiResponse::error(error),
    }))
}

pub fn simulate_wallet(
    request: WalletSimulationRequest,
) -> Result<WalletSimulationResponse, String> {
    let mut wallet = WalletAccount::new(request.user_id, "KES");
    if request.starting_balance_minor > 0 {
        wallet
            .credit(
                WalletLedgerKind::AdminAdjustment,
                request.starting_balance_minor,
                "starting-balance",
                Utc::now(),
            )
            .map_err(|error| error.to_string())?;
    }

    for operation in request.operations {
        match operation.kind {
            WalletOperationKind::TopUp => wallet
                .credit(
                    WalletLedgerKind::TopUp,
                    operation.amount_minor,
                    operation.reference,
                    operation.now,
                )
                .map(|_| ()),
            WalletOperationKind::BookingDebit => wallet
                .debit(operation.amount_minor, operation.reference, operation.now)
                .map(|_| ()),
            WalletOperationKind::RefundCredit => wallet
                .credit(
                    WalletLedgerKind::RefundCredit,
                    operation.amount_minor,
                    operation.reference,
                    operation.now,
                )
                .map(|_| ()),
        }
        .map_err(|error| error.to_string())?;
    }

    Ok(WalletSimulationResponse {
        balance_minor: wallet.balance_minor,
        ledger_sum_minor: wallet.ledger_sum(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerSubaccountPayloadRequest {
    pub business_name: String,
    pub settlement_bank: Option<String>,
    pub account_number: Option<String>,
    pub mobile_money_number: Option<String>,
    pub platform_commission_bps: i64,
}

pub async fn manager_subaccount_payload(
    headers: HeaderMap,
    Json(request): Json<ManagerSubaccountPayloadRequest>,
) -> Result<Json<ApiResponse<Value>>, Response> {
    require_policy(&headers, RoutePolicy::Manager, None).await?;
    let percentage_charge_bps = request.platform_commission_bps;
    Ok(Json(
        subaccount_payload(PaystackSubaccountRequest {
            business_name: request.business_name,
            settlement_bank: request.settlement_bank,
            account_number: request.account_number,
            mobile_money_number: request.mobile_money_number,
            percentage_charge_bps,
        })
        .map(ApiResponse::ok)
        .unwrap_or_else(ApiResponse::error),
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundPayloadRequest {
    pub transaction_reference: String,
    pub amount_minor: i64,
    pub merchant_note: String,
}

pub async fn refund_request_payload(
    headers: HeaderMap,
    Json(request): Json<RefundPayloadRequest>,
) -> Result<Json<ApiResponse<Value>>, Response> {
    require_policy(&headers, RoutePolicy::Admin, None).await?;
    Ok(Json(
        refund_payload(PaystackRefundRequest {
            transaction_reference: request.transaction_reference,
            amount_minor: request.amount_minor,
            currency: "KES".to_owned(),
            merchant_note: request.merchant_note,
        })
        .map(ApiResponse::ok)
        .unwrap_or_else(ApiResponse::error),
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminVerifyManagerRequest {
    pub manager_id: i64,
    pub verified: bool,
    pub settlement_phone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminVerifyManagerResponse {
    pub manager_id: i64,
    pub verification_status: String,
    pub normalized_settlement_phone: Option<String>,
}

pub async fn admin_verify_manager(
    headers: HeaderMap,
    Json(request): Json<AdminVerifyManagerRequest>,
) -> Result<Json<ApiResponse<AdminVerifyManagerResponse>>, Response> {
    require_policy(&headers, RoutePolicy::Admin, None).await?;
    Ok(Json(match build_admin_verify_manager_response(request) {
        Ok(response) => ApiResponse::ok(response),
        Err(error) => ApiResponse::error(error),
    }))
}

pub fn build_admin_verify_manager_response(
    request: AdminVerifyManagerRequest,
) -> Result<AdminVerifyManagerResponse, String> {
    let normalized_settlement_phone = request
        .settlement_phone
        .as_deref()
        .map(normalize_kenyan_phone)
        .transpose()
        .map_err(|error| error.to_string())?;

    Ok(AdminVerifyManagerResponse {
        manager_id: request.manager_id,
        verification_status: if request.verified {
            "Verified"
        } else {
            "Rejected"
        }
        .to_owned(),
        normalized_settlement_phone,
    })
}

pub fn route_manifest() -> Value {
    json!({
        "auth": ["POST /api/auth/register", "POST /api/auth/login", "POST /api/auth/logout", "GET /api/auth/me", "POST /api/auth/change-password", "POST /api/auth/verify-email", "POST /api/auth/resend-verification", "POST /api/auth/password-forgot", "POST /api/auth/password-reset"],
        "discovery": ["POST /api/discovery/search", "GET /api/venue/", "GET /api/venue/:id"],
        "availability": ["POST /api/fields/availability"],
        "booking": ["POST /api/bookings/hold", "POST /api/bookings/cancellation-quote"],
        "payments": ["POST /api/payments/initialize", "POST /api/payments/webhook", "POST /api/payments/webhook/verify", "POST /api/payments/refund-payload"],
        "wallet": ["POST /api/wallet/simulate"],
        "manager": ["POST /api/manager/subaccount-payload"],
        "admin": ["POST /api/admin/managers/verify"],
        "crud": "Umbral REST exposes registered MVP models under /api/<table>/"
    })
}

pub async fn routes() -> Json<Value> {
    Json(route_manifest())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn discovery_requires_complete_geo_filter() {
        assert_eq!(
            build_discovery_plan(DiscoverySearchRequest {
                city: Some("Nairobi".to_owned()),
                sport: None,
                date: None,
                min_price_minor: None,
                max_price_minor: None,
                latitude: Some(-1.286389),
                longitude: None,
                radius_km: Some(10.0),
            })
            .unwrap_err(),
            "latitude, longitude, and radius_km must be supplied together"
        );
    }

    #[test]
    fn builds_mpesa_payment_init_payload() {
        let response = build_payment_init_payload(PaymentInitRequest {
            channel: PaymentInitChannel::Mpesa,
            email: None,
            phone: Some("0712345678".to_owned()),
            amount_minor: 50_000,
            reference: "booking-1".to_owned(),
            split_code: Some("SPL_test".to_owned()),
            metadata: json!({ "booking_id": "booking-1" }),
        })
        .unwrap();

        assert_eq!(response.provider, "paystack");
        assert_eq!(response.payload["mobile_money"]["phone"], "+254712345678");
    }

    #[test]
    fn wallet_simulation_preserves_ledger_invariant() {
        let now = Utc.with_ymd_and_hms(2026, 7, 9, 12, 0, 0).unwrap();
        let response = simulate_wallet(WalletSimulationRequest {
            user_id: 1,
            starting_balance_minor: 0,
            operations: vec![
                WalletOperationRequest {
                    kind: WalletOperationKind::TopUp,
                    amount_minor: 100_000,
                    reference: "topup-1".to_owned(),
                    now,
                },
                WalletOperationRequest {
                    kind: WalletOperationKind::BookingDebit,
                    amount_minor: 25_000,
                    reference: "booking-1".to_owned(),
                    now,
                },
                WalletOperationRequest {
                    kind: WalletOperationKind::RefundCredit,
                    amount_minor: 10_000,
                    reference: "refund-1".to_owned(),
                    now,
                },
            ],
        })
        .unwrap();

        assert_eq!(response.balance_minor, 85_000);
        assert_eq!(response.ledger_sum_minor, response.balance_minor);
    }

    #[test]
    fn admin_verification_normalizes_settlement_phone() {
        let response = build_admin_verify_manager_response(AdminVerifyManagerRequest {
            manager_id: 1,
            verified: true,
            settlement_phone: Some("0712345678".to_owned()),
        })
        .unwrap();

        assert_eq!(response.verification_status, "Verified");
        assert_eq!(
            response.normalized_settlement_phone,
            Some("+254712345678".to_owned())
        );
    }
}
