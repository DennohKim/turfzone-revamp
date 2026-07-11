# Turfzone Backend Architecture

## Purpose

This backend serves the Turfzone marketplace: venue discovery, slot availability, booking holds, M-Pesa/card payments, wallet refunds, manager operations, and platform admin workflows.

## Boundaries

### Umbral Boundary

Umbral owns application boot, model registration, generated CRUD, OpenAPI, auth plugin wiring, and route mounting.

Files:

- `src/main.rs`
- `src/models.rs`
- `src/api.rs`

Rule: keep Umbral-specific code at the boundary. Do not put booking, money, payout, or provider invariants inside route handlers or model derives.

### Domain Services

Plain Rust services own business correctness. They are tested without a database or HTTP server.

Files:

- `src/services/availability.rs`
- `src/services/booking.rs`
- `src/services/payment.rs`
- `src/services/wallet.rs`
- `src/services/notification.rs`
- `src/services/payout.rs`

Rule: money and booking invariants must be expressible as unit tests here before route or persistence integration.

### Provider Boundary

External provider payloads and verification live behind provider modules.

Files:

- `src/paystack.rs`

Rule: route handlers can call provider builders/verifiers, but must not hand-build provider JSON inline.

## Persistence Strategy

Umbral REST exposes registered MVP models under `/api/<model>/` for basic CRUD.

Custom workflow routes exist for operations where generic CRUD is unsafe:

- booking hold creation
- cancellation quote/refund decision
- payment initialization
- Paystack webhook verification
- wallet debit/credit behavior
- manager payout destination setup
- admin manager verification

## Beta Framework Strategy

Umbral is beta. Every framework mismatch or unsupported feature goes into `UMBRAL_FRICTION_LOG.md` with:

- observed error
- affected code
- workaround
- risk
- preferred fix
- verification status

## Module Ownership

`main.rs` should remain wiring only. If it grows beyond plugin/model/route mounting, move code into `api`, `services`, or a provider module.

`api.rs` owns request/response DTOs and thin handlers. If a handler needs more than validation plus one service call, create a service function.

`models.rs` owns Umbral schema declarations only. Do not add business methods that depend on current time, payment provider behavior, or request identity.
