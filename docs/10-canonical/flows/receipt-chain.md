# Receipt Chain Flow

Receipts are the runtime's audit trail. They bind a turn to the executing agent,
the effects/events that were accepted, and the previous receipt when a chain is
continuing.

## Code Anchors

| Concern | Open first |
| --- | --- |
| `TurnReceipt` fields and hashing | [../../../turn/src/turn.rs](../../../turn/src/turn.rs) |
| Receipt signing and chain verification | [../../../turn/src/verify.rs](../../../turn/src/verify.rs) |
| Witnessed receipt bundle | [../../../turn/src/witnessed_receipt.rs](../../../turn/src/witnessed_receipt.rs) |
| Standalone replay verifier | [../../../verifier/src/lib.rs](../../../verifier/src/lib.rs) |
| CLI verifier entry point | [../../../verifier/src/main.rs](../../../verifier/src/main.rs) |
| Receipt architecture debt | [../debt/RECEIPT-ARCHITECTURE-STUDY.md](../debt/RECEIPT-ARCHITECTURE-STUDY.md) |

## Flow

1. Execution finalization creates a `TurnReceipt`.
2. The receipt hash becomes the continuity handle for the next accepted turn.
3. `previous_receipt_hash` links consecutive receipts.
4. `WitnessedReceipt` adds proof/public-input material for replay and external
   verification.
5. `dregg-verifier` can replay witnessed receipt chains and report whether the
   chain verifies.

## What A Receipt Proves

A receipt proves that the runtime path emitted a specific accepted result. It is
stronger when paired with:

- a verified signature or federation attestation,
- a valid previous-receipt chain,
- public inputs that bind the turn hash, effects hash, and previous receipt,
- a proof that verifies through the real verifier path.

## Current Cautions

The test audits identify cases where tests reject for the wrong reason, such as
empty proof bytes failing before the intended witness-hash or unwitnessable
branch is reached. When adding receipt tests, assert the rejection cause, not
only `verified == false`.

## Good Integration Examples

- [../../../sdk/tests/integration_cipherclerk_receipt_chain.rs](../../../sdk/tests/integration_cipherclerk_receipt_chain.rs)
  focuses on receipt chain continuity in the SDK path.
- [../../../verifier/tests/integration_replay_chain.rs](../../../verifier/tests/integration_replay_chain.rs)
  exercises replay-chain verification.
- [../../../app-framework/tests/integration_app_cipherclerk_lifecycle.rs](../../../app-framework/tests/integration_app_cipherclerk_lifecycle.rs)
  checks previous receipt continuity after repeated app submissions.
