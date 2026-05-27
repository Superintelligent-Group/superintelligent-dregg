# dregg-types

Canonical shared protocol types.

## Owns

- Shared identifiers and wire-stable data types.
- Public key and signature wrappers.
- Attested root and federation-facing common structures.

## Does Not Own

- Behavior-heavy executor logic.
- Proof backend implementation.
- Node state.

## Local Check

```bash
cargo check -p dregg-types
```
