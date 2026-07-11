# Turfzone Frontend Authentication Specification

Status: Proposed

Audience: frontend, backend, platform, and security engineers

## 1. Purpose

Define a security-first authentication integration between a Next.js App Router frontend and the Turfzone Rust/Umbral backend.

The frontend must support:

- username and password registration and login
- Google login through `umbral-oauth`
- one local `AuthUser` identity regardless of login method
- server-managed browser sessions
- role-aware authorization enforced by the Rust backend
- CSRF protection for every cookie-authenticated mutation
- safe logout, password reset, account linking, and error handling

This document is the implementation contract. Do not substitute Auth.js, browser-stored bearer tokens, or a second frontend session system without replacing this decision explicitly.

## 2. Decision

Use Umbral as the only authentication authority.

```text
Username/password ----+
                      +--> AuthUser --> Umbral DB session --> HttpOnly cookie
Google OAuth ----------+
```

The Next.js application is a same-origin UI and proxy. The browser never stores an Umbral bearer token. Both login methods establish the same `umbral_session` cookie and resolve the current user through `GET /api/auth/me`.

## 3. Goals

- Keep credentials and application bearer tokens out of browser JavaScript.
- Use one user record, authorization model, and session lifecycle.
- Prevent CSRF, session fixation, open redirects, user enumeration, and accidental token disclosure.
- Keep authentication behavior consistent between Server Components and Client Components.
- Make backend authorization authoritative even when the frontend performs optimistic route checks.
- Preserve a future path for native mobile authentication without weakening browser security.

## 4. Non-Goals

- Native mobile access and refresh tokens.
- Third-party OAuth clients for the Turfzone API.
- Calling Google APIs such as Calendar or Drive.
- Storing Google access or refresh tokens in the frontend.
- Using Auth.js, NextAuth, Clerk, Firebase Auth, or another parallel identity system.
- Making generated Umbral REST routes public.

## 5. Current Backend Blockers

Frontend implementation must not begin production integration until these are resolved.

### 5.1 Custom Routes Accept Only Bearer Authentication

`src/authz.rs` currently calls `BearerAuthentication` directly. Google OAuth creates a cookie session, but booking, payment, wallet, manager, and admin handlers will not recognize that session.

Required change:

- resolve identity from a valid Umbral cookie session for browser requests
- continue supporting bearer authentication only where a non-browser client requires it
- run both identity types through the same role and subject-ownership policy
- reject conflicting cookie and bearer identities rather than guessing which one wins
- add integration tests for player, manager, staff, superuser, missing session, expired session, and subject mismatch

### 5.2 `/api` Is Globally CSRF-Exempt

`src/main.rs` currently configures:

```rust
csrf_exempt_paths: vec!["/api".to_owned()]
```

That exemption is incompatible with cookie-authenticated API mutations. A malicious site could cause a logged-in browser to submit state-changing requests.

Required change:

- remove the blanket `/api` exemption
- require signed double-submit CSRF validation for browser mutations
- bind CSRF tokens to `umbral_session` with `session_bind_cookie`
- move the Paystack provider callback to a dedicated path such as `/webhooks/paystack`
- exempt only the dedicated provider webhook path
- keep admin/test wrappers under CSRF protection
- do not exempt an entire prefix shared by cookie-authenticated routes

Umbral `0.0.6` path exemptions include descendants. Exempting `/api/payments/webhook` would also exempt `/api/payments/webhook/verify`, so a dedicated webhook path is safer.

### 5.3 Bearer Tokens Are Non-Expiring

The password login response includes a permanent Umbral bearer token. The web frontend must ignore it completely.

Required frontend behavior:

- never persist the response token
- never return it from a Server Component
- never place it in React state, Zustand, cookies, local storage, session storage, URLs, logs, analytics, or error reports
- authenticate browser requests only with the session cookie

### 5.4 Email Verification Delivery Configuration

The backend now enables `.require_verified_email()` and installs the Resend `AuthMailer` when configured. Password users cannot log in before verification.

Deployment requirements:

- set `RESEND_API_KEY` and `RESEND_FROM_EMAIL` outside Dev/Test
- optionally set `RESEND_REPLY_TO`
- verify the sender domain in Resend
- monitor provider acceptance, bounce, complaint, and delivery events
- keep verification and reset secrets out of application logs
- use generic responses for resend and forgot-password requests
- move the built-in process-local throttle to a shared production rate limiter

