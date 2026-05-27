# dregg-cli

User-facing command line for dregg.

## Owns

- The `dregg` binary.
- Human-oriented command surfaces for cells, turns, capabilities, proofs,
  federation, storage, routing, and node diagnostics.
- CLI config and output formatting.

## Does Not Own

- Node daemon state.
- Protocol semantics.
- SDK internal key management.

## Local Check

```bash
cargo check -p dregg-cli
```
