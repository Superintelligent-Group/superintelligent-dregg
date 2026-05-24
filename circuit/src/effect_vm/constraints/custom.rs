//! Constraint blocks for Custom dispatch and the custom-count sum-check.

use crate::field::BabyBear;
use crate::effect_vm::{sel, state, STATE_BEFORE_BASE, STATE_AFTER_BASE, AUX_BASE};
use crate::effect_vm::aux_off;

/// Accumulate Custom (CellProgram dispatch) constraints: state continuity only.
///
/// SECURITY NOTE (Gap 5): Custom effects provide WEAKER guarantees than other
/// effect types. The Effect VM only enforces state continuity and proof
/// commitment binding. Verifiers MUST independently verify the external proof
/// against the committed program VK hash.
pub fn constrain_custom(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_custom = local[sel::CUSTOM];
    for i in 0..state::SIZE {
        let c = s_custom * (local[STATE_AFTER_BASE + i] - local[STATE_BEFORE_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate the Custom-effect count exclusive sum-check (CONSTRAINT GROUP 7).
///
/// Per `DESIGN-max-custom-effects.md` §6 step 3: transition constraint
///   `next.acc == this.acc + this.s_custom`
/// makes the cumulative s_custom count algebraically bound to
/// `PI[CUSTOM_EFFECT_COUNT]` (via boundary constraints in `boundary_constraints`).
///
/// Note: this function does NOT advance alpha_pow after the last constraint
/// (matching the original code comment).
pub fn constrain_custom_sum_check(
    local: &[BabyBear],
    next: &[BabyBear],
    alpha_pow: BabyBear,
    mut combined: BabyBear,
) -> BabyBear {
    let this_acc = local[AUX_BASE + aux_off::CUSTOM_COUNT_ACC];
    let next_acc = next[AUX_BASE + aux_off::CUSTOM_COUNT_ACC];
    let this_s_custom = local[sel::CUSTOM];
    // Exclusive-sum transition: next.acc == this.acc + this.s_custom
    let c_acc_step = next_acc - this_acc - this_s_custom;
    combined = combined + alpha_pow * c_acc_step;
    // alpha_pow not advanced — last constraint in eval_constraints.
    combined
}
