# Operations

This page is for commands and environment constraints that affect day-to-day
backend-runtime work.

## Rust Toolchain

The repo pins nightly in [../../rust-toolchain.toml](../../rust-toolchain.toml).
Use the pinned toolchain unless a task explicitly requires a different one.

## Local Build Pattern

Prefer narrow checks:

```powershell
.\scripts\dev-check.ps1
.\scripts\test-fast.ps1
```

Avoid casual full-workspace checks while iterating. They are expensive and can
obscure the specific subsystem you are changing.

For explicit package checks, use Cargo directly:

```bash
cargo check -p dregg-cli
cargo check -p dregg-node
cargo test -p dregg-turn --lib
```

The node check is heavier than the CLI check; run it when changing node/runtime
wire behavior, not as the default edit loop.

## Documentation Check

Validate local Markdown links after moving docs:

```powershell
.\scripts\docs-check.ps1
```

The checker covers root entry files, `apps/README.md`, and the full `docs/`
tree.

## Package Metadata Report

Inspect workspace package metadata quality:

```powershell
.\scripts\workspace-package-report.ps1
```

Use this before package cleanup lanes to see which crates still lack
descriptions, licenses, or README metadata.

## Devnet

The Docker devnet entry point is [../../docker/README.md](../../docker/README.md).
It describes the local 4-node federation, node ports, faucet, logs, and reset
flow.

## Preflight

`dregg-preflight` is the intended subsystem gate:

```powershell
.\scripts\preflight.ps1
```

It exercises boot, cells, turns, proofs, privacy, capabilities, intents, apps,
composition, federation, blocklace, CapTP, storage, node, wire, bridges, and
demo-agent examples.

Interpret subsystem confidence through
[../40-testing/preflight-reality.md](../40-testing/preflight-reality.md). A
green preflight is a strong subsystem smoke signal, not automatic closure of
every soundness or adversarial audit item.

Use the Rust test harness form when you specifically want the per-subsystem test
shape:

```powershell
.\scripts\preflight.ps1 -TestHarness
```

## CI

Current workflows live under [../../.github/workflows/](../../.github/workflows/).
Important surfaces:

- `ci.yml` - check, test, clippy, fmt, audit, and auth grep guard.
- `nightly.yml` - feature-specific circuit testing.
- `bench.yml` - manual Criterion benchmarks.

## Offload And Long Builds

[reports/PERSVATI.md](reports/PERSVATI.md) documents the remote build/offload
box.
Use it for expensive workspace-wide validation when appropriate.
