# dregg-federation

Federation committee and attestation primitives.

## Owns

- Committee descriptors and federation identity.
- Attested roots and threshold-signature behavior.
- Pure federation-state and consensus orchestration primitives.
- Optional native runtime transport support behind the `runtime` feature.

## Does Not Own

- Node HTTP/MCP routing.
- Blocklace data structures.
- SDK or app ergonomics.

## Local Check

```bash
cargo check -p dregg-federation
```
