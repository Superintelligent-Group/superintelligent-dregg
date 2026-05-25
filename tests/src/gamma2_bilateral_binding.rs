//! γ.2 bilateral binding tests (STAGE-7-GAMMA-2-PI-DESIGN.md).
//!
//! Layer: cross-cell PI binding + off-AIR verifier algorithm.
//!
//! Phase 1 of γ.2 defines three canonical instance ids derived from public
//! surface data:
//!
//!   transfer_id = Poseidon2(b"pyana-transfer-id-v1" || from || to || amount_be || sender_nonce_be)
//!   grant_id    = Poseidon2(b"pyana-grant-id-v1"    || from || to || cap_entry_hash || sender_nonce_be)
//!   intro_id    = Poseidon2(b"pyana-intro-id-v1"    || introducer || recipient || target || permissions_bits || introducer_nonce_be)
//!
//! Each test in this file covers one of:
//!   - happy-path symmetric/asymmetric/trilateral binding;
//!   - sender-outgoing vs receiver-incoming disagreement → off-AIR reject;
//!   - tampered transfer_id (substitute a different id) → AIR reject;
//!   - permissions-bit tamper on `Introduce` → AIR reject;
//!   - federation-id binding across cross-federation `Introduce` (§1.3 tail).
//!
//! Most are `#[ignore]`d on γ.2 wiring — the PI fields exist on the design
//! doc but Phase 1 lands the off-AIR verifier independently from any
//! Phase 2 joint-aggregation AIR.

use pyana_cell::CellId;

// ---------------------------------------------------------------------------
// Canonical id derivations (testable today: pure-public-data functions)
// ---------------------------------------------------------------------------

/// Compute the canonical Phase-1 `transfer_id` preimage per
/// STAGE-7-GAMMA-2-PI-DESIGN.md §3.1.
fn transfer_id_preimage(from: &CellId, to: &CellId, amount: u64, sender_nonce: u64) -> Vec<u8> {
    let mut v = Vec::with_capacity(128);
    v.extend_from_slice(b"pyana-transfer-id-v1");
    v.extend_from_slice(&from.0);
    v.extend_from_slice(&to.0);
    v.extend_from_slice(&amount.to_be_bytes());
    v.extend_from_slice(&sender_nonce.to_be_bytes());
    v
}

fn grant_id_preimage(
    from: &CellId,
    to: &CellId,
    cap_entry_hash: &[u8; 32],
    sender_nonce: u64,
) -> Vec<u8> {
    let mut v = Vec::with_capacity(128);
    v.extend_from_slice(b"pyana-grant-id-v1");
    v.extend_from_slice(&from.0);
    v.extend_from_slice(&to.0);
    v.extend_from_slice(cap_entry_hash);
    v.extend_from_slice(&sender_nonce.to_be_bytes());
    v
}

fn intro_id_preimage(
    introducer: &CellId,
    recipient: &CellId,
    target: &CellId,
    permissions_bits: u32,
    introducer_nonce: u64,
) -> Vec<u8> {
    let mut v = Vec::with_capacity(128);
    v.extend_from_slice(b"pyana-intro-id-v1");
    v.extend_from_slice(&introducer.0);
    v.extend_from_slice(&recipient.0);
    v.extend_from_slice(&target.0);
    v.extend_from_slice(&permissions_bits.to_be_bytes());
    v.extend_from_slice(&introducer_nonce.to_be_bytes());
    v
}

// ===========================================================================
// Preimage shape + injectivity (testable today; design-level)
// ===========================================================================

#[test]
fn transfer_id_preimage_includes_domain_separator() {
    let pre = transfer_id_preimage(&CellId([1u8; 32]), &CellId([2u8; 32]), 10, 0);
    assert!(pre.starts_with(b"pyana-transfer-id-v1"));
}

#[test]
fn transfer_id_preimage_changes_with_direction() {
    let a = CellId([1u8; 32]);
    let b = CellId([2u8; 32]);
    let p_ab = transfer_id_preimage(&a, &b, 10, 0);
    let p_ba = transfer_id_preimage(&b, &a, 10, 0);
    assert_ne!(p_ab, p_ba, "direction must be in the preimage");
}

#[test]
fn transfer_id_preimage_changes_with_amount() {
    let a = CellId([1u8; 32]);
    let b = CellId([2u8; 32]);
    assert_ne!(
        transfer_id_preimage(&a, &b, 10, 0),
        transfer_id_preimage(&a, &b, 11, 0)
    );
}

#[test]
fn transfer_id_preimage_changes_with_sender_nonce() {
    let a = CellId([1u8; 32]);
    let b = CellId([2u8; 32]);
    assert_ne!(
        transfer_id_preimage(&a, &b, 10, 7),
        transfer_id_preimage(&a, &b, 10, 8),
        "same transfer at two nonces must yield different transfer_id (§3.4)"
    );
}

#[test]
fn grant_id_preimage_changes_with_cap_entry() {
    let a = CellId([1u8; 32]);
    let b = CellId([2u8; 32]);
    assert_ne!(
        grant_id_preimage(&a, &b, &[1u8; 32], 0),
        grant_id_preimage(&a, &b, &[2u8; 32], 0)
    );
}

