# dregg-dsl

Procedural macro DSL for declaring dregg caveats, effects, and constraint
surfaces.

## Owns

- Proc-macro entry points for DSL declarations.
- Syntax-to-generated-code shape for caveats/effects.

## Does Not Own

- Runtime IR semantics. Those live in `dregg-dsl-runtime`.
- Proof backend implementation. That lives in `dregg-circuit`.

## Local Check

```bash
cargo check -p dregg-dsl
```
