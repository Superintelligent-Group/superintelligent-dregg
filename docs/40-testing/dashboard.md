# Testing Dashboard

This page turns the test audits into an operating dashboard. It does not claim
the whole workspace is sound; it names which command is cheap iteration, which
command is subsystem evidence, and which gaps are still documentary only.

## Command Lanes

| Lane | Command | Use for | Evidence level |
| --- | --- | --- | --- |
| Dev check | `.\scripts\dev-check.ps1` | Docs links, package metadata, format, and a narrow compile check | Hygiene and compile smoke |
| Fast tests | `.\scripts\test-fast.ps1` | Small deterministic unit-test smoke | Useful local regression signal |
| Full tests | `.\scripts\test-full.ps1` | Broad local validation before a large branch handoff | Expensive workspace signal |
| Preflight | `.\scripts\preflight.ps1` | Golden-master subsystem pass across the runtime stack | Highest intended local subsystem gate; interpret with [preflight-reality.md](preflight-reality.md) |
| CI parity | `cargo check --workspace --all-targets`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` | Matching current CI shape | Broad but not complete soundness proof |

## Current Evidence Map

| Surface | Current status | Main audit | Next action |
| --- | --- | --- | --- |
| Primitive predicate tests | Strongest real-test examples in the repo | [TEST-REALITY-AUDIT.md](../30-audits/tests/TEST-REALITY-AUDIT.md) | Keep new predicate tests adversarial and verifier-facing |
| Bridge presentation tests | Good end-to-end proof path coverage | [TEST-REALITY-AUDIT.md](../30-audits/tests/TEST-REALITY-AUDIT.md) | Preserve real mint/attenuate/prove/verify shape |
| Effect VM soundness | Important FRI single-row and verifier-path gaps remain | [IGNORED-TESTS-AUDIT.md](../30-audits/tests/IGNORED-TESTS-AUDIT.md) | Fix FRI parameter/minimum-trace gap before treating green output as soundness proof |
| Executor honesty threats | Mostly future-lane tests, not CI teeth | [EXECUTOR-HONESTY-AUDIT.md](../10-canonical/debt/EXECUTOR-HONESTY-AUDIT.md) | Promote one T* threat at a time into running adversarial tests |
| Sovereign witness threats | Documentary and demo-backed, not fully AIR-backed | [SOVEREIGN-WITNESS-AIR-DESIGN.md](../20-active-design/proofs/SOVEREIGN-WITNESS-AIR-DESIGN.md) | Land AIR teeth, then unignore the threat suite |
| Gamma2 bilateral binding | Future-lane test surface | [STAGE-7-GAMMA-2-PI-DESIGN.md](../20-active-design/proofs/STAGE-7-GAMMA-2-PI-DESIGN.md) | Wire PI offsets and off-AIR sender/receiver join checks |
| Multi-node devnet demos | Some scripts are scaffold-shaped | [TEST-REALITY-AUDIT.md](../30-audits/tests/TEST-REALITY-AUDIT.md) | Mark pending assertions honestly or submit through real node/executor APIs |
| Preflight | Intended golden-master subsystem gate | [preflight/src/main.rs](../../preflight/src/main.rs) | Keep it runnable, bounded, and explicit about scaffolded checks |

For a deeper runtime loop, run:

```powershell
.\scripts\test-fast.ps1 -Package dregg-cell,dregg-turn,dregg-verifier
```

That package set is intentionally opt-in because it can become compile-heavy on
Windows when proof and verifier dependencies are cold.

## Rules For Promoting Tests

1. A test name must match the behavior actually exercised.
2. Rejection tests should enter through the real verifier, executor, AIR, or
   runtime surface whenever that surface exists.
3. A hash inequality assertion is not a verifier rejection test.
4. `#[ignore]` must name the blocker and should be reflected in an audit or debt
   doc.
5. Demo scripts should not emit `true` for assertions that only prove the script
   wrote a fixture.

## Next Test Lanes

| Lane | Target | Acceptance condition |
| --- | --- | --- |
| FRI single-row gap | Effect VM soundness tests | A tampered proof reliably rejects through `verify`, not only direct AIR evaluation |
| Executor T1/T4 | Effects and pre-state AIR binding | One ignored executor-honesty threat becomes a running adversarial test |
| Devnet honesty | Cross-fed handoff / intent scenarios | Scenario submits to the real surface or reports `pending`, not fixture success |
| Preflight reality | `dregg-preflight` checks | Each subsystem check says whether it is real, scaffold, slow, or blocked |
