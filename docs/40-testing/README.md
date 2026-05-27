# Testing Reality

The testing contract for this repo is: a passing test should mean the runtime
claim it names is actually exercised. If a test is scaffold, slow, blocked, or
synthetic, it must say so clearly.

## Current Truth

Read [../30-audits/tests/TEST-REALITY-AUDIT.md](../30-audits/tests/TEST-REALITY-AUDIT.md)
first. Its headline is the best current summary:

- Primitive unit tests are often real and useful.
- Cross-cutting soundness, adversarial, and multi-node tests are the weak spot.
- Some demos contain real independent verification; others still test fixtures
  they wrote themselves.

## Command Dashboard

Use [dashboard.md](dashboard.md) as the current command/evidence map. It names
the supported local scripts, their evidence level, and the main gap areas that
green output does not yet cover.

## Fast Local Checks

Use narrow checks while iterating:

```bash
cargo check -p <crate>
cargo test -p <crate> --lib
```

Or use the supported wrapper:

```powershell
.\scripts\test-fast.ps1
```

The default wrapper target is deliberately small. For a deeper runtime loop:

```powershell
.\scripts\test-fast.ps1 -Package dregg-cell,dregg-turn,dregg-verifier
```

Use nextest profiles when available:

```bash
cargo nextest run
cargo nextest run --profile full
```

The default nextest profile excludes known slow backend categories. See
[../../.config/nextest.toml](../../.config/nextest.toml).

## Full-Claim Checks

- `.\scripts\preflight.ps1` is intended as the golden-master subsystem check.
  See [../../preflight/src/main.rs](../../preflight/src/main.rs).
- `.\scripts\test-full.ps1` runs the broad local test lane.
- CI currently runs broad workspace check/test/clippy in
  [../../.github/workflows/ci.yml](../../.github/workflows/ci.yml). Local
  developers should use narrower crate checks unless they are intentionally
  validating the whole graph.

## Known Risk Areas

Track these through the audit and debt docs before relying on green output:

- `demo/multi-node-devnet` scenarios have known scaffold-shaped assertions.
- `tests/src/*` contains many ignored threat-model tests with unblock labels.
- Effect VM, sovereign witness, gamma2, and witnessed-predicate paths contain
  high-value adversarial coverage gaps or slow/manual tests.

## Rule For New Tests

Name the behavior you actually exercise. If a test only checks hash inequality,
do not name it as verifier rejection. If it only writes a fixture and reads it
back, mark it scaffold or pending.
