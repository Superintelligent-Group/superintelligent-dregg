# dregg-bridge

Bridge proof and presentation layer for dregg.

## Owns

- Presentation proof formats that connect token traces to verifier inputs.
- Bridge-facing verifier helpers.
- Test utilities for presentation and bridge proof paths.

## Does Not Own

- Turn execution.
- Federation root production.
- Node API behavior.

## Local Check

```bash
cargo check -p dregg-bridge
```
