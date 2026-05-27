# dregg-persist

Persistent storage backend for dregg state.

## Owns

- Persistence helpers for token chains, federation state, and audit logs.
- Storage integration primitives used by node/runtime surfaces.

## Does Not Own

- Protocol semantics.
- Node route behavior.

## Local Check

```bash
cargo check -p dregg-persist
```
