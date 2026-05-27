# dregg-dsl-differential

Cross-backend differential testing for dregg DSL.

## Owns

- Tests that compare DSL-generated predicates across available backends.
- Agreement checks for accept/reject behavior.

## Does Not Own

- Production runtime behavior.
- Proc-macro parsing.

## Local Check

```bash
cargo test -p dregg-dsl-differential
```
