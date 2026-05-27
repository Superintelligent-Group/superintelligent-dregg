# dregg-circuit

Proof systems, AIRs, and verifier/prover support for dregg.

## Owns

- Effect VM AIR and proof-tier types.
- Predicate, derivation, membership, IVC, and backend-specific proof modules.
- Plonky3/Kimchi/Mina bridge experiments and real proof backends.

## Does Not Own

- Executor state mutation.
- Node APIs.
- App-specific authorization policy.

## Local Check

```bash
cargo check -p dregg-circuit
```
