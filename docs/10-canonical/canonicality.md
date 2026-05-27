# Canonicality Contract

This repo has many design notes, audits, demos, and session handoffs. A claim is
not canonical just because it is written down. Canonical runtime behavior must be
grounded in the current implementation and, where possible, in a real test.

## Truth Ladder

Use this order when sources disagree:

1. Current code paths that compile and are exercised by real tests.
2. Canonical docs in `docs/10-canonical/` plus the source docs linked there.
3. Current debt ledgers such as
   [debt/SILVER-DEBT.md](debt/SILVER-DEBT.md).
4. Audit evidence that has been checked against current code.
5. Active design docs.
6. Session handoffs and historical docs.
7. Narrative demos and old app READMEs.

## Runtime Claim Checklist

Before calling a behavior "implemented", answer:

- Which crate owns the behavior?
- Which public API or binary path exercises it?
- Which verifier, executor, AIR, API route, or independent derivation path checks
  it?
- Which test would fail if the behavior regressed?
- Is the test real, slow/manual, ignored, scaffold, or synthetic?
- Is there a matching entry in `SILVER-DEBT.md` or a current audit that narrows
  the claim?

If those answers are unclear, the behavior is not yet canonical. It may be an
active design or a prototype.

## Layer Ownership

| Layer | Owns | Canonical evidence |
| --- | --- | --- |
| `cell/` | state, permissions, capabilities, predicates, factories | state/program tests, predicate registry tests |
| `turn/` | action forest, executor, receipts, effects, obligations | executor tests, receipt tests, Effect VM bridge tests |
| `circuit/` | AIRs, proof backends, predicate proofs, proof tiers | adversarial AIR tests, prover/verifier tests |
| `federation/` + `blocklace/` | roots, committees, threshold attestations, ordering | threshold tests, blocklace tests, node integration |
| `node/` | HTTP API, MCP, relay, genesis, daemon operation | route/API tests, devnet/preflight |
| `sdk/` + `cli/` | developer/user entry points | crate checks, command tests, integration demos |
| `starbridge-apps/` | canonical userspace app pattern | factory descriptor tests, app-framework integration |

## Demo Claim Rule

A demo proves a runtime claim only when it drives a real surface:

- real Rust API or binary,
- real node HTTP/MCP route,
- real verifier/prover path,
- real independent commitment derivation,
- or real federation/devnet behavior.

Fixture writes, JSON self-checks, and string interpolation checks are useful
scaffold, but they are not protocol evidence.

## Documentation Promotion Rule

When a design lands:

1. Update or add real tests.
2. Update the owning crate docs or README.
3. Update the relevant `docs/*/README.md`.
4. Retire or relabel stale claims in old root docs.
5. Move broad unresolved work into `SILVER-DEBT.md` instead of burying it in a
   session note.
