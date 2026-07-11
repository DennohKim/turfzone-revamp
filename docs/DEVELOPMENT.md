# Development Notes

## Commands

```bash
cargo fmt
cargo test
cargo check
cargo run -- makemigrations
cargo run -- migrate
cargo run -- tasks-worker --once
```

## Before Editing

- Read `docs/ARCHITECTURE.md` for module boundaries.
- Read `docs/INVARIANTS.md` before touching booking, wallet, or payments.
- Check `UMBRAL_FRICTION_LOG.md` before using less common Umbral model field types or plugin APIs.

## Adding A Workflow

1. Add pure service logic under `src/services/` if the workflow changes state or enforces an invariant.
2. Add tests for failure and idempotency paths.
3. Add request/response DTOs in `src/api.rs` or split `api` into modules if the file becomes large.
4. Wire the route in `src/main.rs` only after the service tests pass.
5. Run `cargo fmt`, `cargo test`, and `cargo check`.
6. Log any Umbral issue in `UMBRAL_FRICTION_LOG.md`.

## Current Known Tradeoffs

- JSON-shaped model fields are temporarily stored as `String` fields with `_json` suffix because Umbral `0.0.1` rejects `serde_json::Value` in models.
- Some workflow routes currently return provider payloads or service decisions rather than performing transactional database writes. The service layer is intentionally ready for DB integration next.
- Umbral REST handles generic CRUD, but workflow routes must enforce identity and object-level permissions before production launch.
