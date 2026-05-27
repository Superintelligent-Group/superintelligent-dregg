# dregg-coord

Two-layer turn coordination: causal chaining and atomic multi-party turns.

## Owns

- Coordination primitives around turn ordering and multi-party atomicity.
- Causal and commit-style coordination helpers.

## Does Not Own

- Turn execution internals.
- Federation consensus.

## Local Check

```bash
cargo check -p dregg-coord
```
