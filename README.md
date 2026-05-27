# dregg - Dragon's Egg

Dragon's Egg is the backend runtime for a capability-secure, proof-carrying
distributed agent fabric. The core model is:

- **Cells**: isolated objects with state, permissions, capabilities, predicates,
  and optional proof-verification keys.
- **Turns**: atomic call-forest transactions that apply effects and emit
  receipts.
- **Effect VM and circuits**: algebraic execution traces, predicate proofs, and
  proof-tier typing.
- **Federation and blocklace**: committee/state-root logic, DAG ordering, and
  cross-federation receipts.
- **CapTP and intents**: capability transport, handoff, store-and-forward, and
  privacy-preserving intent discovery.
- **SDK, CLI, node, and starbridge apps**: developer/runtime surfaces for using
  the system.

This is active research software and the core backend runtime for the larger
company system. Treat canonical docs, tests, and code as a single contract:
claims should be tied to real runtime behavior or clearly marked as active
design, scaffold, or debt.

## Start Here

The documentation front door is [docs/README.md](docs/README.md).

Recommended reading order:

1. [docs/00-start-here/README.md](docs/00-start-here/README.md)
2. [docs/10-canonical/README.md](docs/10-canonical/README.md)
3. [docs/40-testing/README.md](docs/40-testing/README.md)
4. [docs/50-apps-runtime/README.md](docs/50-apps-runtime/README.md)
5. [docs/60-operations/README.md](docs/60-operations/README.md)

For current debt and unfinished proof/runtime work, read
[docs/10-canonical/debt/SILVER-DEBT.md](docs/10-canonical/debt/SILVER-DEBT.md).
For current test honesty, read
[docs/30-audits/tests/TEST-REALITY-AUDIT.md](docs/30-audits/tests/TEST-REALITY-AUDIT.md).

## Quick Checks

Use narrow checks while iterating:

```bash
cargo check -p dregg-cli
cargo check -p dregg-node
cargo test -p dregg-turn --lib
```

Run the preflight subsystem gate deliberately:

```bash
cargo run -p dregg-preflight
```

Use the Docker devnet when you need a local federation:

```bash
./docker/start-devnet.sh
```

See [docker/README.md](docker/README.md) for ports, faucet, logs, and reset.

## Workspace Shape

The root Cargo workspace contains the core runtime crates, node, CLI, SDK,
starbridge apps, protocol tests, and preflight harness. `chain/` and
`chain/program/` are standalone workspaces excluded from the root workspace.

Primary runtime crates:

| Area | Crates / Paths |
| --- | --- |
| Core model | `cell/`, `turn/`, `types/`, `commit/`, `trace/` |
| Proofs | `circuit/`, `verifier/`, `dregg-dsl/`, `dregg-dsl-runtime/` |
| Federation/network | `federation/`, `blocklace/`, `node/`, `net/`, `wire/` |
| Capability/intents/storage | `captp/`, `intent/`, `storage/`, `dregg-storage-templates/` |
| Developer surfaces | `cli/`, `sdk/`, `app-framework/`, `wasm/`, `site/` |
| Apps | `starbridge-apps/*` |
| Tests/gates | `tests/`, `teasting/`, `protocol-tests/`, `preflight/` |

Legacy/research app crates under `apps/` are not the canonical app direction.
Start with [starbridge-apps/README.md](starbridge-apps/README.md) and
[docs/50-apps-runtime/README.md](docs/50-apps-runtime/README.md).

## Documentation Classes

- **Canonical**: current runtime truth.
- **Active design**: intended or in-flight direction.
- **Audit evidence**: specific evidence that may need freshness checks.
- **Operations**: commands and environment constraints.
- **History**: archaeology, not authority.

When these conflict, prefer current code plus canonical docs plus real tests.

## Status

Experimental. The codebase contains real proof, receipt, federation, and runtime
work, but some demos and adversarial tests are still scaffolded or intentionally
ignored. Do not use this for security-critical production behavior without an
independent audit and a current full-system verification pass.

## License

MIT OR Apache-2.0
