use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurfzoneSettings {
    pub default_currency: String,
    pub hold_minutes: i64,
    pub default_cancellation_cutoff_hours: i64,
    pub min_cancellation_cutoff_hours: i64,
    pub max_cancellation_cutoff_hours: i64,
    pub platform_commission_bps: i64,
    pub paystack_secret_key: Option<String>,
}

impl TurfzoneSettings {
    pub fn from_env() -> Self {
        Self {
            default_currency: env_string("TURFZONE_DEFAULT_CURRENCY", "KES"),
            hold_minutes: env_i64("TURFZONE_HOLD_MINUTES", 7),
            default_cancellation_cutoff_hours: env_i64(
                "TURFZONE_DEFAULT_CANCELLATION_CUTOFF_HOURS",
                6,
            ),
            min_cancellation_cutoff_hours: env_i64("TURFZONE_MIN_CANCELLATION_CUTOFF_HOURS", 2),
            max_cancellation_cutoff_hours: env_i64("TURFZONE_MAX_CANCELLATION_CUTOFF_HOURS", 24),
            platform_commission_bps: env_i64("TURFZONE_PLATFORM_COMMISSION_BPS", 1000),
            paystack_secret_key: env::var("PAYSTACK_SECRET_KEY").ok(),
        }
    }
}

fn env_string(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn env_i64(key: &str, default: i64) -> i64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}
