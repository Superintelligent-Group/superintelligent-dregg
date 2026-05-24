//! Constraint blocks for sealing effects: Seal, Unseal, MakeSovereign.

use crate::field::BabyBear;
use crate::effect_vm::{sel, state, param, AUX_BASE, STATE_BEFORE_BASE, STATE_AFTER_BASE, PARAM_BASE};
use crate::effect_vm::aux_off;

/// Lagrange-basis polynomial `L(x)` that maps x=k to `2^k` for k in {0..7}.
/// Used by Seal/Unseal to verify `aux_pow2 == 2^field_idx` algebraically.
pub fn lagrange_pow2(x: BabyBear) -> BabyBear {
    let mut result = BabyBear::ZERO;
    for k in 0..8u32 {
        let mut num = BabyBear::ONE;
        let mut den = BabyBear::ONE;
        for j in 0..8u32 {
            if j == k { continue; }
            num = num * (x - BabyBear::new(j));
            let diff = if k > j {
                BabyBear::new(k - j)
            } else {
                BabyBear::ZERO - BabyBear::new(j - k)
            };
            den = den * diff;
        }
        let den_inv = den.inverse().expect("Lagrange denominator non-zero on {0..7}");
        result = result + num * den_inv * BabyBear::new(1u32 << k);
    }
    result
}

/// Accumulate Seal constraints.
///
/// Requires `mode_bit` from the bit-decomposition (computed in `constrain_setfield`).
pub fn constrain_seal(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_seal = local[sel::SEAL];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let old_reserved_seal = local[STATE_BEFORE_BASE + state::RESERVED];
    let new_reserved_seal = local[STATE_AFTER_BASE + state::RESERVED];
    let seal_pow2 = local[AUX_BASE + aux_off::SEAL_POW2_IDX];

    let c_seal_bal_lo = s_seal * (new_bal_lo - old_bal_lo);
    combined = combined + alpha_pow * c_seal_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_seal_bal_hi = s_seal * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_seal_bal_hi;
    alpha_pow = alpha_pow * alpha;
    let c_seal_cap = s_seal * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_seal_cap;
    alpha_pow = alpha_pow * alpha;
    for i in 0..8 {
        let c = s_seal
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    // Stage 2: reserved increases by 2^field_idx (sets the bit).
    let c_seal_reserved = s_seal * (new_reserved_seal - old_reserved_seal - seal_pow2);
    combined = combined + alpha_pow * c_seal_reserved;
    alpha_pow = alpha_pow * alpha;
    // Stage 2: aux_pow2 == 2^field_idx (Lagrange over {0..7}).
    let seal_field_idx_a = local[PARAM_BASE + param::SEAL_FIELD_IDX];
    let c_seal_pow2_check = s_seal * (seal_pow2 - lagrange_pow2(seal_field_idx_a));
    combined = combined + alpha_pow * c_seal_pow2_check;
    alpha_pow = alpha_pow * alpha;

    // RANGE CHECK: Seal field_idx must be in {0..7}.
    {
        let seal_field_idx = local[PARAM_BASE + param::SEAL_FIELD_IDX];
        let mut seal_idx_range_product = BabyBear::ONE;
        for k in 0..8u32 {
            seal_idx_range_product =
                seal_idx_range_product * (seal_field_idx - BabyBear::new(k));
        }
        let c_seal_idx_range = s_seal * seal_idx_range_product;
        combined = combined + alpha_pow * c_seal_idx_range;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate Unseal constraints.
pub fn constrain_unseal(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_unseal = local[sel::UNSEAL];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let old_reserved_unseal = local[STATE_BEFORE_BASE + state::RESERVED];
    let new_reserved_unseal = local[STATE_AFTER_BASE + state::RESERVED];
    let unseal_pow2 = local[AUX_BASE + aux_off::SEAL_POW2_IDX];

    let c_unseal_bal_lo = s_unseal * (new_bal_lo - old_bal_lo);
    combined = combined + alpha_pow * c_unseal_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_unseal_bal_hi = s_unseal * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_unseal_bal_hi;
    alpha_pow = alpha_pow * alpha;
    let c_unseal_cap = s_unseal * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_unseal_cap;
    alpha_pow = alpha_pow * alpha;
    for i in 0..8 {
        let c = s_unseal
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    // Stage 2: reserved decreases by 2^field_idx (clears the bit).
    let c_unseal_reserved = s_unseal * (old_reserved_unseal - new_reserved_unseal - unseal_pow2);
    combined = combined + alpha_pow * c_unseal_reserved;
    alpha_pow = alpha_pow * alpha;
    // Stage 2: aux_pow2 == 2^field_idx.
    let unseal_field_idx_a = local[PARAM_BASE + param::UNSEAL_FIELD_IDX];
    let c_unseal_pow2_check = s_unseal * (unseal_pow2 - lagrange_pow2(unseal_field_idx_a));
    combined = combined + alpha_pow * c_unseal_pow2_check;
    alpha_pow = alpha_pow * alpha;

    // RANGE CHECK: Unseal field_idx must be in {0..7}.
    {
        let unseal_field_idx = local[PARAM_BASE + param::UNSEAL_FIELD_IDX];
        let mut unseal_idx_range_product = BabyBear::ONE;
        for k in 0..8u32 {
            unseal_idx_range_product =
                unseal_idx_range_product * (unseal_field_idx - BabyBear::new(k));
        }
        let c_unseal_idx_range = s_unseal * unseal_idx_range_product;
        combined = combined + alpha_pow * c_unseal_idx_range;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate MakeSovereign constraints.
///
/// `mode_bit` is the mode bit from the bit-decomposition (from `constrain_setfield`).
pub fn constrain_make_sovereign(
    local: &[BabyBear],
    mode_bit: BabyBear,
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_makesov = local[sel::MAKE_SOVEREIGN];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let old_reserved = local[STATE_BEFORE_BASE + state::RESERVED];
    let new_reserved = local[STATE_AFTER_BASE + state::RESERVED];

    let c_sov_mode = s_makesov * (new_reserved - old_reserved - BabyBear::new(256));
    combined = combined + alpha_pow * c_sov_mode;
    alpha_pow = alpha_pow * alpha;
    // Stage 2 (MakeSovereign once-only): mode bit must currently be 0.
    let c_sov_was_managed = s_makesov * mode_bit;
    combined = combined + alpha_pow * c_sov_was_managed;
    alpha_pow = alpha_pow * alpha;
    let c_sov_bal_lo = s_makesov * (new_bal_lo - old_bal_lo);
    combined = combined + alpha_pow * c_sov_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_sov_bal_hi = s_makesov * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_sov_bal_hi;
    alpha_pow = alpha_pow * alpha;
    let c_sov_cap = s_makesov * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_sov_cap;
    alpha_pow = alpha_pow * alpha;
    for i in 0..8 {
        let c = s_makesov
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}
