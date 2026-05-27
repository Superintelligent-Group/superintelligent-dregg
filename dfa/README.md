# dregg-dfa

Canonical DFA routing engine and userspace dispatch primitive.

## Owns

- DFA route-table construction and matching.
- Governance-attestable dispatch primitives.

## Does Not Own

- Directory registration semantics.
- Federation committee implementation.

## Local Check

```bash
cargo check -p dregg-dfa
```