Dev/Test intentionally falls back to `ConsoleMailer`. Non-Dev startup fails if Resend is absent, and partial Resend configuration fails in every environment.

Email verification is a registration step, not an email OTP login method. Users still sign in with username/password after verifying their email.

### 5.5 Web Identity Requires Email

Umbral's built-in registration requires `username`, `email`, and `password`, while the original Turfzone product decision describes player email as optional and phone as primary.

This specification proposes the following web policy:

- email is required for every web authentication account
- phone remains required for the Turfzone player profile and operational communication
- phone onboarding happens after authentication
- phone-only authentication is a separate future feature and must not use fake email addresses

Product approval is required before implementation because this changes the meaning of "email optional" for web users.

### 5.6 Account Management Gaps

The complete experience also requires explicit application behavior not covered by the default routes:

- Google-first users need a safe set-username/set-password flow if they want password login later
- changing an email requires re-verification before replacing the trusted address
- users need a revoke-other-sessions action for account recovery
- disconnecting Google must not remove the user's only login method
- account deletion must revoke sessions, bearer tokens, OAuth links, and provider tokens

Do not emulate set-password by asking a Google-only user for a current password they never created.

## 6. Deployment Topology

### 6.1 Browser-Facing Origin

The browser must use one origin for the frontend, API, and OAuth entry points.

Development:

```text
Browser origin:       http://localhost:3000
Next.js:              http://localhost:3000
Rust internal origin: http://127.0.0.1:8000
```

Production example:

```text
Browser origin:       https://app.turfzone.co.ke
Next.js:              internal frontend service
Rust:                 internal backend service
```

The edge proxy must route browser-facing `/api`, `/oauth`, `/healthz`, and `/ready` requests to the Rust backend while serving all other routes from Next.js.

Do not expose a second browser-facing API origin unless there is a reviewed reason to accept CORS and cross-origin cookie complexity.

### 6.2 Next.js Development Rewrites

Use rewrites for local development:

```ts
import type { NextConfig } from "next";

const backendUrl =
  process.env.TURFZONE_BACKEND_URL ?? "http://127.0.0.1:8000";

const nextConfig: NextConfig = {
  async rewrites() {
    return [
      {
        source: "/api/:path*",
        destination: `${backendUrl}/api/:path*`,
      },
      {
        source: "/oauth/:path*",
        destination: `${backendUrl}/oauth/:path*`,
      },
      {
        source: "/healthz",
        destination: `${backendUrl}/healthz`,
      },
      {
        source: "/ready",
        destination: `${backendUrl}/ready`,
      },
    ];
  },
};

export default nextConfig;
```

`TURFZONE_BACKEND_URL` is server-only. Do not prefix it with `NEXT_PUBLIC_`.

Production should prefer explicit ingress or reverse-proxy routing over depending on development rewrites.

## 7. Environment Configuration

### 7.1 Next.js

```env
TURFZONE_BACKEND_URL=http://127.0.0.1:8000
```

No Google client secret, mask private key, Umbral secret key, or database credential belongs in the frontend environment.

### 7.2 Rust Backend For Local Frontend Development

```env
UMBRAL_OAUTH_PUBLIC_ORIGIN=http://localhost:3000
UMBRAL_OAUTH_LOGIN_REDIRECT=/dashboard
UMBRAL_OAUTH_GOOGLE_CLIENT_ID=...
UMBRAL_OAUTH_GOOGLE_CLIENT_SECRET=...
UMBRAL_MASK_PUBLIC_KEY=...
UMBRAL_MASK_PRIVATE_KEY=...
```

The Google Cloud OAuth client must register this exact redirect URI:

```text
http://localhost:3000/oauth/google/callback
```

Use `localhost` consistently for browser-facing URLs. `localhost` and `127.0.0.1` are different cookie hosts and different Google redirect URIs.

Production must use HTTPS and the exact production callback:

```text
https://app.turfzone.co.ke/oauth/google/callback
```

## 8. Cookie Policy

### 8.1 Session Cookie

Umbral manages `umbral_session`.

Required properties:

| Property | Requirement |
|---|---|
| `HttpOnly` | Required |
| `Secure` | Required in production |
| `SameSite` | `Lax` |
| `Path` | `/` |
| `Domain` | Omit; use a host-only cookie |
| Lifetime | Fixed 14-day default for initial launch |

