//! Constraint blocks for obligation effects: CreateObligation, FulfillObligation, SlashObligation.

use crate::field::BabyBear;
use crate::poseidon2::hash_2_to_1;
use crate::effect_vm::{sel, state, param, STATE_BEFORE_BASE, STATE_AFTER_BASE, PARAM_BASE};

/// Accumulate CreateObligation constraints.
pub fn constrain_create_obligation(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_create_obligation = local[sel::CREATE_OBLIGATION];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let p0 = local[PARAM_BASE + 0];
    let stake_lo = p0;

    let c_co_bal = s_create_obligation * (new_bal_lo - old_bal_lo + stake_lo);
    combined = combined + alpha_pow * c_co_bal;
    alpha_pow = alpha_pow * alpha;
    let c_co_hi = s_create_obligation * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_co_hi;
    alpha_pow = alpha_pow * alpha;
    // Cap_root advance: encodes obligation_id + beneficiary.
    let obligation_id = local[PARAM_BASE + param::OBLIGATION_ID];
    let obligation_beneficiary = local[PARAM_BASE + param::OBLIGATION_BENEFICIARY];
    let obligation_leaf = hash_2_to_1(obligation_id, obligation_beneficiary);
    let expected_co_cap = hash_2_to_1(old_cap_root, obligation_leaf);
    let c_co_cap = s_create_obligation * (new_cap_root - expected_co_cap);
    combined = combined + alpha_pow * c_co_cap;
    alpha_pow = alpha_pow * alpha;
    for i in 0..8 {
        let c = s_create_obligation
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate FulfillObligation constraints.
pub fn constrain_fulfill_obligation(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_fulfill_obligation = local[sel::FULFILL_OBLIGATION];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let p1 = local[PARAM_BASE + 1];
    let return_lo = p1;

    let c_fo_bal = s_fulfill_obligation * (new_bal_lo - old_bal_lo - return_lo);
    combined = combined + alpha_pow * c_fo_bal;
    alpha_pow = alpha_pow * alpha;
    let c_fo_hi = s_fulfill_obligation * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_fo_hi;
    alpha_pow = alpha_pow * alpha;
    let c_fo_cap = s_fulfill_obligation * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_fo_cap;
    alpha_pow = alpha_pow * alpha;
    for i in 0..8 {
        let c = s_fulfill_obligation
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate SlashObligation constraints.
pub fn constrain_slash_obligation(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_slash = local[sel::SLASH_OBLIGATION];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let slash_stake_lo = local[PARAM_BASE + param::SLASH_STAKE_LO];

    let c_slash_bal = s_slash * (new_bal_lo - old_bal_lo - slash_stake_lo);
    combined = combined + alpha_pow * c_slash_bal;
    alpha_pow = alpha_pow * alpha;
    let c_slash_hi = s_slash * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_slash_hi;
    alpha_pow = alpha_pow * alpha;
    let slash_obligation_id = local[PARAM_BASE + param::SLASH_OBLIGATION_ID];
    let expected_slash_cap = hash_2_to_1(old_cap_root, slash_obligation_id);
    let c_slash_cap = s_slash * (new_cap_root - expected_slash_cap);
    combined = combined + alpha_pow * c_slash_cap;
    alpha_pow = alpha_pow * alpha;
    for i in 0..8 {
        let c = s_slash
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}
