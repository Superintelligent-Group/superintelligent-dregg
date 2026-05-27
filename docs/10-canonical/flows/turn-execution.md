# Turn Execution Flow

The turn flow is the core backend runtime path. A turn is an atomic call forest:
all roots and children commit together, or the executor rejects the whole unit.

## Code Anchors

| Concern | Open first |
| --- | --- |
| Type exports and trust boundary | [../../../turn/src/lib.rs](../../../turn/src/lib.rs) |
| Turn and receipt structures | [../../../turn/src/turn.rs](../../../turn/src/turn.rs) |
| Action shape and authorization | [../../../turn/src/action.rs](../../../turn/src/action.rs) |
| Builders | [../../../turn/src/builder.rs](../../../turn/src/builder.rs) |
| Executor owner module | [../../../turn/src/executor/mod.rs](../../../turn/src/executor/mod.rs) |
| Authorization path | [../../../turn/src/executor/authorize.rs](../../../turn/src/executor/authorize.rs) |
| Effect application | [../../../turn/src/executor/apply.rs](../../../turn/src/executor/apply.rs) |
| Final receipt construction | [../../../turn/src/executor/finalize.rs](../../../turn/src/executor/finalize.rs) |

## Classical Path

1. A caller builds an `Action` with target, method, effects, and
   `Authorization`.
2. One or more actions are assembled into a `CallForest`.
3. `TurnExecutor` validates authorization, preconditions, budget, and conflict
   constraints.
4. The executor applies effects to the touched `CellState` records.
5. Finalization emits a `TurnReceipt` with the turn hash, action count, events,
   state roots, and receipt-chain fields.

This is executor-trusted behavior. The federation and receipt chain are the
external evidence that the executor accepted and ordered the transition.

## Proof-Carrying Path

`turn/src/lib.rs` names the trust boundary: when a turn carries an
`execution_proof`, the executor should verify proof material and update the
commitment without interpreting the full private state. The surrounding code is
under `turn/src/executor/proof_verify.rs` and Effect VM bridge code.

Treat this as a different security mode from the classical path. A test that
only exercises classical mutation does not prove proof-carrying execution.

## Good Integration Examples

- [../../../turn/tests/integration_lifecycle.rs](../../../turn/tests/integration_lifecycle.rs)
  drives lifecycle behavior through `TurnExecutor`.
- [../../../app-framework/tests/integration_app_cipherclerk_lifecycle.rs](../../../app-framework/tests/integration_app_cipherclerk_lifecycle.rs)
  covers construct, sign, submit, receipt, restart, and multi-action behavior
  through the app-facing executor wrapper.
- [../../../starbridge-apps/identity/tests/integration_issue_present_verify.rs](../../../starbridge-apps/identity/tests/integration_issue_present_verify.rs)
  drives Starbridge identity actions through the embedded executor and asserts
  receipt events.

## Change Rule

When changing turn execution, update the closest real executor-facing test. Do
not rely on a builder-only test to prove executor behavior, and do not rely on a
hash inequality assertion to prove verifier rejection.
