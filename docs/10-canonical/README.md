# Canonical Runtime

These documents define the current backend-runtime model. Prefer them over
older plans or one-off session notes when there is a conflict.

## Core Story

- [canonicality.md](canonicality.md) - how to decide what is true when code,
  design docs, audits, and demos disagree.
- [runtime-map.md](runtime-map.md) - runtime layer map and developer entry
  points.
- [crate-map.md](crate-map.md) - crate ownership and dependency-orientation map.
- [api-surface.md](api-surface.md) - stable, internal, transitional, and
  experimental API surfaces.
- [flows/](flows/README.md) - turn, receipt, Effect VM, and Starbridge app
  walkthroughs grounded in code anchors.
- [model/NEW-WORLD.md](model/NEW-WORLD.md) - current coherent story of dregg:
  layers, naming, composition, and the runtime shape.
- [model/DREGG_DESIGN.md](model/DREGG_DESIGN.md) - older overview of fabric,
  cells, and turns. Still useful at the headline level.
- [model/BOUNDARIES.md](model/BOUNDARIES.md) - what is inside the system, what
  is outside, and what enforces each boundary.
- [model/PREDICATE-INVENTORY.md](model/PREDICATE-INVENTORY.md) - predicate and
  witnessed-predicate map.

## Runtime Spine

- `cell/` - isolated object state, permissions, capabilities, predicates,
  factories, and state constraints.
- `turn/` - atomic call-forest transaction model, executor, receipts,
  obligations, queues, bilateral binding, and proof-carrying paths.
- `circuit/` - proof systems, Effect VM AIR, predicate AIRs, Plonky3/Kimchi
  backend work, and proof-tier typing.
- `federation/`, `blocklace/`, `node/`, `net/` - committee/state-root logic,
  blocklace consensus, daemon/API/MCP surfaces, and networking.
- `sdk/`, `app-framework/`, `cli/` - developer-facing integration surfaces.

## Canonical Debt And Closure

- [debt/SILVER-DEBT.md](debt/SILVER-DEBT.md) - current Silver-vs-Golden debt
  ledger. This is the best single source for what remains unfinished.
- [debt/EXECUTOR-HONESTY-AUDIT.md](debt/EXECUTOR-HONESTY-AUDIT.md) - executor
  trust boundary and threat ledger.
- [debt/CAVEAT-LAYER-COVERAGE.md](debt/CAVEAT-LAYER-COVERAGE.md) - coverage of
  slot caveats, token caveats, and Effect VM AIR constraints.
- [debt/RECEIPT-ARCHITECTURE-STUDY.md](debt/RECEIPT-ARCHITECTURE-STUDY.md) -
  receipt chain and audit trail model.

## Canonicality Rules

- A runtime claim is canonical only if it is reflected in code, a canonical doc,
  and preferably a real test.
- An active design doc is not canonical runtime behavior until the code and tests
  land.
- A demo is not proof of protocol behavior unless it calls the real runtime,
  verifier, API, or independent commitment derivation path.
