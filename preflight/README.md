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

## Result Interpretation

Use [../docs/40-testing/preflight-reality.md](../docs/40-testing/preflight-reality.md)
to interpret subsystem results. Preflight is the broad local promotion gate, but
some subsystems are smoke or mixed evidence rather than full adversarial
soundness proof.
