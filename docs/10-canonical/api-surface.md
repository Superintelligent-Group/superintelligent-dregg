# API Surface Inventory

This page names which public-looking surfaces are intended developer APIs, which
are runtime-internal, and which are transitional. Use it before adding re-exports
or teaching downstream code to depend on a module path.

## API Classes

| Class | Meaning |
| --- | --- |
| Stable | Preferred import surface for application, SDK, CLI, or verifier users. |
| Runtime-internal | Public for crate composition or tests, but not the preferred downstream API. |
| Transitional | Kept for compatibility or active migration; avoid new call sites. |
| Experimental | Useful, but feature-gated, backend-specific, or not yet part of the stable runtime contract. |

## Stable Developer Surfaces

| Surface | Class | Use for | Code anchor |
| --- | --- | --- | --- |
| `dregg-sdk` crate root | Stable | Agent-local identity, tokens, turn building, privacy helpers, receipt verification helpers | [../../sdk/src/lib.rs](../../sdk/src/lib.rs) |
| `dregg-app-framework` crate root | Stable | App servers, `AppCipherclerk`, `EmbeddedExecutor`, Starbridge app context, common app re-exports | [../../app-framework/src/lib.rs](../../app-framework/src/lib.rs) |
| `dregg-cli` binary | Stable | User/operator command line over runtime and node surfaces | [../../cli/src/main.rs](../../cli/src/main.rs) |
| `dregg-node` binary | Stable for operators, internal for libraries | Daemon, HTTP API, MCP stdio, relay, genesis, devnet behavior | [../../node/src/main.rs](../../node/src/main.rs) |
| `dregg-verifier` binary and crate root | Stable | Standalone proof, receipt, bilateral, aggregated, and cross-fed verification | [../../verifier/src/lib.rs](../../verifier/src/lib.rs) |
| `starbridge-apps/*` crates | Stable examples | Canonical userspace app examples and factory patterns | [../../starbridge-apps](../../starbridge-apps) |

## Stable Runtime Composition Surfaces

| Surface | Class | Use for | Code anchor |
| --- | --- | --- | --- |
| `dregg-types` crate root | Stable | IDs, signatures, attested roots, common protocol types | [../../types/src/lib.rs](../../types/src/lib.rs) |
| `dregg-cell` crate root | Stable for runtime crates | Cell state, permissions, capabilities, predicates, factories, state constraints | [../../cell/src/lib.rs](../../cell/src/lib.rs) |
| `dregg-turn` crate root | Stable for runtime crates | `Action`, `Turn`, `TurnBuilder`, `TurnExecutor`, `TurnReceipt`, `WitnessedReceipt` | [../../turn/src/lib.rs](../../turn/src/lib.rs) |
| `dregg-captp` crate root | Stable for capability transport work | Sturdy refs, handoff, pipelining, CapTP wire concepts | [../../captp/src/lib.rs](../../captp/src/lib.rs) |
| `dregg-federation` crate root | Stable for federation layer | Federation state, threshold attestations, cross-fed receipt bundle types | [../../federation/src/lib.rs](../../federation/src/lib.rs) |
| `dregg-blocklace` crate root | Stable for consensus layer | Blocklace/DAG structures and ordering primitives | [../../blocklace/src/lib.rs](../../blocklace/src/lib.rs) |

## Internal Or Low-Level Surfaces

These are public because Rust crates need to compose, but downstream app code
should usually reach them through `sdk`, `app-framework`, `cli`, or `verifier`.

| Surface | Prefer instead | Reason |
| --- | --- | --- |
| `turn/src/executor/*` modules | `dregg_turn::TurnExecutor`, `app-framework::EmbeddedExecutor` | Executor internals are security-sensitive and change with runtime semantics. |
| `cell/src/predicate.rs` internals | `dregg_cell::WitnessedPredicateRegistry` and app-framework re-exports | Predicate dispatch is canonical, but internals track proof-lane changes. |
| `circuit/src/effect_vm/*` | `dregg-verifier`, `dregg-circuit` top-level proof APIs | AIR columns and public-input offsets are low-level proof implementation. |
| `node/src/api.rs` and `node/src/mcp.rs` | `dregg-cli`, `dregg-sdk`, documented node API surfaces | Large daemon files should not become library APIs by accident. |
| `sdk/src/cipherclerk.rs` internals | `dregg_sdk::AgentCipherclerk` | Keep cipherclerk state representation private to the SDK. |

## Transitional Surfaces

| Surface | Status | Rule |
| --- | --- | --- |
| `dregg_sdk::cclerk` | Transitional alias | Existing callers may compile; new code should use `dregg_sdk::cipherclerk` or `AgentCipherclerk`. |
| `dregg_app_framework::cclerk` | Transitional alias | Existing callers may compile; new code should use `AppCipherclerk`. |
| `apps/` legacy app tree | Research/legacy | New canonical app work should go under `starbridge-apps/`. |
| `chain/` and `chain/program/` | Standalone workspaces | They are intentionally excluded from the root workspace; do not assume root workspace checks cover them. |
| `teasting/` | Historical spelling | Treat as the current package path unless a coordinated rename updates workspace, docs, CI, and imports. |

## Experimental Or Feature-Gated Surfaces

| Surface | Why |
| --- | --- |
| `dregg-circuit` backend modules | Proof backend behavior is feature-gated and some tests are slow or conditional. |
| `dregg-sdk` network and CapTP modules | Gated behind `network` or `captp`; wasm/no-IO builds intentionally exclude them. |
| Recursive proof and IVC helpers | Important but still tied to active proof-lane work and verification caveats. |
| Devnet shell scripts | Useful operations fixtures, not canonical protocol proof by themselves. |

## Import Rules

1. App code should import from `dregg-app-framework` first.
2. Agent/client code should import from `dregg-sdk` first.
3. Runtime crates may import from `dregg-cell`, `dregg-turn`, `dregg-types`,
   `dregg-captp`, `dregg-federation`, and `dregg-blocklace` according to the
   ownership map.
4. Verification tools should avoid depending on `dregg-node`, `dregg-wire`, or
   mutable executor state.
5. Do not add new compatibility aliases without also documenting the migration
   target and removal condition here.

## Re-export Rule

Add a root re-export only when it is a stable import path that downstream code
should use. If a type is only public to connect internal crates, leave it in its
own module and document the owning crate in [crate-map.md](crate-map.md).
