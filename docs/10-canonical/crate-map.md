# Crate Ownership Map

This map explains the root workspace from the perspective of backend-runtime
ownership. Use it before opening a large file or adding a dependency.

## Core State And Transaction Layer

| Crate | Owns | Does not own |
| --- | --- | --- |
| `dregg-types` | shared identifiers, signatures, attested roots, common protocol types | executor logic or proof systems |
| `dregg-cell` | cell state, permissions, capabilities, factories, predicates, state constraints | circuit verifier registries that would create dependency cycles |
| `dregg-turn` | actions, turns, executor, receipts, obligations, queues, bilateral scheduling | long-lived daemon state or networking |
| `dregg-commit` | canonical commitments and Merkle/tree helpers | policy or authorization semantics |
| `dregg-trace` | derivation trace format and evaluator | turn execution |

## Proof And Verification Layer

| Crate | Owns | Does not own |
| --- | --- | --- |
| `dregg-circuit` | AIRs, proof backends, Effect VM, predicate proofs, proof tiers | daemon routing or app policy |
| `dregg-verifier` | standalone proof and receipt verification entry point | witness generation or executor mutation |
| `dregg-dsl` | macro/DSL surface for caveats and effects | runtime execution |
| `dregg-dsl-runtime` | runtime IR/types used by DSL-generated code | compiler/proc-macro behavior |
| `dregg-dsl-tests` and `dregg-dsl-differential` | DSL conformance and cross-backend checks | production runtime surfaces |

## Federation, Network, And Daemon Layer

| Crate | Owns | Does not own |
| --- | --- | --- |
| `dregg-federation` | committee roots, BFT-ish federation state, threshold attestations | local node API routing |
| `dregg-blocklace` | DAG/blocklace consensus structures and ordering | HTTP/MCP server behavior |
| `dregg-node` | daemon, HTTP API, MCP stdio, relay, genesis, live node state | SDK ergonomics |
| `dregg-net` | gossip/network transport primitives | consensus policy |
| `dregg-wire` | wire protocol framing and network server/client behavior | local app framework |
| `dregg-persist` | persistent storage backend | protocol semantics |
| `dregg-observability` | trace event emission | core runtime decisions |

## Capability, Intent, Storage, And Userspace Layer

| Crate | Owns | Does not own |
| --- | --- | --- |
| `dregg-captp` | capability transport, sturdy refs, handoff, pipelining | executor application of effects |
| `dregg-intent` | privacy-preserving intent discovery and matching | node API policy |
| `dregg-storage` | storage queues, inboxes, relay primitives | canonical cell-program templates |
| `dregg-storage-templates` | storage primitives expressed as cell-program descriptors | operator-side enforcement loops |
| `dregg-credentials` | credential issue/present/verify/revoke primitives | app UI |
| `dregg-directory` | named-cell registration and lookup primitives | full starbridge app hosting |
| `dregg-dfa` | canonical DFA routing engine | legacy rbg routing |
| `dregg-rbg` | Robigalia-inspired userspace primitives | canonical DFA implementation |

## Developer And App Surfaces

| Crate | Owns | Does not own |
| --- | --- | --- |
| `dregg-cli` | user-facing command line over node/API/runtime surfaces | daemon state ownership |
| `dregg-sdk` | agent SDK, cipherclerk, local proof/turn helpers | server framework concerns |
| `dregg-app-framework` | app server/auth/persistence/cipherclerk/starbridge context helpers | core protocol semantics |
| `dregg-wasm` | browser playground/runtime bindings | native daemon runtime |
| `starbridge-*` | canonical userspace app examples and factories | new domain-specific runtime effects |
| `dregg-discord-bot` | devnet Discord interface | canonical runtime protocol |

## Test And Gate Crates

| Crate | Owns |
| --- | --- |
| `dregg-tests` | workspace-level regression/threat tests |
| `dregg-teasting` | integration simulation suite |
| `dregg-protocol-tests` | protocol invariant/property tests |
| `dregg-preflight` | subsystem promotion gate |
| `dregg-audit` | audit helper crate |
| `dregg-demo`, `dregg-demo-agent`, `dregg-sdk-consensus-demo` | demos and smoke paths |

## Metadata Quality Gate

Run:

```powershell
.\scripts\workspace-package-report.ps1
```

Every package should eventually have:

- `description`,
- `license` or inherited workspace license,
- `readme` or a clear crate-level module doc,
- a short owner/non-owner statement in docs or README.