Do not read or write `umbral_session` from frontend JavaScript.

Do not enable sliding expiry until its browser-cookie and server-row behavior has an integration test. A fixed expiration is easier to reason about and limits persistent compromise.

### 8.2 CSRF Cookie

Umbral manages `umbral_csrf_token`. It is intentionally readable by JavaScript.

For every `POST`, `PUT`, `PATCH`, and `DELETE` authenticated by cookie:

```http
Cookie: umbral_csrf_token=<signed-token>
X-CSRF-Token: <same-signed-token>
```

The token is signed with `UMBRAL_SECRET_KEY`. Configure `session_bind_cookie` as `umbral_session` so a token minted for one session cannot be replayed under another.

## 9. Frontend Module Layout

Recommended App Router structure:

```text
app/
  (auth)/
    login/page.tsx
    register/page.tsx
    forgot-password/page.tsx
    reset-password/page.tsx
  (protected)/
    dashboard/page.tsx
    layout.tsx
  api-health/route.ts
components/
  auth/
    login-form.tsx
    register-form.tsx
    google-login-button.tsx
    logout-button.tsx
lib/
  auth/
    csrf.ts
    server-session.ts
    types.ts
  api/
    browser.ts
    server.ts
proxy.ts
```

Use `middleware.ts` instead of `proxy.ts` only when the selected Next.js version predates the rename.

## 10. API Client Requirements

### 10.1 CSRF Helper

```ts
const CSRF_COOKIE = "umbral_csrf_token";

export function readCsrfToken(): string | null {
  const prefix = `${CSRF_COOKIE}=`;

  for (const part of document.cookie.split(";")) {
    const value = part.trim();

    if (value.startsWith(prefix)) {
      return decodeURIComponent(value.slice(prefix.length));
    }
  }

  return null;
}
```

### 10.2 Browser Client

```ts
import { readCsrfToken } from "@/lib/auth/csrf";

const WRITE_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);

export async function apiFetch(
  path: string,
  init: RequestInit = {},
): Promise<Response> {
  if (!path.startsWith("/")) {
    throw new Error("API paths must be root-relative");
  }

  const method = (init.method ?? "GET").toUpperCase();
  const headers = new Headers(init.headers);

  headers.set("Accept", "application/json");

  if (WRITE_METHODS.has(method)) {
    const csrfToken = readCsrfToken();

    if (!csrfToken) {
      throw new Error("CSRF token is unavailable");
    }

    headers.set("X-CSRF-Token", csrfToken);
  }

  return fetch(path, {
    ...init,
    method,
    headers,
    credentials: "same-origin",
    cache: "no-store",
  });
}
```

The helper must not accept arbitrary absolute URLs. Authentication cookies should only accompany same-origin requests.

### 10.3 CSRF Bootstrap

A safe backend request mints or rotates the CSRF cookie. Before enabling a mutation form, call a safe route such as:

```ts
await fetch("/oauth/providers", {
  credentials: "same-origin",
  cache: "no-store",
});
```

After login or Google callback, perform another safe backend request before the first mutation. This rotates a session-bound CSRF token after the authenticated session cookie changes.

The UI must disable submission while CSRF bootstrap is pending and provide a retry state if it fails.

### 10.4 Server-Side Client

Server Components must forward the incoming cookie explicitly when calling the internal Rust origin:

```ts
import { cookies } from "next/headers";

export async function serverApiFetch(
  path: string,
  init: RequestInit = {},
): Promise<Response> {
  const cookieStore = await cookies();

  return fetch(`${process.env.TURFZONE_BACKEND_URL}${path}`, {
    ...init,
    headers: {
      ...Object.fromEntries(new Headers(init.headers)),
      Cookie: cookieStore.toString(),
    },
    cache: "no-store",
  });
}
```

Only use the server helper with fixed application paths. Never construct the destination from user-controlled input.

## 11. Authentication State Machine

```text
Anonymous
  | register(username, email, password)
  v
RegisteredUnverified
  | verify six-digit email code
  v
VerifiedUnauthenticated
  | login(username, password)
  v
AuthenticatedNeedsProfile
  | complete required phone/profile onboarding
  v
AuthenticatedReady

Anonymous
  | Google OAuth with verified email
  v
AuthenticatedNeedsProfile or AuthenticatedReady

Authenticated*
  | logout, session expiry, suspension, or password reset
  v
Anonymous
```

