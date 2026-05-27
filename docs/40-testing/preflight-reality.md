# Preflight Reality Map

`dregg-preflight` is the intended golden-master subsystem gate. This page keeps
its meaning explicit: a passing preflight is a broad subsystem smoke signal, not
a blanket proof that every protocol claim is fully enforced.

The binary output prints the same coarse labels beside each subsystem:
`[real]`, `[mixed]`, `[smoke]`, or `[conditional]`.

## Status Legend

| Status | Meaning |
| --- | --- |
| Real | Uses the real subsystem surface and asserts runtime behavior. |
| Mixed | Combines real checks with smoke, feature-conditional, or known partial coverage. |
| Smoke | Confirms construction/wiring/API shape more than adversarial soundness. |
| Conditional | Check is meaningful only when optional features or external context are enabled. |

## Subsystem Map

| Preflight subsystem | Status | What it means when green | Main caution |
| --- | --- | --- | --- |
| Boot | Smoke | Basic runtime boot invariants still construct | Does not exercise distributed operation |
| Cell lifecycle | Real | Cell state and lifecycle primitives behave on the checked path | Not a full predicate/caveat coverage proof |
| Turn execution | Mixed | Turn executor accepts expected happy/adversarial paths in the preflight set | Executor honesty threats remain broader than preflight |
| Proofs | Mixed | Selected STARK, derivation, Effect VM, and IVC paths prove/verify | Effect VM FRI single-row gaps are tracked separately |
| Effect VM | Mixed | Effect VM trace/proof wiring is live for selected cases | Green output does not close all soundness audit findings |
| Privacy | Mixed | Privacy primitives and selected request/proof flows still wire together | Does not cover every credential or anonymity threat model |
| Capabilities | Mixed | Capability grant/revoke/delegation paths work for selected flows | CapTP and cross-fed handoff need their own adversarial coverage |
| Intents | Mixed | Intent matching and privacy-preserving flow smoke checks pass | Cross-fed intent scenarios still have scaffold-shaped demo risk |
| Apps | Smoke | Canonical app helpers and framework paths construct and submit selected actions | App-specific security claims need executor-facing tests |
| Composition | Mixed | Multi-surface composition checks still hold for selected paths | Not a substitute for every cross-subsystem invariant |
| Federation | Mixed | Federation state/root/threshold paths work in local checks | Not a multi-node liveness or byzantine-schedule proof |
| Blocklace | Mixed | DAG/blocklace ordering structures satisfy selected invariants | Does not prove production network convergence |
| Factory & Sovereign | Mixed | Factory and sovereign-cell checks cover selected construction paths | Sovereign witness AIR teeth remain a tracked future lane |
| Cross-backend | Conditional | Available proof backend paths compile and verify selected witnesses | Optional Kimchi/Plonky3/Pickles paths may be skipped by feature gates |
| CapTP | Mixed | Capability transport primitives pass selected checks | Handoff/pipelining adversarial cases need dedicated tests |
| DFA Routing | Real | Canonical DFA routing primitives satisfy checked behavior | Legacy `rbg` routing is not the canonical owner |
| Storage | Mixed | Storage queues/inboxes/relay primitives pass selected checks | Operator and recovery behavior need broader integration coverage |
| Nameservice | Mixed | Nameservice app primitives work in selected checks | Full app/runtime lifecycle should remain executor-facing |
| Relay | Smoke | Relay wiring and basic behavior remain constructible | Not a production network reliability test |
| CLI | Smoke | CLI-facing runtime surfaces remain reachable in selected checks | Does not replace command-level end-to-end tests |
| Node | Smoke | Node state/API/wire wiring passes local checks | Full daemon/API behavior needs node integration tests |
| Wire Protocol | Mixed | Wire framing/server/client behavior passes selected checks | Does not prove all compatibility or adversarial framing cases |
| Solver | Mixed | Solver paths satisfy selected examples | Economic/adversarial solver behavior needs separate coverage |
| Bridges | Mixed | Bridge state/proof paths pass selected checks | Mina/cross-fed bridge claims need explicit integration evidence |
| Demo-Agent Examples | Smoke | Demo-agent examples still compile/run on checked paths | Demo success is not protocol proof unless it calls real verifier/runtime surfaces |
| StateConstraint surface | Real | Cell-side StateConstraint evaluator and canonical id derivations behave as checked | Full caveat correctness remains broader than this gate |

## Operating Rule

When a preflight subsystem moves from smoke to real evidence, update this page
and `preflight/src/report.rs` in the same change. When a check is scaffolded,
skipped by feature gates, slow, or external-environment-dependent, keep that
visible here instead of relying on tribal memory.

## Code Anchors

- [../../preflight/src/main.rs](../../preflight/src/main.rs)
- [../../preflight/src/checks](../../preflight/src/checks)
- [../../preflight/src/report.rs](../../preflight/src/report.rs)
- [dashboard.md](dashboard.md)
- [../30-audits/tests/TEST-REALITY-AUDIT.md](../30-audits/tests/TEST-REALITY-AUDIT.md)
- [../30-audits/tests/IGNORED-TESTS-AUDIT.md](../30-audits/tests/IGNORED-TESTS-AUDIT.md)
