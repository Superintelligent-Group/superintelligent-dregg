# dregg-dsl-runtime

Runtime IR and backend adapters used by generated dregg DSL code.

## Owns

- Runtime data structures emitted or consumed by DSL macros.
- Encodings shared between DSL-generated code and proof/runtime crates.
- Optional Plonky3/Kimchi bridge surfaces for DSL-backed constraints.

## Does Not Own

- Proc-macro parsing.
- Core Effect VM implementation.
- App-specific policy.

## Local Check

```bash
cargo check -p dregg-dsl-runtime
```