Authentication state and product onboarding state are separate. A valid `AuthUser` session does not imply that a required `UserProfile`, phone number, manager profile, or settlement setup exists.

Recommended frontend state:

| State | Meaning | Allowed destination |
|---|---|---|
| `anonymous` | No valid session | Public pages and auth pages |
| `email_verification_required` | Password registration exists but email is unverified | Verification and resend |
| `authenticated_needs_profile` | Valid session but required Turfzone profile fields are missing | Onboarding only |
| `authenticated_ready` | Valid session and required profile exists | Role-appropriate application |
| `suspended` | Identity exists but backend denies application access | Support/suspension page |

## 12. Authentication Flows

### 12.1 Password Registration

1. Render `/register` without leaking whether an email already exists.
2. Bootstrap CSRF with a safe backend request.
3. Normalize presentation whitespace but let the backend perform authoritative username/email normalization.
4. Submit `POST /api/auth/register` with `X-CSRF-Token`:

```json
{
  "username": "player1",
  "email": "player@example.com",
  "password": "user-supplied-password"
}
```

5. Treat `201 Created` as account creation, not login; registration does not establish an authenticated session.
6. Navigate to `/verify-email` and show the destination address in redacted form.
7. Do not put the email, password, or verification code in a query string.
8. Complete phone/profile onboarding only after verification and login.

Password requirements must be rendered from one shared product policy and enforced independently by the backend. The frontend may offer strength guidance but must not claim acceptance before the backend responds.

When `.require_verified_email()` is enabled, registration starts verification automatically. Mail delivery is best-effort in Umbral `0.0.6`; registration may still return `201` when delivery fails, so the verification page must always provide resend and support paths.

### 12.2 Registration Email Verification

Email verification proves ownership of the registration email. It does not authenticate future sessions and is not passwordless email login.

Normal flow:

```text
register
  -> email six-digit code
  -> verify code once
  -> email_verified_at is set
  -> proceed to username/password login
```

Challenge behavior in Umbral `0.0.6`:

| Property | Value |
|---|---|
| Code | Zero-padded six digits |
| Lifetime | 15 minutes |
| Failed-attempt limit | 5 |
| Storage | SHA-256 digest, not plaintext |
| Reuse | Single-use |
| Success | Sets `auth_user.email_verified_at` transactionally |

Verification request:

```http
POST /api/auth/verify-email
Content-Type: application/json
X-CSRF-Token: <signed-token>
```

```json
{
  "email": "player@example.com",
  "code": "483920"
}
```

Frontend behavior:

- use a six-digit numeric input with paste support
- keep the email editable in case navigation state was lost
- never persist the code
- submit only when all six digits are present
- treat `204 No Content` as success
- show one generic invalid-or-expired message for `400 invalid_code`
- do not reveal remaining server-side attempts
- return the user to login after successful verification

### 12.3 Resend Verification

Request:

```json
{
  "email": "player@example.com"
}
```

Submit to `POST /api/auth/resend-verification` with CSRF. The endpoint returns `202 Accepted` for unknown, verified, and unverified addresses to prevent account enumeration.

Frontend behavior:

- always show the same acknowledgement message
- enforce a visible client cooldown without treating it as the security control
- handle `429` with a retry-later state
- never say whether an account exists or is already verified
- explain that only the newest verification code should be used
- provide a support path after repeated delivery failures

The backend currently allows five email-action requests per hour per IP+email in each process. Production requires a shared limiter across replicas.

### 12.4 Password Login

1. Bootstrap CSRF.
2. Submit `POST /api/auth/login` with username, password, and `X-CSRF-Token`:

```json
{
  "username": "player1",
  "password": "user-supplied-password"
}
```

3. Allow the browser to accept `Set-Cookie: umbral_session=...`.
4. Ignore the bearer token in the JSON response.
5. Fetch `GET /api/auth/me` to verify the session and rotate session-bound CSRF.
6. Fetch the Turfzone profile/onboarding status.
7. Navigate to onboarding or a validated same-origin destination, defaulting to `/dashboard`.

Response handling:

| Response | Frontend behavior |
|---|---|
| `200` | Verify session, then route by onboarding/role state |
| `401` | Show generic invalid-credentials message |
| `403 email_not_verified` | Navigate to verification and offer resend |
| `429` | Show retry-later state |

