# Security Audit Baseline

This file records the current `cargo audit` decisions for the workspace.

## Fixed In This Lane

- `rustls-webpki 0.102.8` was removed from the active dependency graph by moving `dregg-discord-bot` from Serenity's `rustls_backend` to `native_tls_backend`. Serenity `0.12.5` is the latest published Serenity release, so there was no leaf version bump available for its old Rustls chain.

## Temporary Explicit Ignore

- `RUSTSEC-2025-0055` (`tracing-subscriber 0.2.25`) remains in the graph through `ark-relations 0.5.1`, pulled by the `dregg-hints` arkworks `0.5` proof stack.

Rationale:

- This is not a direct runtime dependency or the workspace's primary logging subscriber.
- The narrow upgrade path is not a leaf patch; it requires moving the arkworks stack to `0.6` or splitting the hints dependency features so `ark-relations/std` no longer pulls the old subscriber.
- The ignore is scoped in `.cargo/audit.toml` and should be removed when that migration lands.

## Follow-Up

1. Test an arkworks `0.6` migration for `hints` in a separate branch.
2. If that is too large, prototype a no-std feature split for the `hints` dependency chain.
3. Re-run `cargo audit` and remove the advisory ignore once `tracing-subscriber 0.2.25` leaves `Cargo.lock`.
