//! Sovereign-witness tests — Phase 1 algebraic teeth + wire-malleability.
//!
//! Layer: AIR (Effect VM) + canonical signing message + verifier-side
//! replay. See `AUDIT-sovereign-witness-teeth.md`,
//! `SOVEREIGN-WITNESS-AIR-DESIGN.md`, and `EXECUTOR-HONESTY-AUDIT.md` T9.
//!
//! Three concerns:
//!
//!   1. Phase 1: legal witness accepted; tampered key / sequence-regression
//!      rejected.
//!   2. T9 (executor skips sovereign witness): AIR must algebraically
//!      constrain the witness; it can't just decorate the receipt.
//!   3. Wire-malleability: turn v3 signing message must cover sovereign
//!      witnesses so tamper-then-sign fails.
//!
//! All currently `#[ignore]`d on the sovereign-witness AIR teeth lane.

use pyana_cell::CellId;

// ===========================================================================
// Phase 1: legal witness path
// ===========================================================================

#[test]
#[ignore = "blocked on SOVEREIGN-WITNESS-AIR-DESIGN.md Phase 1: AIR algebraically constrains sovereign witness (currently only decorates the receipt per AUDIT-sovereign-witness-teeth.md)"]
fn sovereign_witness_with_legal_key_accepts() {
    // Build a sovereign cell, sign a witness payload with its key, attach
    // to a turn, execute. Expect Committed + proof verifies.
    panic!("blocked");
}

#[test]
#[ignore = "blocked on sovereign-witness AIR teeth: tampered key (witness signed by a different key) must reject"]
fn sovereign_witness_with_tampered_key_rejects() {
    panic!("blocked");
}

#[test]
#[ignore = "blocked on sovereign-witness AIR teeth: witness sequence regression must reject"]
fn sovereign_witness_sequence_regression_rejects() {
    // Two turns with sovereign witnesses; the second turn's witness sequence
    // must be > the first's.
    panic!("blocked");
}

// ===========================================================================
// T9: executor cannot skip sovereign witness verification
// ===========================================================================

#[test]
#[ignore = "blocked on T9 (EXECUTOR-HONESTY-AUDIT.md T9): a turn against a sovereign cell with NO witness must reject"]
fn sovereign_cell_turn_without_witness_rejects() {
    // The whole point of sovereign cells is they can only mutate when the
    // owner signs a witness; a turn omitting the witness must reject.
    panic!("blocked");
}

#[test]
#[ignore = "blocked on T9: AIR-side constraint binds the sovereign witness to the cell transition (not just the receipt)"]
fn air_proof_constrains_sovereign_witness_to_transition() {
    // Build a turn with a valid witness payload but mismatched effect
    // (e.g., the witness authorized Transfer(10), the executor applies
    // Transfer(20)). The AIR's per-transition witness check must reject.
    panic!("blocked");
}

// ===========================================================================
// Wire-malleability (T9 tail)
// ===========================================================================

#[test]
#[ignore = "blocked on turn-canonical-signing-message audit: v3 signing message MUST cover sovereign_witnesses field (Turn::hash currently checks Turn::sovereign_witnesses, audit per EXECUTOR-HONESTY-AUDIT.md)"]
fn signing_message_covers_sovereign_witness_payload() {
    // 1. Sign a turn with witness W.
    // 2. Replace W with W' in the on-the-wire envelope (same shape, different
    //    payload bytes).
    // 3. Verifier MUST reject — the signature is over the hash that includes
    //    the witness bytes.
    panic!("blocked");
}

#[test]
#[ignore = "blocked on wire-malleability: tamper-then-sign workflow (attacker mutates witness AFTER signing, recomputes signature) — should still reject because re-signing requires the cell key"]
fn tamper_then_sign_witness_workflow_rejects() {
    panic!("blocked");
}

// ===========================================================================
// Cross-cutting: sovereign + bilateral + slot caveats
// ===========================================================================

#[test]
#[ignore = "blocked on sovereign witness AIR teeth + γ.2 + caveat-correctness: full composition"]
fn sovereign_witness_plus_bilateral_transfer_plus_slot_caveats() {
    // Composition mandate — see CAVEAT-LAYER-COVERAGE composition row.
    panic!("blocked");
}

// ===========================================================================
// Sanity: presence of sovereign_witnesses field on Turn does not by itself
// authorize a non-sovereign mutation.
// ===========================================================================

#[test]
fn turn_sovereign_witnesses_field_is_a_map_and_constructs_empty() {
    use pyana_turn::Turn;
    use std::collections::HashMap;
    let agent = CellId([1u8; 32]);
    let turn = Turn {
        agent,
        nonce: 0,
        call_forest: pyana_turn::CallForest::new(),
        fee: 0,
        memo: None,
        valid_until: None,
        previous_receipt_hash: None,
        depends_on: vec![],
        conservation_proof: None,
        sovereign_witnesses: HashMap::new(),
        execution_proof: None,
        execution_proof_cell: None,
        execution_proof_new_commitment: None,
        custom_program_proofs: None,
    };
    assert!(turn.sovereign_witnesses.is_empty());
}
