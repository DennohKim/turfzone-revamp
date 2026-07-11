# Turfzone MVP API

## Generated CRUD

Umbral REST exposes registered MVP models under `/api/<table>/`.

Registered models:

- `UserProfile`
- `ManagerProfile`
- `StaffMembership`
- `Venue`
- `Amenity`
- `VenueAmenity`
- `Field`
- `FieldImage`
- `OpeningHours`
- `AvailabilityException`
- `Booking`
- `Payment`
- `Refund`
- `Wallet`
- `WalletTransaction`
- `PaystackSubaccount`
- `Payout`
- `Notification`

## Custom Workflow Routes

These routes exist because generic CRUD cannot safely enforce workflow invariants.

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/healthz` | Health check |
| `GET` | `/ready` | Readiness check from `umbral-health` |
| `POST` | `/api/auth/register` | Register an auth user |
| `POST` | `/api/auth/login` | Create a session and bearer token |
| `POST` | `/api/auth/logout` | Clear the current session |
| `GET` | `/api/auth/me` | Return the authenticated auth user |
| `POST` | `/api/auth/change-password` | Change the authenticated user's password |
| `POST` | `/api/auth/verify-email` | Verify an email challenge code |
| `POST` | `/api/auth/resend-verification` | Resend an email verification challenge |
| `POST` | `/api/auth/password-forgot` | Request a password-reset challenge |
| `POST` | `/api/auth/password-reset` | Reset a password with a challenge token |
| `GET` | `/api/meta` | Runtime/product defaults |
| `GET` | `/api/routes` | Route manifest |
| `POST` | `/api/discovery/search` | Build validated discovery filters |
| `POST` | `/api/fields/availability` | Compute slot availability |
| `POST` | `/api/bookings/hold` | Create a pending booking hold |
| `POST` | `/api/bookings/cancellation-quote` | Quote cancellation eligibility and wallet refund |
| `POST` | `/api/payments/initialize` | Build Paystack M-Pesa/card init payload |
| `POST` | `/api/payments/webhook` | Verify real Paystack webhook signature from header/body |
| `POST` | `/api/payments/webhook/verify` | Test-friendly webhook verification wrapper |
| `POST` | `/api/payments/refund-payload` | Build Paystack refund payload for supported card refunds |
| `POST` | `/api/wallet/simulate` | Exercise wallet ledger behavior |
| `POST` | `/api/manager/subaccount-payload` | Build Paystack subaccount payload |
| `POST` | `/api/admin/managers/verify` | Validate admin manager verification inputs |

## Response Envelope

Custom workflow routes use:

```json
{
  "ok": true,
  "data": {},
  "error": null
}
```

or:

```json
{
  "ok": false,
  "data": null,
  "error": "message"
}
```

## Authentication And Authorization

Protected API routes use Umbral bearer tokens. Obtain a token from `POST /api/auth/login` and send it as:

```http
Authorization: Bearer umbral_<token>
```

Authorization policy:

| Access | Routes |
|---|---|
| Public | Health, readiness, OpenAPI in non-production, metadata, route manifest, discovery, and availability |
| Paystack HMAC | `POST /api/payments/webhook` validates `x-paystack-signature` against the raw request body |
| Player | Booking hold, cancellation quote, and payment initialization |
| Manager or manager staff | Manager subaccount payload |
| Auth superuser | Webhook verification wrapper, refund payload, wallet simulation, and manager verification |
| Auth staff or superuser, read-only | All Umbral-generated model REST routes |

The role gate reads `UserProfile.role`. A newly registered auth user without a profile is treated as a player. Platform admin access requires `AuthUser.is_superuser`; setting `UserProfile.role` to `Admin` is not sufficient. Booking hold also requires `player_id` to equal the authenticated auth-user ID.

Generated REST is intentionally not a public application API. It requires a bearer token, permits reads only to auth staff/superusers, and denies generic create/update/delete operations.

## Production Hardening Still Required

Before launch, custom routes must be connected to real persistence and object-level records:

- transactional DB writes for bookings, payments, refunds, wallet ledger, and payouts
- booking ownership checks for cancellation and payment initialization after those requests use persisted booking IDs
- manager ownership and active staff-membership checks after the subaccount request is tied to a persisted manager ID
- dedicated public venue/field projections instead of exposing generated model REST
