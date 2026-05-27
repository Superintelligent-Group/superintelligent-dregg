# dregg-verifier

Standalone verification surface for dregg proof and receipt artifacts.

## Owns

- Verifier binary and library entry points.
- Receipt/proof replay checks that do not require mutable node state.
- Production-vs-scaffold proof-tier enforcement.

## Does Not Own

- Witness generation.
- Executor mutation.
- Node API routing.

## Local Check

```bash
cargo check -p dregg-verifier
```