Do not reveal whether the username exists, the password is wrong, or an account also has Google linked.

### 12.5 Google Login

Use top-level browser navigation:

```tsx
export function GoogleLoginButton() {
  return <a href="/oauth/google/login">Continue with Google</a>;
}
```

Do not start OAuth with `fetch`, a popup, or a client-side Google SDK.

Umbral owns:

- state generation and validation
- PKCE verifier and challenge
- Google authorization-code exchange
- verified-email account linking
- encrypted provider-token persistence
- local `AuthUser` creation or lookup
- session creation
- callback redirect

After redirect to `/dashboard`, fetch `GET /api/auth/me`, rotate CSRF through the safe request, and render authenticated state from the returned user.

Google is a login method, not a second Turfzone account type. Google must return a verified email before email-based auto-linking is allowed.

### 12.6 Account Linking And Adding A Login Method

Expected behavior:

- an existing password user may sign in with Google when Google returns the same verified email
- Umbral links the provider identity to the existing local user
- Google `sub`, represented by `provider_uid`, remains the stable external identifier
- an unverified Google email must never auto-link an existing account
- a logged-in user may intentionally use `/oauth/google/connect`
- disconnect must require an authenticated session and valid CSRF token
- disconnect must be blocked if it would leave the user with no usable login method

The final disconnect safeguard requires an application-level policy check if Umbral does not enforce it.

Supported transitions:

| Starting account | User action | Required result |
|---|---|---|
| Password account | Sign in with matching verified Google email | Link Google to the existing `AuthUser` |
| Logged-in password account | Connect Google | Link after state/PKCE and verified identity checks |
| Google-only account | Add password | Custom authenticated set-username/set-password flow |
| Account with password and Google | Disconnect Google | Allowed after reauthentication and CSRF |
| Google-only account | Disconnect Google | Rejected until another login method exists |

Do not ask a Google-only user for a current password. Require recent Google reauthentication before adding a password or changing a trusted email.

### 12.7 Current User And Onboarding

`GET /api/auth/me` is the only frontend source of truth for authentication state.

The frontend may cache display data in memory for rendering, but it must revalidate after:

- login
- logout
- Google callback
- password change
- profile or role change
- a `401` response

Do not infer authentication from the presence of a non-HttpOnly cookie, local storage value, URL parameter, or client state alone.

The current-user response should be complemented by an application bootstrap response containing only frontend-safe data:

```json
{
  "user": {
    "id": 42,
    "username": "player1",
    "email": "player@example.com",
    "email_verified": true
  },
  "profile": {
    "role": "player",
    "phone_complete": true,
    "onboarding_complete": true
  }
}
```

Do not include password hashes, session IDs, OAuth provider tokens, bearer tokens, challenge rows, or privileged fields not needed by the UI.

### 12.8 Logout

1. Read the current CSRF token.
2. Submit `POST /api/auth/logout` with `X-CSRF-Token`.
3. Clear all in-memory user state regardless of whether the response body is available.
4. Navigate to `/login`.
5. Revalidate protected layouts.

Logout clears the current cookie session. The frontend must not claim to have logged out every device unless the backend explicitly revokes all user sessions.

### 12.9 Forgot Password

Submit `POST /api/auth/password-forgot`:

```json
{
  "email": "player@example.com"
}
```

The endpoint returns `202 Accepted` whether the account exists, delivery succeeds, or delivery fails.

Frontend behavior:

- always show "If an account exists, we sent reset instructions"
- never reveal whether the email exists
- handle `429` without changing the generic account message
- provide a safe return to login
- do not continuously poll for delivery

The reset email contains a high-entropy, single-use token valid for one hour and points to `/auth/reset?token=...` on the browser-facing frontend origin.

### 12.10 Reset Password

1. Render a minimal reset page with no third-party scripts or resources.
2. Read the token once and remove it from the visible URL with `history.replaceState`.
3. Keep the token only in page memory.
4. Submit `POST /api/auth/password-reset` with the token, new password, and CSRF.
5. Treat invalid, expired, used, and replayed tokens as one generic failure.
6. On `204`, clear local auth state and navigate to login.

Successful reset must consume the challenge and revoke all user sessions and bearer tokens. The user must authenticate again.

