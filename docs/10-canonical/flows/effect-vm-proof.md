# Effect VM Proof Flow

The Effect VM path turns a list of effects and state commitments into an AIR
trace, proves the trace, and verifies that the public inputs match the expected
state transition.

## Code Anchors

| Concern | Open first |
| --- | --- |
| Effect VM module | [../../../circuit/src/effect_vm](../../../circuit/src/effect_vm) |
| Integration prove/verify tests | [../../../circuit/tests/integration_effect_vm_prove_verify.rs](../../../circuit/tests/integration_effect_vm_prove_verify.rs) |
| General STARK implementation | [../../../circuit/src/stark.rs](../../../circuit/src/stark.rs) |
| Turn bridge into Effect VM | [../../../turn/src/executor/effect_vm_bridge.rs](../../../turn/src/executor/effect_vm_bridge.rs) |
| Recursive witness bundle | [../../../circuit/src/recursive_witness_bundle.rs](../../../circuit/src/recursive_witness_bundle.rs) |
| Effect VM soundness audit | [../../30-audits/soundness/EFFECT-VM-NOOP-AUDIT.md](../../30-audits/soundness/EFFECT-VM-NOOP-AUDIT.md) |
| Ignored-test audit | [../../30-audits/tests/IGNORED-TESTS-AUDIT.md](../../30-audits/tests/IGNORED-TESTS-AUDIT.md) |

## Flow

1. Runtime effects are projected into Effect VM effect variants.
2. Trace generation records state-before, state-after, and effect-specific
   columns.
3. Public inputs bind the claimed transition.
4. The STARK prover emits proof bytes.
5. The verifier checks AIR constraints and public inputs.
6. Recursive/witnessed receipt paths can carry proof material for replay or
   aggregation.

## What Counts As Evidence

Strong evidence:

- prove through the real prover,
- verify through the real verifier,
- assert the transition semantics, not only `Ok`,
- tamper proof/state/public inputs and assert the intended rejection path.

Weak evidence:

- direct AIR constraint evaluation without verifier coverage,
- hash inequality without verifier invocation,
- ignored tests for current soundness requirements,
- demo fixture checks that never touch proof verification.

## Current Caveat

The audits call out FRI single-row gaps and several tests that only prove direct
constraint nonzero behavior. Do not present those as full STARK rejection until
the verifier path reliably rejects the tampered trace.

## Change Rule

When adding a new effect variant, update trace generation, public-input binding,
prove/verify tests, and the testing dashboard if the proof path is intentionally
partial.
