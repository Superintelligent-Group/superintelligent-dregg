# dregg-protocol-tests

Protocol-invariant property tests.

## Owns

- Generators and proptest harnesses for protocol invariants.
- Cross-crate invariant checks.

## Does Not Own

- Runtime implementation.
- Unit tests for individual crates.

## Local Check

```bash
cargo test -p dregg-protocol-tests
```
