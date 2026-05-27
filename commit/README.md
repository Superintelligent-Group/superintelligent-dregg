# dregg-commit

Canonical commitment helpers for dregg.

## Owns

- BLAKE3/Poseidon-oriented commitment helpers.
- Merkle/tree helpers used by state, capabilities, and proof inputs.
- Low-level commitment vocabulary shared by runtime crates.

## Does Not Own

- Authorization policy.
- Executor behavior.
- Network or federation state.

## Local Check

```bash
cargo check -p dregg-commit
```
