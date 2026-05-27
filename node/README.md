# dregg-node

Federation node daemon.

## Owns

- `dregg-node` binary.
- Local HTTP API.
- MCP stdio server.
- Relay service hosting.
- Genesis and devnet node setup.
- Live node state integration across cell, turn, federation, blocklace, storage,
  and SDK surfaces.

## Does Not Own

- SDK ergonomics.
- Core proof semantics.
- App-specific UI or policy.

## Local Check

```bash
cargo check -p dregg-node
```