### 12.11 Change Password

Authenticated users submit current and new passwords to `POST /api/auth/change-password` with CSRF.

Security requirements:

- require recent authentication for high-risk accounts
- show a generic failure for an incorrect current password
- notify the account email after a successful change
- offer an explicit "sign out other devices" option
- define whether a password change revokes other sessions

Umbral `0.0.6` does not revoke existing sessions or bearer tokens during ordinary password change. Turfzone must choose and test a stricter application policy before production.

### 12.12 Session Expiry And Revocation

- The initial web session has a fixed 14-day lifetime.
- An expired, deleted, or revoked session returns `401`.
- The frontend clears in-memory user data and redirects to `/login` on `401`.
- The frontend must preserve an intended destination only when it is a validated same-origin path.
- Account suspension must deny protected routes even if the session row still exists.
- Password reset revokes all sessions; ordinary logout revokes only the active session.

Do not silently refresh an expired session with a permanent bearer token.

### 12.13 Email Delivery Contract

Turfzone installs its Resend `AuthMailer` implementation on the Umbral auth plugin:

```rust
AuthPlugin::<AuthUser>::default()
    .with_default_routes()
    .require_verified_email()
    .mailer(resend_mailer)
```

The adapter delivers Umbral's `OutgoingMail` through Resend's `POST /emails` API. It forwards the recipient, subject, HTML body, and text body, supports a reply-to address, applies a hashed idempotency key, uses a ten-second request timeout, and does not surface provider response bodies.

Required production configuration:

- provider API key in the backend secret store
- verified sending domain
- SPF and DKIM records
- DMARC policy and monitored reporting address
- stable `From` name and address
- support/reply-to address
- frontend browser origin used in reset links
- provider webhook secret for delivery, bounce, and complaint events

Environment behavior:

| Environment | Mailer behavior |
|---|---|
| Local development | Explicit console or sandbox inbox; secrets may appear only in local developer output |
| Automated tests | Fake mailer with captured messages and no network |
| Staging | Provider sandbox or restricted recipient allowlist |
| Production | Resend; startup fails if provider configuration is absent |

Email templates must include:

- Turfzone identity and support contact
- reason for the email
- code or reset action
- exact expiry period
- ignore-this-message guidance
- plain-text and HTML alternatives
- no sensitive account details beyond the destination address

Google OAuth registration does not send a Turfzone verification code when Google supplies a trusted verified email. The callback and account-linking integration must test that this satisfies the application's verified-email policy.

## 13. Route Protection

### 13.1 Authorized Request Pipeline

Every protected application route follows one pipeline:

```text
Browser request
  -> same-origin Next.js proxy
  -> Umbral security middleware
  -> CSRF validation for write methods
  -> session identity resolution
  -> UserProfile role lookup
  -> route policy and object-ownership checks
  -> workflow handler
```

Example player request:

```http
POST /api/bookings/hold
Cookie: umbral_session=<http-only-session>; umbral_csrf_token=<signed-token>
X-CSRF-Token: <signed-token>
Content-Type: application/json
```

The frontend does not add an `Authorization` header. The browser supplies the session cookie, the frontend copies the readable CSRF cookie into `X-CSRF-Token`, and the backend resolves the session to an `AuthUser`.

The shared identity resolver must then apply the existing route policies:

| Route class | Required backend decision |
|---|---|
| Public discovery and availability | No user required |
| Player workflows | Authenticated player or superuser |
| Player-owned writes | Authenticated user ID must equal the request subject |
| Manager workflows | Manager, active manager staff, or superuser |
| Admin workflows | `AuthUser.is_superuser`; profile role alone is insufficient |
| Paystack webhook | Valid Paystack HMAC; no browser session |

Authentication and authorization remain separate. A valid session answers who the user is; role, membership, ownership, and object state determine what that user may do.

If a request contains both a session cookie and bearer token, the backend must reject it when they resolve to different users. It may accept matching identities, but browser code should never send both.

Return `401` when no valid identity exists and `403` when an authenticated identity lacks permission.

### 13.2 Generated REST Routes

The current `RestPlugin` uses `BearerAuthentication` and `StaffOrSuperuserReadOnly`. These generated routes are not the public frontend API and must not be used by player or manager UI.

Required approach:

