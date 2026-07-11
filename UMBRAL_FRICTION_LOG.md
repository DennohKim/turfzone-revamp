# Umbral Friction Log

This file tracks framework gaps discovered while building the Turfzone backend with Umbral. Treat each entry as implementation feedback for the beta framework and as a reminder of local workarounds we should revisit.

## 2026-07-09

### `serde_json::Value` model fields are rejected by `#[derive(Model)]`

**Status:** Open

**Where found:** `src/models.rs`

**Compile error:**

```text
error: umbral M3 doesn't yet support this field type; see docs/specs/04-orm-model-and-fields.md for the M3 type catalogue
```

**Fields affected:**

- `ManagerProfile::settlement_details`
- `Booking::cancellation_policy_snapshot`
- `Payment::provider_payload`
- `WalletTransaction::metadata`
- `PaystackSubaccount::payload`

**Why Turfzone needs this:**

These values are naturally JSON-shaped: payout destination details, booking policy snapshots, Paystack payloads, wallet metadata, and Paystack subaccount payloads. They need to be stored as immutable audit data without forcing every provider-specific key into first-class columns.

**Docs mismatch:**

The Turfzone spec and Umbral examples imply `serde_json::Value` can be used for these model fields, but Umbral `0.0.1` rejects them during macro expansion.

**Current workaround:**

Converted these columns to string-encoded JSON fields:

- `settlement_details_json: String`
- `cancellation_policy_snapshot_json: String`
- `provider_payload_json: String`
- `metadata_json: String`
- `payload_json: String`

**Risk:**

String-encoded JSON loses database-level JSON validation/querying unless we add custom validation in services or a custom Postgres migration later.

**Preferred fix:**

Use native JSON/JSONB model field support once Umbral supports it, or add a hand-written migration for JSONB columns and map them through a supported custom field type if the framework exposes one.

**Verification:**

After applying the workaround, `cargo test` passed all 12 tests.

### `umbral_cli::dispatch(app)` error type is stricter than the docs imply

**Status:** Worked around

**Where found:** `src/main.rs`

**Compile error:**

```text
error[E0277]: `?` couldn't convert the error: `dyn std::error::Error + Send + Sync: Sized` is not satisfied
```

**Why Turfzone needs this:**

The backend should expose Umbral management commands through the project binary: `makemigrations`, `migrate`, `createsuperuser`, `tasks-worker`, `tasks-beat`, `clearsessions`, and `collectstatic`.

**Docs mismatch:**

The management-command docs show `umbral_cli::dispatch(app).await` in a generic context, but the project `main` return type must accept `Box<dyn std::error::Error + Send + Sync>` for `?` to compile.

**Current workaround:**

Changed `main` from:

```rust
async fn main() -> Result<(), Box<dyn std::error::Error>>
```

to:

```rust
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
```

**Risk:**

Low. The stricter error bound is normal for async/server code, but the scaffold/docs should show it to avoid copy-paste compile failures.

**Preferred fix:**

Update Umbral docs/scaffold examples to use the stricter return type or expose a dispatch error type that converts cleanly into `Box<dyn Error>`.
