# dregg-captp

Capability transport primitives for dregg.

## Owns

- Sturdy references and live capability reference data.
- Handoff certificates.
- Pipelined actions.
- CapTP URI formats and transport-level capability vocabulary.

## Does Not Own

- Executor application of CapTP effects.
- Node MCP/API exposure.
- Federation consensus.

## Local Check

```bash
cargo check -p dregg-captp
```