- expose dedicated application endpoints for player and manager reads
- return only fields intended for that audience
- enforce object-level ownership and active membership
- keep generated REST limited to internal staff diagnostics until it supports the reviewed session policy

If a staff web console later needs generated REST, add a reviewed composite session/bearer authenticator to the plugin or create dedicated session-authenticated admin projections. Do not place a permanent service bearer token in Next.js client code.

### 13.3 Backend Is Authoritative

Every protected Rust route must authenticate and authorize independently. A Next.js redirect is user experience, not a security boundary.

### 13.4 Next.js Protected Layout

Protected layouts should call `GET /api/auth/me` server-side and redirect unauthenticated users:

```tsx
import { redirect } from "next/navigation";
import { serverApiFetch } from "@/lib/api/server";

export default async function ProtectedLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  const response = await serverApiFetch("/api/auth/me");

  if (response.status === 401) {
    redirect("/login");
  }

  if (!response.ok) {
    throw new Error("Unable to verify the current session");
  }

  return children;
}
```

Use `proxy.ts` only for an optimistic cookie-presence redirect. Do not perform database-backed authorization in proxy middleware, and do not treat cookie presence as proof of a valid session.

## 14. Authorization And Roles

The frontend may hide unavailable actions, but the Rust backend must enforce all role and ownership rules.

| Role | Frontend behavior |
|---|---|
| Player | Player booking and wallet surfaces |
| Manager | Manager venue, field, settlement, and staff surfaces |
| Manager staff | Only granted manager operational surfaces |
| Staff | Read-only generated REST/admin surfaces where authorized |
| Superuser | Platform administration surfaces |

Do not authorize from an editable client role value. Always use the role returned by an authenticated backend response and expect the backend to reject stale or forged actions.

## 15. Error Handling

- `400`: show safe field or workflow validation errors.
- `401`: clear in-memory auth state and redirect to login.
- `403` with CSRF failure: refresh CSRF through a safe request and allow one explicit user retry; do not loop automatically.
- `403` authorization failure: show an access-denied state without changing identity.
- `409`: show a conflict message appropriate to registration or booking.
- `429`: preserve retry timing and show a rate-limit message.
- `5xx`: show a generic retry state and attach a correlation ID when provided.

Never render raw backend errors containing SQL, token, provider, or infrastructure details.

## 16. Security Headers

Production responses should include:

