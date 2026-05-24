//! Constraint block for CreateCellFromFactory.

use crate::field::BabyBear;
use crate::effect_vm::{sel, state, STATE_BEFORE_BASE, STATE_AFTER_BASE};

/// Accumulate CreateCellFromFactory constraints: state flows through unchanged.
pub fn constrain_create_cell_from_factory(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_factory = local[sel::CREATE_CELL_FROM_FACTORY];
    for i in 0..state::SIZE {
        let c = s_factory * (local[STATE_AFTER_BASE + i] - local[STATE_BEFORE_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}
