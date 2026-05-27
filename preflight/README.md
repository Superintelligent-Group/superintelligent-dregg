# dregg-preflight

Golden-master subsystem gate for dregg.

## Owns

- Broad subsystem checks for runtime promotion.
- Preflight binary and test harness.
- Cross-subsystem smoke coverage.

## Does Not Own

- Individual subsystem implementation.
- CI policy by itself.

## Local Check

```bash
cargo run -p dregg-preflight
```