- HSTS after HTTPS is stable on all required subdomains
- a tested Content Security Policy
- `frame-ancestors 'none'`
- `base-uri 'self'`
- `form-action 'self'` plus only provider destinations genuinely required
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: strict-origin-when-cross-origin`
- `Cross-Origin-Opener-Policy: same-origin` unless a reviewed OAuth popup flow requires otherwise
- a restrictive `Permissions-Policy`

OAuth uses full-page redirects, so it does not require loosening `frame-src` or popup policy.

Next.js and Rust headers must be tested together at the browser-facing origin. Avoid contradictory duplicate CSP headers.

## 17. Logging And Telemetry

Never log:

- passwords
- session IDs
- CSRF tokens
- Umbral bearer tokens
- Google authorization codes
- Google access or refresh tokens
- reset or verification tokens
- complete `Cookie`, `Set-Cookie`, or `Authorization` headers

Permitted audit fields include:

- internal user ID
- provider name
- flow name
- success or failure category
- timestamp
- request correlation ID
- coarse client metadata after privacy review

Authentication failure logs must not include submitted credentials or raw provider responses.

## 18. Rate Limiting

Apply backend rate limits to:

- registration
- password login
- password reset request and completion
- email verification resend
- OAuth login initiation
- OAuth callback failures

Use both IP-based and account-identifier-based controls where appropriate. Normalize identifiers before rate-limit keys. Do not let a distributed attacker lock out a victim indefinitely.

## 19. Testing Requirements

### 19.1 Backend Integration Tests

- Password registration creates an unverified user and sends one verification message.
- Password login rejects an unverified user with `email_not_verified`.
- Verification succeeds once, sets `email_verified_at`, and then permits login.
- Verification fails for wrong, expired, exhausted, and replayed codes.
- Resend returns the same `202` for unknown, verified, and unverified emails.
- Resend produces a new code without exposing account state.
- Production startup rejects a missing real mailer configuration.
- Forgot-password returns the same `202` regardless of account existence.
- Password-reset tokens expire after one hour and are single-use.
- Password login sets a valid `HttpOnly` session cookie.
- Google callback sets the same session cookie type.
- Cookie session authorizes player workflow routes.
- Expired and deleted sessions return `401`.
- Every cookie-authenticated mutation fails without CSRF.
- Every cookie-authenticated mutation fails with mismatched CSRF.
- A valid signed, session-bound CSRF token succeeds.
- Paystack webhook succeeds without CSRF only when HMAC is valid.
- Google state and PKCE failures are rejected.
- Verified-email linking does not create duplicate users.
- Unverified-email linking is rejected.
- Logout destroys only the active session.
- Password reset revokes all applicable sessions and tokens.

### 19.2 Frontend Unit Tests

- Registration transitions to verification rather than authenticated state.
- Verification code input handles paste, completion, invalid, and expired states.
- Resend cooldown does not reveal account state.
- Login routes `email_not_verified` to verification.
- Password-reset tokens are removed from the visible URL.
- CSRF cookie parser handles missing and encoded values.
- API client adds CSRF only to write methods.
- API client rejects absolute destinations.
- Login ignores bearer tokens in response JSON.
- Auth state clears on `401`.
- Redirect destinations remain same-origin.

### 19.3 Browser Tests

- Register, receive a captured email, verify, log in, refresh, and remain authenticated.
- Confirm an unverified password account cannot log in.
- Resend verification and complete verification with the newest code.
- Request and complete password reset, then confirm old sessions are invalid.
- Log in with Google and reach `/dashboard`.
- Use password and Google on the same verified email and reach one account.
- Submit a protected mutation with valid CSRF.
- Confirm a forged cross-site form cannot mutate data.
- Log out and confirm protected routes redirect.
- Expire the session server-side and confirm the next request redirects.
- Confirm no auth token appears in local storage, session storage, URL, page source, or client-visible session data.
- Confirm cookies are `Secure` in production and the session cookie is `HttpOnly`.

## 20. Implementation Phases

### Phase 1: Backend Security Prerequisites

- Add session authentication to custom route identity resolution.
- Remove blanket `/api` CSRF exemption.
- Bind signed CSRF to `umbral_session`.
- Isolate the Paystack webhook path.
- Require verified email for password accounts.
- Install and configure the production transactional mailer.
- Add fake/sandbox mailers for test, development, and staging.
- Decide and document the web email-required product policy.
- Add backend integration tests.

### Phase 2: Same-Origin Infrastructure

- Add Next.js development rewrites.
- Configure production ingress routes.
- Change OAuth public origin to the frontend origin.
- Update the Google callback URI.
- Verify cookie attributes through the proxy.

### Phase 3: Frontend Authentication

- Implement CSRF bootstrap and API clients.
- Implement register and password login.
- Implement Google login navigation.
- Implement current-user resolution and protected layouts.
- Implement logout, reset, verification, and error states.

### Phase 4: Hardening

- Add rate limits.
- Add production CSP and HSTS.
- Add security telemetry and redaction tests.
- Run browser security tests.
- Perform session and account-linking threat review.

## 21. Acceptance Criteria

- Both login methods resolve to one Umbral `AuthUser` when identities match safely.
- Both login methods establish the same server-side session mechanism.
- No application or provider token is available to browser JavaScript.
- No auth token is stored in local storage or session storage.
- Protected Rust routes accept valid sessions and enforce roles and ownership.
- Every cookie-authenticated mutation requires valid signed, session-bound CSRF.
- Paystack remains authenticated by HMAC, not CSRF or browser session.
- Frontend and backend use one browser-facing HTTPS origin in production.
- Logout invalidates the current session.
- Expired sessions cannot access protected data.
- Google callback, account linking, password reset, and CSRF behavior have automated integration coverage.
- Password registration requires successful email verification before login.
- Verification and reset messages are delivered by a monitored transactional provider.
- Unknown email, resend, and forgot-password responses do not reveal account existence.
- Google verified-email handling satisfies the same account trust policy without duplicate accounts.
- Production cookies and security headers pass browser inspection.

## 22. References

- `docs/API.md`
- `docs/ARCHITECTURE.md`
- `src/authz.rs`
- `src/main.rs`
- `umbral-auth 0.0.6`
- `umbral-oauth 0.0.6`
- `umbral-security 0.0.6`
- `umbral-sessions 0.0.6`
