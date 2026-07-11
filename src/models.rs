use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use umbral::prelude::*;
use umbral_auth::AuthUser;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum UserRole {
    Player,
    Manager,
    ManagerStaff,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum VerificationStatus {
    Pending,
    Verified,
    Rejected,
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum SettlementDestinationType {
    BankAccount,
    MpesaWallet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum StaffRole {
    Owner,
    Operations,
    Finance,
    CheckIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum SportType {
    Football,
    Basketball,
    Futsal,
    Tennis,
    Padel,
    MultiSport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum BookingStatus {
    PendingPayment,
    Confirmed,
    Cancelled,
    Completed,
    NoShow,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum PaymentProvider {
    Paystack,
    Wallet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum PaymentChannel {
    Mpesa,
    Card,
    ApplePay,
    Wallet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum PaymentStatus {
    Pending,
    Succeeded,
    Failed,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum RefundStatus {
    Pending,
    Succeeded,
    Failed,
    ManualRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum WalletTransactionKind {
    TopUp,
    BookingDebit,
    RefundCredit,
    AdminAdjustment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum PayoutStatus {
    Pending,
    Processing,
    Paid,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum NotificationChannel {
    Email,
    Sms,
    Whatsapp,
    InApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Choices)]
pub enum NotificationStatus {
    Queued,
    Sent,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct UserProfile {
    pub id: i64,
    pub user: ForeignKey<AuthUser>,
    #[umbral(choices)]
    pub role: UserRole,
    #[umbral(unique, max_length = 20)]
    pub phone: String,
    pub phone_verified_at: Option<DateTime<Utc>>,
    pub display_name: Option<String>,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
    #[umbral(auto_now)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct ManagerProfile {
    pub id: i64,
    pub user: ForeignKey<AuthUser>,
    #[umbral(string, max_length = 120)]
    pub business_name: String,
    #[umbral(max_length = 20)]
    pub business_phone: String,
    #[umbral(choices, index)]
    pub verification_status: VerificationStatus,
    #[umbral(choices)]
    pub settlement_destination_type: SettlementDestinationType,
    pub payout_currency: String,
    pub settlement_details_json: String,
    pub verified_at: Option<DateTime<Utc>>,
    pub verified_by_id: Option<i64>,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
    #[umbral(auto_now)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
#[umbral(unique_together = [["manager_id", "staff_user_id"]])]
pub struct StaffMembership {
    pub id: i64,
    pub manager: ForeignKey<ManagerProfile>,
    pub staff_user: ForeignKey<AuthUser>,
    #[umbral(choices)]
    pub role: StaffRole,
    pub is_active: bool,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct Venue {
    pub id: i64,
    pub manager: ForeignKey<ManagerProfile>,
    #[umbral(string, max_length = 140, index)]
    pub name: String,
    pub description: String,
    pub address: String,
    #[umbral(index)]
    pub city: String,
    pub latitude: f64,
    pub longitude: f64,
    pub is_active: bool,
    pub cancellation_cutoff_hours: i32,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
    #[umbral(auto_now)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct Amenity {
    pub id: i64,
    #[umbral(unique, max_length = 80)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
#[umbral(unique_together = [["venue_id", "amenity_id"]])]
pub struct VenueAmenity {
    pub id: i64,
    pub venue: ForeignKey<Venue>,
    pub amenity: ForeignKey<Amenity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct Field {
    pub id: i64,
    pub venue: ForeignKey<Venue>,
    #[umbral(string, max_length = 100)]
    pub name: String,
    #[umbral(choices, index)]
    pub sport: SportType,
    pub size_label: Option<String>,
    pub surface: Option<String>,
    pub capacity: Option<i32>,
    pub base_price_minor: i64,
    pub currency: String,
    pub slot_minutes: i32,
    pub is_active: bool,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
    #[umbral(auto_now)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct FieldImage {
    pub id: i64,
    pub field: ForeignKey<Field>,
    pub image_url: String,
    pub sort_order: i32,
    pub is_cover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
#[umbral(unique_together = [["field_id", "weekday"]])]
pub struct OpeningHours {
    pub id: i64,
    pub field: ForeignKey<Field>,
    pub weekday: i16,
    pub opens_at: NaiveTime,
    pub closes_at: NaiveTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
#[umbral(unique_together = [["field_id", "date"]])]
pub struct AvailabilityException {
    pub id: i64,
    pub field: ForeignKey<Field>,
    pub date: NaiveDate,
    pub opens_at: Option<NaiveTime>,
    pub closes_at: Option<NaiveTime>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
#[umbral(unique_together = [["field_id", "date", "start_time"]])]
pub struct Booking {
    pub id: Uuid,
    pub field: ForeignKey<Field>,
    pub player: ForeignKey<AuthUser>,
    pub date: NaiveDate,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    #[umbral(choices, index)]
    pub status: BookingStatus,
    pub amount_minor: i64,
    pub currency: String,
    pub commission_minor: i64,
    pub manager_share_minor: i64,
    pub hold_expires_at: Option<DateTime<Utc>>,
    pub cancellation_policy_snapshot_json: String,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
    #[umbral(auto_now)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct Payment {
    pub id: Uuid,
    pub booking: ForeignKey<Booking>,
    #[umbral(unique, index)]
    pub reference: String,
    #[umbral(choices)]
    pub provider: PaymentProvider,
    #[umbral(choices)]
    pub channel: PaymentChannel,
    #[umbral(choices, index)]
    pub status: PaymentStatus,
    pub amount_minor: i64,
    pub currency: String,
    pub provider_payload_json: String,
    pub paid_at: Option<DateTime<Utc>>,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
    #[umbral(auto_now)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct Refund {
    pub id: Uuid,
    pub payment: ForeignKey<Payment>,
    pub booking: ForeignKey<Booking>,
    pub amount_minor: i64,
    pub currency: String,
    #[umbral(choices, index)]
    pub status: RefundStatus,
    pub reason: Option<String>,
    pub provider_reference: Option<String>,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
    #[umbral(auto_now)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct Wallet {
    pub id: i64,
    pub user: ForeignKey<AuthUser>,
    pub balance_minor: i64,
    pub currency: String,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
    #[umbral(auto_now)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct WalletTransaction {
    pub id: Uuid,
    pub wallet: ForeignKey<Wallet>,
    #[umbral(choices)]
    pub kind: WalletTransactionKind,
    pub amount_minor: i64,
    pub balance_after_minor: i64,
    pub currency: String,
    pub reference: String,
    pub metadata_json: String,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct PaystackSubaccount {
    pub id: i64,
    pub manager: ForeignKey<ManagerProfile>,
    #[umbral(unique)]
    pub subaccount_code: String,
    pub split_code: Option<String>,
    pub active: bool,
    pub payload_json: String,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct Payout {
    pub id: Uuid,
    pub manager: ForeignKey<ManagerProfile>,
    pub amount_minor: i64,
    pub currency: String,
    #[umbral(choices, index)]
    pub status: PayoutStatus,
    pub provider_reference: Option<String>,
    pub settled_at: Option<DateTime<Utc>>,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, umbral::orm::Model)]
pub struct Notification {
    pub id: Uuid,
    pub user: ForeignKey<AuthUser>,
    #[umbral(choices)]
    pub channel: NotificationChannel,
    #[umbral(choices, index)]
    pub status: NotificationStatus,
    pub subject: Option<String>,
    pub body: String,
    pub provider_reference: Option<String>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub sent_at: Option<DateTime<Utc>>,
    #[umbral(auto_now_add)]
    pub created_at: DateTime<Utc>,
}

pub fn all_model_meta() -> Vec<umbral::migrate::ModelMeta> {
    vec![
        umbral::migrate::ModelMeta::for_::<UserProfile>(),
        umbral::migrate::ModelMeta::for_::<ManagerProfile>(),
        umbral::migrate::ModelMeta::for_::<StaffMembership>(),
        umbral::migrate::ModelMeta::for_::<Venue>(),
        umbral::migrate::ModelMeta::for_::<Amenity>(),
        umbral::migrate::ModelMeta::for_::<VenueAmenity>(),
        umbral::migrate::ModelMeta::for_::<Field>(),
        umbral::migrate::ModelMeta::for_::<FieldImage>(),
        umbral::migrate::ModelMeta::for_::<OpeningHours>(),
        umbral::migrate::ModelMeta::for_::<AvailabilityException>(),
        umbral::migrate::ModelMeta::for_::<Booking>(),
        umbral::migrate::ModelMeta::for_::<Payment>(),
        umbral::migrate::ModelMeta::for_::<Refund>(),
        umbral::migrate::ModelMeta::for_::<Wallet>(),
        umbral::migrate::ModelMeta::for_::<WalletTransaction>(),
        umbral::migrate::ModelMeta::for_::<PaystackSubaccount>(),
        umbral::migrate::ModelMeta::for_::<Payout>(),
        umbral::migrate::ModelMeta::for_::<Notification>(),
    ]
}
