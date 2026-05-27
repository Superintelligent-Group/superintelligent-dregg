# Runtime Map

This map is for navigating the backend runtime without reading every crate.

## Execution Path

1. A user, app, or agent builds an `Action` / `Turn` through `sdk/`, `cli/`, or
   `app-framework/`.
2. `turn/` validates authorization, applies effects through the executor, and
   emits a receipt.
3. `cell/` owns the state model, permission model, capability set, factory
   descriptors, predicates, and state constraints touched by those effects.
4. `circuit/` proves or verifies the proof-carrying parts of the transition.
5. `federation/` and `blocklace/` bind the transition into attested roots and
   ordered history.
6. `node/` exposes the live daemon surfaces: HTTP API, MCP stdio, relay, gossip,
   genesis, and devnet behavior.

## Developer Entry Points

| Task | Start in |
| --- | --- |
| Build or submit a turn | `sdk/`, `cli/`, `turn/` |
| Debug authorization | `cell/src/permissions.rs`, `turn/src/action.rs`, `turn/src/executor/` |
| Debug a receipt or replay chain | `turn/src/verify.rs`, `verifier/`, `RECEIPT-ARCHITECTURE-STUDY.md` |
| Debug Effect VM proof behavior | `circuit/src/effect_vm/`, `turn/src/executor/effect_vm_bridge.rs` |
| Debug federation state roots | `federation/`, `blocklace/`, `node/src/state.rs` |
| Debug node API/MCP behavior | `node/src/api.rs`, `node/src/mcp.rs`, `cli/src/commands/` |
| Build a canonical app | `starbridge-apps/`, `app-framework/` |
| Validate subsystem health | `preflight/`, `.config/nextest.toml` |

## Main Binary Surfaces

- `dregg-cli` from `cli/` - user-facing command line.
- `dregg-node` from `node/` - daemon, MCP server, relay, genesis.
- `dregg-preflight` from `preflight/` - subsystem gate.
- `dregg-verifier` from `verifier/` - standalone proof/receipt verification.

## Large Files To Approach Carefully

These files are central but too large to treat casually:

- `node/src/mcp.rs`
- `node/src/api.rs`
- `sdk/src/cipherclerk.rs`
- `turn/src/tests.rs`
- `turn/src/executor/apply.rs`
- `circuit/src/effect_vm/tests.rs`

Before refactoring them, identify the owning behavior and add or preserve the
test that proves it.
