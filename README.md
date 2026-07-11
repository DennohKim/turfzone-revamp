# Turfzone Backend

Rust/Umbral backend for the Turfzone turf and court booking marketplace.

## MVP Decisions Captured

- Launch market: Kenya, KES-only public pricing at launch.
- Wallet is part of MVP for clean M-Pesa refund handling.
- Manager settlement destination is manager-selected: bank account or M-Pesa wallet.
- Default unpaid booking hold window is 7 minutes.
- Cancellation policy is manager-configurable between 2 and 24 hours, default 6 hours, eligible refunds credit the wallet.
- Player identity is phone-primary; player email is optional, manager/admin email is required.

## Run

```bash
cp .env.example .env
cargo run
```

## Validate

```bash
cargo test
cargo check
```

The core booking, wallet, and payment rules are intentionally implemented behind plain Rust service modules. Umbral stays at the application boundary so framework beta gaps do not leak into money/booking correctness.

## Documentation

- `docs/ARCHITECTURE.md` - module boundaries and ownership rules.
- `docs/API.md` - generated CRUD and custom workflow route map.
- `docs/INVARIANTS.md` - booking, wallet, payment, payout, and notification rules.
- `docs/DEVELOPMENT.md` - development workflow and maintainability checklist.
- `UMBRAL_FRICTION_LOG.md` - framework gaps, workarounds, and verification notes.
