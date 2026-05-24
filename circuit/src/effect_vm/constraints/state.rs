//! Constraint blocks for state-field effects: NoOp, SetField, GrantCapability.

use crate::field::BabyBear;
use crate::poseidon2::hash_2_to_1;
use crate::effect_vm::{
    sel, state, param,
    AUX_BASE, STATE_BEFORE_BASE, STATE_AFTER_BASE, PARAM_BASE,
};
use crate::effect_vm::aux_off;

/// Accumulate NoOp constraints: state_after == state_before for all state columns.
pub fn constrain_noop(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_noop = local[sel::NOOP];
    for i in 0..state::SIZE {
        let c = s_noop * (local[STATE_AFTER_BASE + i] - local[STATE_BEFORE_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate SetField constraints (including sealing-honesty bit-decomposition).
///
/// This block covers:
///  - Per-field non-target unchanged constraint
///  - Target-field sum constraint
///  - Balance + cap_root + reserved unchanged
///  - Stage 2: unconditional bit-decomposition of old_reserved (9 boolean checks + sum)
///  - Stage 2: s_setfield * bit_at_idx == 0 (cannot set sealed field)
///  - Stage 2: s_seal * seal_bit_at_idx == 0 (no double seal)
///  - Stage 2: s_unseal * (unseal_bit_at_idx - 1) == 0 (must be sealed to unseal)
///  - Range check: field_idx in {0..7}
#[allow(clippy::too_many_arguments)]
pub fn constrain_setfield(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear, [BabyBear; 8], BabyBear) {
    // This function also returns the l_bits and mode_bit for use by sealing constraints.
    let s_setfield = local[sel::SET_FIELD];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let field_index = local[PARAM_BASE + param::FIELD_INDEX];
    let new_value = local[PARAM_BASE + param::NEW_VALUE];

    // Non-target fields must be unchanged.
    for j in 0..8u32 {
        let old_fj = local[STATE_BEFORE_BASE + state::FIELD_BASE + j as usize];
        let new_fj = local[STATE_AFTER_BASE + state::FIELD_BASE + j as usize];
        let c = s_setfield * (field_index - BabyBear::new(j)) * (new_fj - old_fj);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    // Target field sum.
    let old_value_at_idx = local[AUX_BASE + 0];
    let mut field_diff_sum = BabyBear::ZERO;
    for j in 0..8 {
        let old_fj = local[STATE_BEFORE_BASE + state::FIELD_BASE + j];
        let new_fj = local[STATE_AFTER_BASE + state::FIELD_BASE + j];
        field_diff_sum = field_diff_sum + (new_fj - old_fj);
    }
    let c_setfield_sum = s_setfield * (field_diff_sum - (new_value - old_value_at_idx));
    combined = combined + alpha_pow * c_setfield_sum;
    alpha_pow = alpha_pow * alpha;

    // Balance and cap_root unchanged.
    let c_sf_bal_lo = s_setfield * (new_bal_lo - old_bal_lo);
    combined = combined + alpha_pow * c_sf_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_sf_bal_hi = s_setfield * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_sf_bal_hi;
    alpha_pow = alpha_pow * alpha;
    let c_sf_cap = s_setfield * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_sf_cap;
    alpha_pow = alpha_pow * alpha;
    // Stage 2: reserved unchanged across SetField.
    let sf_old_reserved = local[STATE_BEFORE_BASE + state::RESERVED];
    let sf_new_reserved = local[STATE_AFTER_BASE + state::RESERVED];
    let c_sf_reserved = s_setfield * (sf_new_reserved - sf_old_reserved);
    combined = combined + alpha_pow * c_sf_reserved;
    alpha_pow = alpha_pow * alpha;

    // Stage 2: unconditional bit-decomposition of old_reserved (every row).
    let b0 = local[AUX_BASE + aux_off::RESERVED_BIT_0];
    let b1 = local[AUX_BASE + aux_off::RESERVED_BIT_1];
    let b2 = local[AUX_BASE + aux_off::RESERVED_BIT_2];
    let b3 = local[AUX_BASE + aux_off::RESERVED_BIT_3];
    let b4 = local[AUX_BASE + aux_off::RESERVED_BIT_4];
    let b5 = local[AUX_BASE + aux_off::RESERVED_BIT_5];
    let b6 = local[AUX_BASE + aux_off::RESERVED_BIT_6];
    let b7 = local[AUX_BASE + aux_off::RESERVED_BIT_7];
    let mode_bit = local[AUX_BASE + aux_off::RESERVED_MODE];
    // Boolean constraints (unconditional, every row).
    for bit in [b0, b1, b2, b3, b4, b5, b6, b7, mode_bit].iter() {
        let cb = (*bit) * ((*bit) - BabyBear::ONE);
        combined = combined + alpha_pow * cb;
        alpha_pow = alpha_pow * alpha;
    }
    // Decomposition: Σ bi * 2^i + mode * 256 == old_reserved.
    let sf_old_reserved_dec = local[STATE_BEFORE_BASE + state::RESERVED];
    let reconstructed = b0
        + b1 * BabyBear::new(2)
        + b2 * BabyBear::new(4)
        + b3 * BabyBear::new(8)
        + b4 * BabyBear::new(16)
        + b5 * BabyBear::new(32)
        + b6 * BabyBear::new(64)
        + b7 * BabyBear::new(128)
        + mode_bit * BabyBear::new(256);
    let c_decomp = reconstructed - sf_old_reserved_dec;
    combined = combined + alpha_pow * c_decomp;
    alpha_pow = alpha_pow * alpha;

    let l_bits: [BabyBear; 8] = [b0, b1, b2, b3, b4, b5, b6, b7];

    // Lagrange-basis selection of the bit at field_idx.
    let bit_at_idx = lagrange_select(field_index, &l_bits);
    // s_setfield * bit_at_idx == 0  (cannot set a sealed field).
    let c_sf_not_sealed = s_setfield * bit_at_idx;
    combined = combined + alpha_pow * c_sf_not_sealed;
    alpha_pow = alpha_pow * alpha;

    // Stage 2: Seal: bit at field_idx must currently be 0 (no double-seal).
    let seal_bit_at_idx = lagrange_select(local[PARAM_BASE + param::SEAL_FIELD_IDX], &l_bits);
    let s_seal_early = local[sel::SEAL];
    let c_seal_no_double = s_seal_early * seal_bit_at_idx;
    combined = combined + alpha_pow * c_seal_no_double;
    alpha_pow = alpha_pow * alpha;

    // Stage 2: Unseal: bit at field_idx must currently be 1.
    let unseal_bit_at_idx = lagrange_select(local[PARAM_BASE + param::UNSEAL_FIELD_IDX], &l_bits);
    let s_unseal_early = local[sel::UNSEAL];
    let c_unseal_must_be_set = s_unseal_early * (unseal_bit_at_idx - BabyBear::ONE);
    combined = combined + alpha_pow * c_unseal_must_be_set;
    alpha_pow = alpha_pow * alpha;

    // Range check: field_idx in {0..7}.
    {
        let mut field_idx_range_product = BabyBear::ONE;
        for k in 0..8u32 {
            field_idx_range_product =
                field_idx_range_product * (field_index - BabyBear::new(k));
        }
        let c_field_idx_range = s_setfield * field_idx_range_product;
        combined = combined + alpha_pow * c_field_idx_range;
        alpha_pow = alpha_pow * alpha;
    }

    (combined, alpha_pow, l_bits, mode_bit)
}

/// Accumulate GrantCapability constraints.
pub fn constrain_grantcap(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_grantcap = local[sel::GRANT_CAP];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];

    let cap_entry_val = local[PARAM_BASE + param::CAP_ENTRY];
    let expected_new_cap = hash_2_to_1(old_cap_root, cap_entry_val);
    let c_grantcap = s_grantcap * (new_cap_root - expected_new_cap);
    combined = combined + alpha_pow * c_grantcap;
    alpha_pow = alpha_pow * alpha;

    let c_gc_bal_lo = s_grantcap * (new_bal_lo - old_bal_lo);
    combined = combined + alpha_pow * c_gc_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_gc_bal_hi = s_grantcap * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_gc_bal_hi;
    alpha_pow = alpha_pow * alpha;
    for i in 0..8 {
        let c = s_grantcap
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Lagrange-basis selection of `l_bits[field_idx]` for field_idx ∈ {0..7}.
/// Returns `l_bits[k]` when x == k, algebraically bound via degree-7 polynomial.
pub(super) fn lagrange_select(x: BabyBear, l_bits: &[BabyBear; 8]) -> BabyBear {
    let mut acc = BabyBear::ZERO;
    for k in 0..8usize {
        let mut num = BabyBear::ONE;
        let mut den = BabyBear::ONE;
        for j in 0..8usize {
            if j == k { continue; }
            num = num * (x - BabyBear::new(j as u32));
            let diff = if k > j {
                BabyBear::new((k - j) as u32)
            } else {
                BabyBear::ZERO - BabyBear::new((j - k) as u32)
            };
            den = den * diff;
        }
        let den_inv = den.inverse().expect("Lagrange denominator non-zero on {0..7}");
        acc = acc + num * den_inv * l_bits[k];
    }
    acc
}
