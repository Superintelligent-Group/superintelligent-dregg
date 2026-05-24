//! Constraint blocks for storage queue effects:
//! AllocateQueue, EnqueueMessage, DequeueMessage, ResizeQueue, AtomicQueueTx, PipelineStep.

use crate::field::BabyBear;
use crate::poseidon2::hash_2_to_1;
use crate::effect_vm::{sel, state, param, AUX_BASE, STATE_BEFORE_BASE, STATE_AFTER_BASE, PARAM_BASE};
use crate::effect_vm::aux_off;

/// Accumulate AllocateQueue constraints.
///
/// Balance debit (capacity * cost_per_slot), field[4] = empty_queue_hash.
pub fn constrain_allocate_queue(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_alloc_queue = local[sel::ALLOCATE_QUEUE];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let capacity = local[PARAM_BASE + param::QUEUE_CAPACITY];
    let cost_per_slot = local[PARAM_BASE + param::QUEUE_COST_PER_SLOT];
    let alloc_cost = capacity * cost_per_slot;

    // Balance debit: new_bal_lo = old_bal_lo - alloc_cost.
    let c_aq_bal = s_alloc_queue * (new_bal_lo - old_bal_lo + alloc_cost);
    combined = combined + alpha_pow * c_aq_bal;
    alpha_pow = alpha_pow * alpha;
    let c_aq_hi = s_alloc_queue * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_aq_hi;
    alpha_pow = alpha_pow * alpha;

    // field[4] must become empty_queue_hash = hash_2_to_1(ZERO, ZERO).
    let empty_queue_hash = hash_2_to_1(BabyBear::ZERO, BabyBear::ZERO);
    let new_f4 = local[STATE_AFTER_BASE + state::FIELD_BASE + 4];
    let c_aq_root = s_alloc_queue * (new_f4 - empty_queue_hash);
    combined = combined + alpha_pow * c_aq_root;
    alpha_pow = alpha_pow * alpha;

    // Cap root unchanged.
    let c_aq_cap = s_alloc_queue * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_aq_cap;
    alpha_pow = alpha_pow * alpha;

    // Other fields (0..4, 5..8) unchanged.
    for i in 0..4 {
        let c = s_alloc_queue
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    for i in 5..8 {
        let c = s_alloc_queue
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate EnqueueMessage constraints.
///
/// Queue root hash chain, balance debit (deposit), optional program validation hash binding.
pub fn constrain_enqueue_message(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_enqueue = local[sel::ENQUEUE_MESSAGE];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let message_hash = local[PARAM_BASE + param::ENQUEUE_MSG_HASH];
    let deposit = local[PARAM_BASE + param::ENQUEUE_DEPOSIT];
    let sender_id = local[PARAM_BASE + param::ENQUEUE_SENDER];

    // Queue root transition: new_root = hash(old_root, message_hash).
    let old_queue_root = local[STATE_BEFORE_BASE + state::FIELD_BASE + 4];
    let expected_new_root = hash_2_to_1(old_queue_root, message_hash);
    let new_f4 = local[STATE_AFTER_BASE + state::FIELD_BASE + 4];
    let c_eq_root = s_enqueue * (new_f4 - expected_new_root);
    combined = combined + alpha_pow * c_eq_root;
    alpha_pow = alpha_pow * alpha;

    // Balance debit: new_bal_lo = old_bal_lo - deposit.
    let c_eq_bal = s_enqueue * (new_bal_lo - old_bal_lo + deposit);
    combined = combined + alpha_pow * c_eq_bal;
    alpha_pow = alpha_pow * alpha;
    let c_eq_hi = s_enqueue * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_eq_hi;
    alpha_pow = alpha_pow * alpha;

    // Cap root unchanged.
    let c_eq_cap = s_enqueue * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_eq_cap;
    alpha_pow = alpha_pow * alpha;

    // Other fields (0..4, 5..8) unchanged.
    for i in 0..4 {
        let c = s_enqueue
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    for i in 5..8 {
        let c = s_enqueue
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }

    // Queue program validation hash binding.
    let program_vk = local[PARAM_BASE + param::ENQUEUE_PROGRAM_VK];
    let validation_hash = local[AUX_BASE + 6];
    let program_vk_inv = local[AUX_BASE + 7];

    // Constraint 1: When program_vk != 0, validation_hash must equal expected.
    let inner_hash = hash_2_to_1(sender_id, message_hash);
    let expected_validation = hash_2_to_1(program_vk, inner_hash);
    let c_prog_valid = s_enqueue * program_vk * (validation_hash - expected_validation);
    combined = combined + alpha_pow * c_prog_valid;
    alpha_pow = alpha_pow * alpha;

    // Constraint 2: When program_vk == 0, validation_hash must be zero.
    let c_prog_zero =
        s_enqueue * (BabyBear::ONE - program_vk * program_vk_inv) * validation_hash;
    combined = combined + alpha_pow * c_prog_zero;
    alpha_pow = alpha_pow * alpha;

    (combined, alpha_pow)
}

/// Accumulate DequeueMessage constraints.
///
/// Queue root hash chain advance, balance credit (deposit refund).
pub fn constrain_dequeue_message(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_dequeue = local[sel::DEQUEUE_MESSAGE];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let expected_msg_hash = local[PARAM_BASE + param::DEQUEUE_EXPECTED_HASH];
    let deposit_refund = local[PARAM_BASE + param::DEQUEUE_DEPOSIT_REFUND];

    // Queue root advances: new_root = hash(old_root, expected_message_hash).
    let old_queue_root = local[STATE_BEFORE_BASE + state::FIELD_BASE + 4];
    let expected_new_root = hash_2_to_1(old_queue_root, expected_msg_hash);
    let new_f4 = local[STATE_AFTER_BASE + state::FIELD_BASE + 4];
    let c_dq_root = s_dequeue * (new_f4 - expected_new_root);
    combined = combined + alpha_pow * c_dq_root;
    alpha_pow = alpha_pow * alpha;

    // expected_message_hash must be non-zero (non-empty queue).
    let msg_inv = local[AUX_BASE + 1];
    let c_dq_nonempty = s_dequeue * (expected_msg_hash * msg_inv - BabyBear::ONE);
    combined = combined + alpha_pow * c_dq_nonempty;
    alpha_pow = alpha_pow * alpha;

    // Balance credit: new_bal_lo = old_bal_lo + deposit_refund.
    let c_dq_bal = s_dequeue * (new_bal_lo - old_bal_lo - deposit_refund);
    combined = combined + alpha_pow * c_dq_bal;
    alpha_pow = alpha_pow * alpha;
    let c_dq_hi = s_dequeue * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_dq_hi;
    alpha_pow = alpha_pow * alpha;

    // Cap root unchanged.
    let c_dq_cap = s_dequeue * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_dq_cap;
    alpha_pow = alpha_pow * alpha;

    // Other fields (0..4, 5..8) unchanged.
    for i in 0..4 {
        let c = s_dequeue
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    for i in 5..8 {
        let c = s_dequeue
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate ResizeQueue constraints.
///
/// Capacity update with sign-decomposed delta (Stage 2 honesty fix).
pub fn constrain_resize_queue(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_resize = local[sel::RESIZE_QUEUE];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let new_capacity = local[PARAM_BASE + param::RESIZE_NEW_CAPACITY];
    let old_capacity = local[PARAM_BASE + param::RESIZE_OLD_CAPACITY];
    let cost_per_slot = local[PARAM_BASE + param::RESIZE_COST_PER_SLOT];
    let delta_sign = local[AUX_BASE + aux_off::RESIZE_DELTA_SIGN];
    let delta_mag = local[AUX_BASE + aux_off::RESIZE_DELTA_MAG];
    let two = BabyBear::ONE + BabyBear::ONE;

    // (a) sign boolean.
    let c_rq_sign_bool = s_resize * delta_sign * (delta_sign - BabyBear::ONE);
    combined = combined + alpha_pow * c_rq_sign_bool;
    alpha_pow = alpha_pow * alpha;

    // (b) signed-delta binding.
    let c_rq_delta = s_resize
        * ((new_capacity - old_capacity) - delta_mag * (BabyBear::ONE - two * delta_sign));
    combined = combined + alpha_pow * c_rq_delta;
    alpha_pow = alpha_pow * alpha;

    // (c) balance change: grow => debit; shrink => no change.
    let resize_cost = delta_mag * cost_per_slot * (BabyBear::ONE - delta_sign);
    let c_rq_bal = s_resize * (new_bal_lo - old_bal_lo + resize_cost);
    combined = combined + alpha_pow * c_rq_bal;
    alpha_pow = alpha_pow * alpha;
    let c_rq_hi = s_resize * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_rq_hi;
    alpha_pow = alpha_pow * alpha;

    // field[5] must become new_capacity.
    let new_f5 = local[STATE_AFTER_BASE + state::FIELD_BASE + 5];
    let c_rq_cap_field = s_resize * (new_f5 - new_capacity);
    combined = combined + alpha_pow * c_rq_cap_field;
    alpha_pow = alpha_pow * alpha;

    // Cap root unchanged.
    let c_rq_cap = s_resize * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_rq_cap;
    alpha_pow = alpha_pow * alpha;

    // Queue root (field[4]) unchanged.
    let c_rq_f4 = s_resize
        * (local[STATE_AFTER_BASE + state::FIELD_BASE + 4]
            - local[STATE_BEFORE_BASE + state::FIELD_BASE + 4]);
    combined = combined + alpha_pow * c_rq_f4;
    alpha_pow = alpha_pow * alpha;

    // Other fields (0..4, 6..8) unchanged.
    for i in 0..4 {
        let c = s_resize
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    for i in 6..8 {
        let c = s_resize
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate AtomicQueueTx constraints.
///
/// Proves: field[4] transitions from combined_old_root to combined_new_root.
pub fn constrain_atomic_queue_tx(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_atomic_tx = local[sel::ATOMIC_QUEUE_TX];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let tx_hash_val = local[PARAM_BASE + param::ATOMIC_TX_HASH];
    let combined_old = local[PARAM_BASE + param::ATOMIC_TX_COMBINED_OLD_ROOT];
    let combined_new = local[PARAM_BASE + param::ATOMIC_TX_COMBINED_NEW_ROOT];
    let net_deposit = local[PARAM_BASE + param::ATOMIC_TX_NET_DEPOSIT];

    // field[4] must equal combined_old_root before.
    let old_f4 = local[STATE_BEFORE_BASE + state::FIELD_BASE + 4];
    let c_atx_old = s_atomic_tx * (old_f4 - combined_old);
    combined = combined + alpha_pow * c_atx_old;
    alpha_pow = alpha_pow * alpha;

    // field[4] must become combined_new_root.
    let new_f4 = local[STATE_AFTER_BASE + state::FIELD_BASE + 4];
    let c_atx_new = s_atomic_tx * (new_f4 - combined_new);
    combined = combined + alpha_pow * c_atx_new;
    alpha_pow = alpha_pow * alpha;

    // Binding constraint: aux[0] == hash(tx_hash, hash(combined_old, combined_new))
    let inner_hash = hash_2_to_1(combined_old, combined_new);
    let expected_binding = hash_2_to_1(tx_hash_val, inner_hash);
    let aux_binding = local[AUX_BASE + 0];
    let c_atx_bind = s_atomic_tx * (aux_binding - expected_binding);
    combined = combined + alpha_pow * c_atx_bind;
    alpha_pow = alpha_pow * alpha;

    // Balance debit: new_bal_lo = old_bal_lo - net_deposit.
    let c_atx_bal_lo = s_atomic_tx * (new_bal_lo - old_bal_lo + net_deposit);
    combined = combined + alpha_pow * c_atx_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_atx_bal_hi = s_atomic_tx * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_atx_bal_hi;
    alpha_pow = alpha_pow * alpha;

    // Cap root unchanged.
    let c_atx_cap = s_atomic_tx * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_atx_cap;
    alpha_pow = alpha_pow * alpha;

    // Other fields (0..4, 5..8) unchanged.
    for i in 0..4 {
        let c = s_atomic_tx
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    for i in 5..8 {
        let c = s_atomic_tx
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}

/// Accumulate PipelineStep constraints.
///
/// Proves a pipeline step correctly routed a message from source to sink queue.
pub fn constrain_pipeline_step(
    local: &[BabyBear],
    alpha: BabyBear,
    mut combined: BabyBear,
    mut alpha_pow: BabyBear,
) -> (BabyBear, BabyBear) {
    let s_pipeline = local[sel::PIPELINE_STEP];
    let old_bal_lo = local[STATE_BEFORE_BASE + state::BALANCE_LO];
    let old_bal_hi = local[STATE_BEFORE_BASE + state::BALANCE_HI];
    let new_bal_lo = local[STATE_AFTER_BASE + state::BALANCE_LO];
    let new_bal_hi = local[STATE_AFTER_BASE + state::BALANCE_HI];
    let old_cap_root = local[STATE_BEFORE_BASE + state::CAP_ROOT];
    let new_cap_root = local[STATE_AFTER_BASE + state::CAP_ROOT];
    let pipeline_id_val = local[PARAM_BASE + param::PIPELINE_ID];
    let source_old = local[PARAM_BASE + param::PIPELINE_SOURCE_OLD_ROOT];
    let source_new = local[PARAM_BASE + param::PIPELINE_SOURCE_NEW_ROOT];
    let sink_new = local[PARAM_BASE + param::PIPELINE_SINK_NEW_ROOT];
    let msg_hash = local[PARAM_BASE + param::PIPELINE_MESSAGE_HASH];

    // P1-5 fix: enforce pipeline_id != 0.
    let pipeline_id_inv = local[AUX_BASE + 6];
    let c_pipeline_nonzero =
        s_pipeline * (pipeline_id_val * pipeline_id_inv - BabyBear::ONE);
    combined = combined + alpha_pow * c_pipeline_nonzero;
    alpha_pow = alpha_pow * alpha;

    // Source dequeue constraint: source_new_root == hash(source_old_root, message_hash)
    let expected_source_new = hash_2_to_1(source_old, msg_hash);
    let c_ps_source = s_pipeline * (source_new - expected_source_new);
    combined = combined + alpha_pow * c_ps_source;
    alpha_pow = alpha_pow * alpha;

    // aux[0] must equal expected_source_new.
    let aux_expected = local[AUX_BASE + 0];
    let c_ps_aux = s_pipeline * (aux_expected - expected_source_new);
    combined = combined + alpha_pow * c_ps_aux;
    alpha_pow = alpha_pow * alpha;

    // field[4] must equal source_old_root before.
    let old_f4 = local[STATE_BEFORE_BASE + state::FIELD_BASE + 4];
    let c_ps_old = s_pipeline * (old_f4 - source_old);
    combined = combined + alpha_pow * c_ps_old;
    alpha_pow = alpha_pow * alpha;

    // field[4] must become source_new_root after.
    let new_f4 = local[STATE_AFTER_BASE + state::FIELD_BASE + 4];
    let c_ps_new = s_pipeline * (new_f4 - source_new);
    combined = combined + alpha_pow * c_ps_new;
    alpha_pow = alpha_pow * alpha;

    // aux[1] stores sink_new_root (pipeline binding).
    let aux_sink = local[AUX_BASE + 1];
    let c_ps_sink = s_pipeline * (aux_sink - sink_new);
    combined = combined + alpha_pow * c_ps_sink;
    alpha_pow = alpha_pow * alpha;

    // Balance unchanged.
    let c_ps_bal_lo = s_pipeline * (new_bal_lo - old_bal_lo);
    combined = combined + alpha_pow * c_ps_bal_lo;
    alpha_pow = alpha_pow * alpha;
    let c_ps_bal_hi = s_pipeline * (new_bal_hi - old_bal_hi);
    combined = combined + alpha_pow * c_ps_bal_hi;
    alpha_pow = alpha_pow * alpha;

    // Cap root unchanged.
    let c_ps_cap = s_pipeline * (new_cap_root - old_cap_root);
    combined = combined + alpha_pow * c_ps_cap;
    alpha_pow = alpha_pow * alpha;

    // Other fields (0..4, 5..8) unchanged.
    for i in 0..4 {
        let c = s_pipeline
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    for i in 5..8 {
        let c = s_pipeline
            * (local[STATE_AFTER_BASE + state::FIELD_BASE + i]
                - local[STATE_BEFORE_BASE + state::FIELD_BASE + i]);
        combined = combined + alpha_pow * c;
        alpha_pow = alpha_pow * alpha;
    }
    (combined, alpha_pow)
}
