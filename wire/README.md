# dregg-wire

Wire protocol framing and network-facing verification behavior.

## Owns

- Postcard/TLS-oriented wire framing.
- Network server/client protocol surface.
- Cross-node authorization demos.
- Optional bridge/STARK verification features used by network paths.

## Does Not Own

- Node daemon state.
- Consensus policy.
- App server framework.

## Local Check

```bash
cargo check -p dregg-wire
```
