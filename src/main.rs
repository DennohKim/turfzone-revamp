use turfzone::authz::StaffOrSuperuserReadOnly;
use turfzone::mailer::ResendMailer;
use turfzone::models::{
    Amenity, AvailabilityException, Booking, Field, FieldImage, ManagerProfile, Notification,
    OpeningHours, Payment, Payout, PaystackSubaccount, Refund, StaffMembership, UserProfile, Venue,
    VenueAmenity, Wallet, WalletTransaction,
};
use umbral::prelude::*;
use umbral_admin::AdminPlugin;
use umbral_auth::{AuthPlugin, AuthUser, BearerAuthentication};
use umbral_health::HealthPlugin;
use umbral_oauth::{OAuthPlugin, providers::GoogleProvider};
use umbral_permissions::PermissionsPlugin;
use umbral_security::{SecurityConfig, SecurityPlugin};
use umbral_sessions::SessionsPlugin;
use umbral_tasks::TasksPlugin;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let _turfzone_settings = turfzone::TurfzoneSettings::from_env();
    let settings = Settings::from_env()?;
    let pool = umbral::db::connect(&settings.database_url).await?;
    let require_mailer = !matches!(settings.environment, Environment::Dev | Environment::Test);
    let mut auth = AuthPlugin::<AuthUser>::default()
        .with_default_routes()
        .require_verified_email();
    if let Some(mailer) = ResendMailer::from_env(require_mailer)? {
        tracing::info!("Resend auth mailer configured");
        auth = auth.mailer(mailer);
    }
    let public_origin = std::env::var("UMBRAL_OAUTH_PUBLIC_ORIGIN")
        .unwrap_or_else(|_| "http://127.0.0.1:8000".to_owned());
    let login_redirect =
        std::env::var("UMBRAL_OAUTH_LOGIN_REDIRECT").unwrap_or_else(|_| "/".to_owned());
    let mut oauth = OAuthPlugin::new(public_origin).login_redirect(login_redirect);
    if let Some(google) = GoogleProvider::from_env() {
        oauth = oauth.provider(google);
    }
    let app = App::builder()
        .settings(settings)
        .database("default", pool)
        .routes(
            Routes::new()
                .get("/api/meta", turfzone::api::meta)
                .get("/api/routes", turfzone::api::routes)
                .post("/api/discovery/search", turfzone::api::discovery_search)
                .post(
                    "/api/fields/availability",
                    turfzone::api::field_availability,
                )
                .post("/api/bookings/hold", turfzone::api::create_booking_hold)
                .post(
                    "/api/bookings/cancellation-quote",
                    turfzone::api::cancellation_quote,
                )
                .post(
                    "/api/payments/initialize",
                    turfzone::api::initialize_payment,
                )
                .post(
                    "/api/payments/webhook/verify",
                    turfzone::api::verify_paystack_webhook,
                )
                .post("/api/payments/webhook", turfzone::api::paystack_webhook)
                .post(
                    "/api/payments/refund-payload",
                    turfzone::api::refund_request_payload,
                )
                .post("/api/wallet/simulate", turfzone::api::wallet_simulate)
                .post(
                    "/api/manager/subaccount-payload",
                    turfzone::api::manager_subaccount_payload,
                )
                .post(
                    "/api/admin/managers/verify",
                    turfzone::api::admin_verify_manager,
                ),
        )
        .plugin(auth)
        .plugin(SessionsPlugin::default())
        .plugin(oauth)
        .plugin(PermissionsPlugin::default())
        .plugin(AdminPlugin::default())
        .plugin(TasksPlugin::default())
        .plugin(HealthPlugin::default())
        .model::<UserProfile>()
        .model::<ManagerProfile>()
        .model::<StaffMembership>()
        .model::<Venue>()
        .model::<Amenity>()
        .model::<VenueAmenity>()
        .model::<Field>()
        .model::<FieldImage>()
        .model::<OpeningHours>()
        .model::<AvailabilityException>()
        .model::<Booking>()
        .model::<Payment>()
        .model::<Refund>()
        .model::<Wallet>()
        .model::<WalletTransaction>()
        .model::<PaystackSubaccount>()
        .model::<Payout>()
        .model::<Notification>()
        .plugin(
            umbral_rest::RestPlugin::default()
                .authenticate(BearerAuthentication::default())
                .default_permission(StaffOrSuperuserReadOnly),
        )
        .plugin(
            umbral_openapi::OpenApiPlugin::new()
                .title("Turfzone API")
                .version(env!("CARGO_PKG_VERSION"))
                .description("M-Pesa-first turf and court booking backend for Kenya."),
        )
        .plugin(SecurityPlugin::with_config(SecurityConfig {
            csrf_exempt_paths: vec!["/api".to_owned()],
            request_body_limit: Some(2 * 1024 * 1024),
            ..Default::default()
        }))
        .build()?;

    umbral_cli::dispatch(app).await?;
    Ok(())
}
