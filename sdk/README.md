# dregg-sdk

Agent SDK and cipherclerk integration surface.

## Owns

- `AgentCipherclerk` and local agent authority management.
- Turn/proof helpers for agent workflows.
- Optional network/embed/discharge/discovery clients behind feature gates.

## Does Not Own

- Server framework behavior.
- Node daemon state.
- Core protocol semantics.

## Local Check

```bash
cargo check -p dregg-sdk
```