#[test]
fn intro_id_preimage_distinguishes_roles() {
    // introducer / recipient / target distinctness — swapping any two
    // must change the preimage.
    let i = CellId([1u8; 32]);
    let r = CellId([2u8; 32]);
    let t = CellId([3u8; 32]);
    let base = intro_id_preimage(&i, &r, &t, 0, 0);
    let swap_ir = intro_id_preimage(&r, &i, &t, 0, 0);
    let swap_rt = intro_id_preimage(&i, &t, &r, 0, 0);
    assert_ne!(base, swap_ir);
    assert_ne!(base, swap_rt);
}

#[test]
fn intro_id_preimage_changes_with_permissions_bits() {
    let i = CellId([1u8; 32]);
    let r = CellId([2u8; 32]);
    let t = CellId([3u8; 32]);
    assert_ne!(
        intro_id_preimage(&i, &r, &t, 0, 0),
        intro_id_preimage(&i, &r, &t, 1, 0),
        "permissions_bits tampering must change preimage (and thus intro_id)"
    );
}

// ===========================================================================
// End-to-end binding: needs γ.2 Phase 1 wiring
// ===========================================================================

#[test]
#[ignore = "blocked on γ.2 Phase 1 PI extension: per-cell proof exposes transfer_id at canonical PI offset, off-AIR verifier joins sender + receiver"]
fn bilateral_transfer_happy_path_two_cells_verify_matched_transfer_id() {
    // 1. Build A=sender + B=receiver cells.
    // 2. Submit Transfer(A→B, 10) turn at A's nonce=7.
    // 3. Produce per-cell proofs for A and B (executor's per-cell projection).
    // 4. Run the off-AIR γ.2 verifier; it must:
    //      - compute transfer_id = Poseidon2(transfer_id_preimage(A, B, 10, 7))
    //      - assert PI[TRANSFER_ID_BASE..+4] on A's proof equals it
    //      - assert PI[TRANSFER_ID_BASE..+4] on B's proof equals it
    //      - assert A's direction-bit = 1 (outflow), B's = 0 (inflow).
    panic!("blocked");
}

#[test]
#[ignore = "blocked on γ.2 Phase 1: sender outgoing disagrees with receiver incoming → off-AIR verifier rejects"]
fn sender_outflow_vs_receiver_inflow_mismatch_rejects() {
    // E.g., A's projection says amount=10, B's says amount=11 → off-AIR
    // verifier sees mismatched transfer_id and rejects.
    panic!("blocked");
}

#[test]
#[ignore = "blocked on γ.2 Phase 1 AIR-side binding: tamper transfer_id between trace and PI; AIR rejects"]
fn tampered_transfer_id_in_pi_rejected_by_air() {
    // Build a proof where the prover claims transfer_id = X but the in-trace
    // transfer effect derives id = Y; AIR's "in-trace transfer-effect data
    // ties to PI transfer_id" constraint fires.
    panic!("blocked");
}

#[test]
#[ignore = "blocked on γ.2 Phase 1: GrantCapability bilateral binding"]
fn bilateral_grant_happy_path_two_cells() {
    // grantor + grantee proofs must both expose grant_id; off-AIR verifier
    // joins.
    panic!("blocked");
}

#[test]
#[ignore = "blocked on γ.2 Phase 1: GrantCapability tampered cap_entry"]
fn bilateral_grant_tampered_cap_entry_rejects() {
    panic!("blocked");
}

#[test]
#[ignore = "blocked on γ.2 Phase 1: Introduce trilateral binding"]
fn trilateral_introduce_happy_path_three_cells() {
    // introducer + recipient + target all expose intro_id; off-AIR verifier
    // joins the three proofs on intro_id agreement.
    panic!("blocked");
}

#[test]
#[ignore = "blocked on γ.2 Phase 1: Introduce permissions_bits tamper"]
fn trilateral_introduce_permissions_bit_tamper_rejects() {
    panic!("blocked");
}

#[test]
#[ignore = "blocked on γ.2 Phase 1 cross-federation extension (§1.3): federation_id appended to Introduce preimage"]
fn cross_federation_introduce_includes_federation_id_in_intro_id_preimage() {
    panic!("blocked");
}

// ===========================================================================
// Three-cell bilateral compositions (ring trade)
// ===========================================================================

#[test]
#[ignore = "blocked on γ.2 Phase 2 (joint aggregation AIR sketch) — three-cell ring of bilateral effects"]
fn three_cell_ring_transfer_all_pairings_bound() {
    // A→B, B→C, C→A; three transfer_ids must each match across their two
    // touched cells; off-AIR verifier walks each pair.
    panic!("blocked");
}

#[test]
#[ignore = "blocked on γ.2 Phase 2: ring with one tampered transfer_id (between any two cells) rejects"]
fn three_cell_ring_with_tampered_pair_rejects() {
    panic!("blocked");
}

// ===========================================================================
// Compositions with slot caveats / sovereign witness
// ===========================================================================

#[test]
#[ignore = "blocked on γ.2 + slot caveats on both cells (composition target from CAVEAT-LAYER-COVERAGE.md row 24)"]
fn bilateral_transfer_with_bound_delta_caveat_on_both_sides() {
    panic!("blocked");
}

#[test]
#[ignore = "blocked on γ.2 + sovereign witness AIR teeth: bilateral transfer between two sovereign cells must bind transfer_id AND verify sovereign witnesses"]
fn bilateral_transfer_with_sovereign_witness_on_both_sides() {
    panic!("blocked");
}
