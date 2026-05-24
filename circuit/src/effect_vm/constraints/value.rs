//! Constraint blocks for value-transfer effects: Transfer, NoteSpend, NoteCreate.

use crate::field::BabyBear;
use crate::effect_vm::{sel, state, param, AUX_BASE, STATE_BEFORE_BASE, STATE_AFTER_BASE, PARAM_BASE};

/// Accumulate Transfer constraints.
pub fn constrain_transfer(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_transfer = local[sel::TRANSFER];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let p0 = local[PARAM_BASE + 0];
    let p1 = local[PARAM_BASE + 1];

    let two = BabyBear::new(2);
    let direction = p1;
    let amount = p0;
    // new_bal_lo == old_bal_lo + amount - 2*direction*amount
    let c_transfer_lo =
        s_transfer * (new_bal_lo - old_bal_lo - amount + two * direction * amount);
    combined = combined + alpha_pow * c_transfer_lo;
    alpha_pow = alpha_pow * alpha;

    // Transfer: hi limb unchanged (for single-limb amounts).
    let c_transfer_hi = s_transfer * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_transfer_hi;
    alpha_pow = alpha_pow * alpha;

    // Transfer: direction must be boolean.
    let c_transfer_dir = s_transfer * direction * (direction - BabyBear::ONE);
    combined = combined + alpha_pow * c_transfer_dir;
    alpha_pow = alpha_pow * alpha;

    // Transfer: cap_root and reserved unchanged.
    for i in [state::CAP_ROOT, state::RESERVED] {
        let c = s_transfer * (local[STATE_AFTER_BASE + i] - local[STATE_BEFORE_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    // Transfer: fields unchanged.
    for i in 0..8 {
        let c = s_transfer
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate NoteSpend constraints.
pub fn constrain_notespend(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_notespend = local[sel::NOTE_SPEND];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let p1 = local[PARAM_BASE + 1];
    let note_val_lo = p1;

    let c_ns_bal = s_notespend * (new_bal_lo - old_bal_lo - note_val_lo);
    combined = combined + alpha_pow * c_ns_bal;
    alpha_pow = alpha_pow * alpha;
    let c_ns_hi = s_notespend * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_ns_hi;
    alpha_pow = alpha_pow * alpha;
    let c_ns_cap = s_notespend * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_ns_cap;
    alpha_pow = alpha_pow * alpha;
    for i in 0..8 {
        let c = s_notespend
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate NoteCreate constraints.
pub fn constrain_notecreate(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_notecreate = local[sel::NOTE_CREATE];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let p1 = local[PARAM_BASE + 1];
    let nc_val_lo = p1;

    let c_nc_bal = s_notecreate * (new_bal_lo - old_bal_lo + nc_val_lo);
    combined = combined + alpha_pow * c_nc_bal;
    alpha_pow = alpha_pow * alpha;
    let c_nc_hi = s_notecreate * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_nc_hi;
    alpha_pow = alpha_pow * alpha;
    let c_nc_cap = s_notecreate * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_nc_cap;
    alpha_pow = alpha_pow * alpha;
    for i in 0..8 {
        let c = s_notecreate
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}
