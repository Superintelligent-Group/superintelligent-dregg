//! Constraint blocks for CapTP effects: ExportSturdyRef, EnlivenRef, DropRef, ValidateHandoff.

use crate::field::BabyBear;
use crate::poseidon2::hash_2_to_1;
use crate::effect_vm::{sel, state, param, AUX_BASE, STATE_BEFORE_BASE, STATE_AFTER_BASE, PARAM_BASE};

/// Accumulate ExportSturdyRef constraints.
///
/// Proves: swiss_number = hash(cell_id, hash(random_seed, counter)).
/// State: field[7] increments (export_counter), balance/cap/other fields unchanged.
pub fn constrain_export_sturdy_ref(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_export = local[sel::EXPORT_STURDY_REF];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];

    let cell_id = local[PARAM_BASE + param::EXPORT_CELL_ID];
    let random_seed = local[PARAM_BASE + param::EXPORT_RANDOM_SEED];
    let export_counter = local[PARAM_BASE + param::EXPORT_COUNTER];
    // Swiss number = hash(cell_id, hash(random_seed, counter))
    let inner_hash = hash_2_to_1(random_seed, export_counter);
    let expected_swiss = hash_2_to_1(cell_id, inner_hash);
    let aux_swiss = local[AUX_BASE + 0];
    let c_swiss = s_export * (aux_swiss - expected_swiss);
    combined = combined + alpha_pow * c_swiss;
    alpha_pow = alpha_pow * alpha;

    // field[7] must increment by 1 (export counter).
    let old_f7 = local[STATE_BEFORE_BASE + state::FIELD_BASE + 7];
    let new_f7 = local[STATE_AFTER_BASE + state::FIELD_BASE + 7];
    let c_counter = s_export * (new_f7 - old_f7 - BabyBear::ONE);
    combined = combined + alpha_pow * c_counter;
    alpha_pow = alpha_pow * alpha;

    // Balance unchanged.
    let c_bal_lo = s_export * (new_bal_lo - old_bal_lo);
    combined = combined + alpha_pow * c_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_bal_hi = s_export * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_bal_hi;
    alpha_pow = alpha_pow * alpha;

    // Cap root unchanged.
    let c_cap = s_export * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_cap;
    alpha_pow = alpha_pow * alpha;

    // Fields 0..7 unchanged (only field[7] changes).
    for i in 0..7 {
        let c = s_export
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate EnlivenRef constraints.
///
/// Proves: hash(swiss_number, expected_cell_id) matches committed table entry.
/// State: field[6] increments (use_count), balance/cap/other fields unchanged.
pub fn constrain_enliven_ref(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_enliven = local[sel::ENLIVEN_REF];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];

    let swiss = local[PARAM_BASE + param::ENLIVEN_SWISS];
    let expected_cell_id = local[PARAM_BASE + param::ENLIVEN_CELL_ID];
    let expected_perms = local[PARAM_BASE + param::ENLIVEN_PERMISSIONS];
    let inner = hash_2_to_1(expected_cell_id, expected_perms);
    let expected_entry_hash = hash_2_to_1(swiss, inner);
    let aux_entry = local[AUX_BASE + 0];
    let c_entry = s_enliven * (aux_entry - expected_entry_hash);
    combined = combined + alpha_pow * c_entry;
    alpha_pow = alpha_pow * alpha;

    // field[6] must increment by 1 (use_count).
    let old_f6 = local[STATE_BEFORE_BASE + state::FIELD_BASE + 6];
    let new_f6 = local[STATE_AFTER_BASE + state::FIELD_BASE + 6];
    let c_use = s_enliven * (new_f6 - old_f6 - BabyBear::ONE);
    combined = combined + alpha_pow * c_use;
    alpha_pow = alpha_pow * alpha;

    // Balance unchanged.
    let c_bal_lo = s_enliven * (new_bal_lo - old_bal_lo);
    combined = combined + alpha_pow * c_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_bal_hi = s_enliven * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_bal_hi;
    alpha_pow = alpha_pow * alpha;

    // Cap root unchanged.
    let c_cap = s_enliven * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_cap;
    alpha_pow = alpha_pow * alpha;

    // Fields 0..6 and field[7] unchanged (only field[6] changes).
    for i in 0..6 {
        let c = s_enliven
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    // field[7] unchanged.
    let c_f7 = s_enliven
        * (local[STATE_AFTER_BASE + state::FIELD_BASE + 7]
            - local[STATE_BEFORE_BASE + state::FIELD_BASE + 7]);
    combined = combined + alpha_pow * c_f7;
    alpha_pow = alpha_pow * alpha;

    (combined, alpha_pow)
}

/// Accumulate DropRef constraints.
///
/// State: field[5] decrements (refcount), balance/cap/other fields unchanged.
pub fn constrain_drop_ref(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_drop = local[sel::DROP_REF];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let refcount_param = local[PARAM_BASE + param::DROP_REFCOUNT];

    // field[5] must decrement by 1.
    let old_f5 = local[STATE_BEFORE_BASE + state::FIELD_BASE + 5];
    let new_f5 = local[STATE_AFTER_BASE + state::FIELD_BASE + 5];
    let c_dec = s_drop * (new_f5 - old_f5 + BabyBear::ONE);
    combined = combined + alpha_pow * c_dec;
    alpha_pow = alpha_pow * alpha;

    // refcount param must match old field[5].
    let c_rc = s_drop * (refcount_param - old_f5);
    combined = combined + alpha_pow * c_rc;
    alpha_pow = alpha_pow * alpha;

    // Prove refcount > 0: aux[0] = inverse(refcount_param).
    let rc_inv = local[AUX_BASE + 0];
    let c_nonzero = s_drop * (refcount_param * rc_inv - BabyBear::ONE);
    combined = combined + alpha_pow * c_nonzero;
    alpha_pow = alpha_pow * alpha;

    // Balance unchanged.
    let c_bal_lo = s_drop * (new_bal_lo - old_bal_lo);
    combined = combined + alpha_pow * c_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_bal_hi = s_drop * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_bal_hi;
    alpha_pow = alpha_pow * alpha;

    // Cap root unchanged.
    let c_cap = s_drop * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_cap;
    alpha_pow = alpha_pow * alpha;

    // Fields 0..5, 6, 7 unchanged (only field[5] changes).
    for i in 0..5 {
        let c = s_drop
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    for i in 6..8 {
        let c = s_drop
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate ValidateHandoff constraints.
///
/// Proves: certificate_hash is in the approved set (hash membership).
/// State: cap_root updated (routing entry for recipient), balance/fields unchanged.
pub fn constrain_validate_handoff(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_handoff = local[sel::VALIDATE_HANDOFF];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let cert_hash = local[PARAM_BASE + param::HANDOFF_CERT_HASH];
    let recipient_pk = local[PARAM_BASE + param::HANDOFF_RECIPIENT_PK];
    let approved_root = local[PARAM_BASE + param::HANDOFF_APPROVED_SET_ROOT];

    // Membership proof: aux[0] = hash(cert_hash, approved_root)
    let expected_membership = hash_2_to_1(cert_hash, approved_root);
    let aux_membership = local[AUX_BASE + 0];
    let c_member = s_handoff * (aux_membership - expected_membership);
    combined = combined + alpha_pow * c_member;
    alpha_pow = alpha_pow * alpha;

    // Cap root updated: new_cap_root = hash(old_cap_root, hash(recipient_pk, cert_hash))
    let routing_entry = hash_2_to_1(recipient_pk, cert_hash);
    let expected_new_cap = hash_2_to_1(old_cap_root, routing_entry);
    let c_cap = s_handoff * (new_cap_root - expected_new_cap);
    combined = combined + alpha_pow * c_cap;
    alpha_pow = alpha_pow * alpha;

    // Balance unchanged.
    let c_bal_lo = s_handoff * (new_bal_lo - old_bal_lo);
    combined = combined + alpha_pow * c_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_bal_hi = s_handoff * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_bal_hi;
    alpha_pow = alpha_pow * alpha;

    // All fields unchanged.
    for i in 0..8 {
        let c = s_handoff
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}
