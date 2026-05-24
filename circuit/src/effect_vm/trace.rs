//! Witness (execution trace) generation for the Effect VM.

use crate::field::BabyBear;
use crate::poseidon2::hash_2_to_1;
use crate::effect_vm::{
    Effect, CellState, EffectVmContext,
    split_u64, fill_reserved_bits, compute_effects_hash, compute_effects_hash_4,
    EFFECT_VM_WIDTH, AUX_BASE, STATE_BEFORE_BASE, STATE_AFTER_BASE, PARAM_BASE,
    sel, param, pi, aux_off,
};

/// Generate the execution trace and public inputs for an effect VM proof.
///
/// # Arguments
/// * `initial_state` - The cell state before executing effects.
/// * `effects` - The sequence of effects to prove.
///
/// # Returns
/// (trace, public_inputs) suitable for `stark::prove`.
pub fn generate_effect_vm_trace(
    initial_state: &CellState,
    effects: &[Effect],
) -> (Vec<Vec<BabyBear>>, Vec<BabyBear>) {
    generate_effect_vm_trace_ext(
        initial_state,
        effects,
        EffectVmContext::default(),
    )
}

/// Stage 1 trace generator. Same as [`generate_effect_vm_trace`] but accepts
/// the widened PI inputs ([`EffectVmContext`]).
pub fn generate_effect_vm_trace_ext(
    initial_state: &CellState,
    effects: &[Effect],
    context: EffectVmContext,
) -> (Vec<Vec<BabyBear>>, Vec<BabyBear>) {
    assert!(!effects.is_empty(), "Need at least one effect");

    // ====================================================================
    // EXECUTOR-SIDE RANGE VALIDATION (o1vm audit mitigations)
    // ====================================================================
    // These checks run at proof generation time. They do NOT add constraints
    // to the STARK, but they prevent the executor from producing a trace with
    // out-of-range values that could exploit modular arithmetic.
    //
    // A verifier receiving a proof from an untrusted prover must additionally
    // verify that the final state (decoded from new_commitment PI) has valid
    // limb ranges. See `verify_balance_limb_ranges` below.
    // ====================================================================

    // Validate initial balance limbs are in range.
    let (init_lo, init_hi) = split_u64(initial_state.balance);
    assert!(
        init_lo.0 < (1 << 30),
        "Initial balance_lo out of range: {} >= 2^30",
        init_lo.0
    );
    assert!(
        init_hi.0 < (1 << 31),
        "Initial balance_hi out of range: {} >= 2^31 (exceeds BabyBear)",
        init_hi.0
    );

    // Validate field_idx bounds and balance underflow for all effects.
    // We track a running balance to catch underflow across multi-effect turns.
    {
        let mut running_balance = initial_state.balance;
        for effect in effects {
            match effect {
                Effect::SetField { field_idx, .. } => {
                    assert!(
                        *field_idx < 8,
                        "SetField field_idx out of bounds: {} (must be 0..7)",
                        field_idx
                    );
                }
                Effect::Transfer {
                    amount, direction, ..
                } => {
                    if *direction == 1 {
                        // Outgoing: check for underflow.
                        assert!(
                            *amount <= running_balance,
                            "Transfer underflow: amount {} > running balance {} \
                             (executor rejects; STARK constraint would wrap in BabyBear)",
                            amount,
                            running_balance
                        );
                        running_balance -= amount;
                    } else {
                        running_balance = running_balance.saturating_add(*amount);
                    }
                }
                Effect::NoteCreate { value, .. } => {
                    assert!(
                        *value <= running_balance,
                        "NoteCreate underflow: value {} > running balance {} \
                         (executor rejects; STARK constraint would wrap in BabyBear)",
                        value,
                        running_balance
                    );
                    running_balance -= value;
                }
                Effect::CreateObligation { stake_amount, .. } => {
                    assert!(
                        *stake_amount <= running_balance,
                        "CreateObligation underflow: stake {} > running balance {} \
                         (executor rejects; STARK constraint would wrap in BabyBear)",
                        stake_amount,
                        running_balance
                    );
                    running_balance -= stake_amount;
                }
                Effect::NoteSpend { value, .. } => {
                    running_balance = running_balance.saturating_add(*value);
                }
                Effect::FulfillObligation { stake_return, .. } => {
                    running_balance = running_balance.saturating_add(*stake_return);
                }
                Effect::SlashObligation { stake_amount, .. } => {
                    running_balance = running_balance.saturating_add(*stake_amount);
                }
                Effect::AllocateQueue {
                    capacity,
                    cost_per_slot,
                    ..
                } => {
                    let cost = (*capacity as u64) * (*cost_per_slot as u64);
                    assert!(
                        cost <= running_balance,
                        "AllocateQueue underflow: cost {} > running balance {}",
                        cost,
                        running_balance
                    );
                    running_balance -= cost;
                }
                Effect::EnqueueMessage { deposit_amount, .. } => {
                    assert!(
                        (*deposit_amount as u64) <= running_balance,
                        "EnqueueMessage underflow: deposit {} > running balance {}",
                        deposit_amount,
                        running_balance
                    );
                    running_balance -= *deposit_amount as u64;
                }
                Effect::DequeueMessage { deposit_refund, .. } => {
                    running_balance = running_balance.saturating_add(*deposit_refund as u64);
                }
                Effect::ResizeQueue {
                    new_capacity,
                    old_capacity,
                    cost_per_slot,
                    ..
                } => {
                    if *new_capacity > *old_capacity {
                        let delta = (*new_capacity - *old_capacity) as u64;
                        let cost = delta * (*cost_per_slot as u64);
                        assert!(
                            cost <= running_balance,
                            "ResizeQueue underflow: cost {} > running balance {}",
                            cost,
                            running_balance
                        );
                        running_balance -= cost;
                    }
                }
                Effect::AtomicQueueTx { net_deposit, .. } => {
                    assert!(
                        (*net_deposit as u64) <= running_balance,
                        "AtomicQueueTx underflow: net_deposit {} > running balance {}",
                        net_deposit,
                        running_balance
                    );
                    running_balance -= *net_deposit as u64;
                }
                _ => {}
            }
        }
    }

    // Determine trace height (pad to power of 2, minimum 2).
    // Stage 2 (REVIEW[stage1-acc-row0]): if the last real effect is a Custom,
    // we need at least one trailing NoOp row so the exclusive-sum boundary
    // `acc[last] == PI[CUSTOM_EFFECT_COUNT]` holds. Reserve a slot.
    let n_effects = effects.len();
    let need_extra_pad = matches!(effects.last(), Some(Effect::Custom { .. }));
    let trace_height = if need_extra_pad {
        (n_effects + 1).next_power_of_two().max(2)
    } else {
        n_effects.next_power_of_two().max(2)
    };

    let mut trace = Vec::with_capacity(trace_height);
    let mut current_state = initial_state.clone();

    // Track net balance delta.
    let mut net_delta: i64 = 0;

    for effect in effects {
        let mut row = vec![BabyBear::ZERO; EFFECT_VM_WIDTH];

        // Set selector.
        let sel_idx = match effect {
            Effect::NoOp => sel::NOOP,
            Effect::Transfer { .. } => sel::TRANSFER,
            Effect::SetField { .. } => sel::SET_FIELD,
            Effect::GrantCapability { .. } => sel::GRANT_CAP,
            Effect::NoteSpend { .. } => sel::NOTE_SPEND,
            Effect::NoteCreate { .. } => sel::NOTE_CREATE,
            Effect::CreateObligation { .. } => sel::CREATE_OBLIGATION,
            Effect::FulfillObligation { .. } => sel::FULFILL_OBLIGATION,
            Effect::Custom { .. } => sel::CUSTOM,
            Effect::SlashObligation { .. } => sel::SLASH_OBLIGATION,
            Effect::Seal { .. } => sel::SEAL,
            Effect::Unseal { .. } => sel::UNSEAL,
            Effect::MakeSovereign => sel::MAKE_SOVEREIGN,
            Effect::CreateCellFromFactory { .. } => sel::CREATE_CELL_FROM_FACTORY,
            Effect::ExportSturdyRef { .. } => sel::EXPORT_STURDY_REF,
            Effect::EnlivenRef { .. } => sel::ENLIVEN_REF,
            Effect::DropRef { .. } => sel::DROP_REF,
            Effect::ValidateHandoff { .. } => sel::VALIDATE_HANDOFF,
            Effect::AllocateQueue { .. } => sel::ALLOCATE_QUEUE,
            Effect::EnqueueMessage { .. } => sel::ENQUEUE_MESSAGE,
            Effect::DequeueMessage { .. } => sel::DEQUEUE_MESSAGE,
            Effect::ResizeQueue { .. } => sel::RESIZE_QUEUE,
            Effect::AtomicQueueTx { .. } => sel::ATOMIC_QUEUE_TX,
            Effect::PipelineStep { .. } => sel::PIPELINE_STEP,
        };
        row[sel_idx] = BabyBear::ONE;

        // Write state_before.
        let state_before_cols = current_state.to_trace_cols();
        for (i, &val) in state_before_cols.iter().enumerate() {
            row[STATE_BEFORE_BASE + i] = val;
        }

        // Apply effect and compute state_after + params.
        let mut new_state = current_state.clone();
        match effect {
            Effect::NoOp => {
                // No state change, no nonce increment for padding.
            }
            Effect::Transfer { amount, direction } => {
                let (lo, _hi) = split_u64(*amount);
                row[PARAM_BASE + param::AMOUNT] = lo;
                row[PARAM_BASE + param::DIRECTION] = BabyBear::new(*direction);

                if *direction == 1 {
                    // Outgoing.
                    new_state.balance = new_state.balance.saturating_sub(*amount);
                    net_delta -= *amount as i64;
                } else {
                    // Incoming.
                    new_state.balance = new_state.balance.saturating_add(*amount);
                    net_delta += *amount as i64;
                }
                new_state.nonce += 1;
            }
            Effect::SetField { field_idx, value } => {
                row[PARAM_BASE + param::FIELD_INDEX] = BabyBear::new(*field_idx);
                row[PARAM_BASE + param::NEW_VALUE] = *value;

                // Store old value at target index in aux[0] for the constraint.
                let idx = *field_idx as usize;
                row[AUX_BASE + 0] = current_state.fields[idx.min(7)];

                new_state.fields[idx.min(7)] = *value;
                new_state.nonce += 1;
            }
            Effect::GrantCapability { cap_entry } => {
                row[PARAM_BASE + param::CAP_ENTRY] = *cap_entry;

                let new_cap = hash_2_to_1(current_state.capability_root, *cap_entry);
                new_state.capability_root = new_cap;
                new_state.nonce += 1;
            }
            Effect::NoteSpend { nullifier, value } => {
                let (val_lo, val_hi) = split_u64(*value);
                row[PARAM_BASE + param::NULLIFIER] = *nullifier;
                row[PARAM_BASE + param::NOTE_VALUE_LO] = val_lo;
                row[PARAM_BASE + param::NOTE_VALUE_HI] = val_hi;

                new_state.balance = new_state.balance.saturating_add(*value);
                net_delta += *value as i64;
                new_state.nonce += 1;
            }
            Effect::NoteCreate { commitment, value } => {
                let (val_lo, val_hi) = split_u64(*value);
                row[PARAM_BASE + param::NOTE_COMMITMENT] = *commitment;
                row[PARAM_BASE + param::NOTE_VALUE_LO] = val_lo;
                row[PARAM_BASE + param::NOTE_VALUE_HI] = val_hi;

                new_state.balance = new_state.balance.saturating_sub(*value);
                net_delta -= *value as i64;
                new_state.nonce += 1;
            }
            Effect::CreateObligation {
                stake_amount,
                obligation_id,
                beneficiary_hash,
            } => {
                let (stake_lo, stake_hi) = split_u64(*stake_amount);
                row[PARAM_BASE + param::OBLIGATION_STAKE_LO] = stake_lo;
                row[PARAM_BASE + param::OBLIGATION_STAKE_HI] = stake_hi;
                row[PARAM_BASE + param::OBLIGATION_ID] = *obligation_id;
                row[PARAM_BASE + param::OBLIGATION_BENEFICIARY] = *beneficiary_hash;

                new_state.balance = new_state.balance.saturating_sub(*stake_amount);
                net_delta -= *stake_amount as i64;
                // Stage 2: cap_root advances to bind both obligation_id and beneficiary.
                let obligation_leaf = hash_2_to_1(*obligation_id, *beneficiary_hash);
                new_state.capability_root =
                    hash_2_to_1(new_state.capability_root, obligation_leaf);
                new_state.nonce += 1;
            }
            Effect::FulfillObligation {
                obligation_id,
                stake_return,
            } => {
                let (ret_lo, ret_hi) = split_u64(*stake_return);
                row[PARAM_BASE + param::FULFILL_OBLIGATION_ID] = *obligation_id;
                row[PARAM_BASE + param::FULFILL_RETURN_LO] = ret_lo;
                row[PARAM_BASE + param::FULFILL_RETURN_HI] = ret_hi;

                new_state.balance = new_state.balance.saturating_add(*stake_return);
                net_delta += *stake_return as i64;
                new_state.nonce += 1;
            }
            Effect::Custom {
                program_vk_hash,
                proof_commitment,
            } => {
                // Write VK hash into params[0..4].
                for i in 0..4 {
                    row[PARAM_BASE + param::CUSTOM_VK_HASH_BASE + i] = program_vk_hash[i];
                }
                // Write proof commitment into params[4..8].
                for i in 0..4 {
                    row[PARAM_BASE + param::CUSTOM_PROOF_COMMIT_BASE + i] = proof_commitment[i];
                }
                // Custom effects do NOT change state (state flows through unchanged).
                // The nonce still increments (it's a real effect, not padding).
                new_state.nonce += 1;
                // No balance change from the Effect VM perspective.
            }
            Effect::SlashObligation {
                obligation_id,
                stake_amount,
                beneficiary_hash,
            } => {
                let (stake_lo, stake_hi) = split_u64(*stake_amount);
                row[PARAM_BASE + param::SLASH_OBLIGATION_ID] = *obligation_id;
                row[PARAM_BASE + param::SLASH_STAKE_LO] = stake_lo;
                row[PARAM_BASE + param::SLASH_STAKE_HI] = stake_hi;
                row[PARAM_BASE + param::SLASH_BENEFICIARY] = *beneficiary_hash;
                // Slash credits the beneficiary: balance increases.
                new_state.balance = new_state.balance.saturating_add(*stake_amount);
                net_delta += *stake_amount as i64;
                // Update cap_root to reflect obligation removal.
                new_state.capability_root = hash_2_to_1(new_state.capability_root, *obligation_id);
                new_state.nonce += 1;
            }
            Effect::Seal { field_idx } => {
                row[PARAM_BASE + param::SEAL_FIELD_IDX] = BabyBear::new(*field_idx);
                // Stage 2: aux witness for 2^field_idx (constrained by Lagrange poly).
                row[AUX_BASE + aux_off::SEAL_POW2_IDX] = BabyBear::new(1u32 << field_idx);
                // Trace-gen-side check: bit must not already be set (no double-seal).
                assert!(
                    new_state.sealed_field_mask & (1 << field_idx) == 0,
                    "Seal: field {} already sealed (sealed_mask={:#b})",
                    field_idx,
                    new_state.sealed_field_mask,
                );
                new_state.sealed_field_mask |= 1 << field_idx;
                new_state.nonce += 1;
            }
            Effect::Unseal { field_idx, brand } => {
                row[PARAM_BASE + param::UNSEAL_FIELD_IDX] = BabyBear::new(*field_idx);
                row[PARAM_BASE + param::UNSEAL_BRAND] = *brand;
                // Store brand in aux for constraint checking.
                row[AUX_BASE + 6] = *brand;
                // Stage 2: aux witness for 2^field_idx.
                row[AUX_BASE + aux_off::SEAL_POW2_IDX] = BabyBear::new(1u32 << field_idx);
                // Trace-gen-side check: bit must be set (cannot unseal unsealed field).
                assert!(
                    new_state.sealed_field_mask & (1 << field_idx) != 0,
                    "Unseal: field {} not sealed (sealed_mask={:#b})",
                    field_idx,
                    new_state.sealed_field_mask,
                );
                new_state.sealed_field_mask &= !(1 << field_idx);
                new_state.nonce += 1;
            }
            Effect::MakeSovereign => {
                // Mode flag transitions from 0 to 1.
                new_state.mode_flag = 1;
                new_state.nonce += 1;
            }
            Effect::CreateCellFromFactory {
                factory_vk,
                child_vk_derived,
            } => {
                row[PARAM_BASE + param::FACTORY_VK_HASH] = *factory_vk;
                row[PARAM_BASE + param::CHILD_VK_DERIVED] = *child_vk_derived;
                // Store in aux columns for constraint verification.
                row[AUX_BASE + 6] = *factory_vk;
                row[AUX_BASE + 7] = *child_vk_derived;
                new_state.nonce += 1;
            }
            Effect::ExportSturdyRef {
                cell_id,
                permissions,
                random_seed,
                export_counter,
            } => {
                row[PARAM_BASE + param::EXPORT_CELL_ID] = *cell_id;
                row[PARAM_BASE + param::EXPORT_PERMISSIONS] = *permissions;
                row[PARAM_BASE + param::EXPORT_RANDOM_SEED] = *random_seed;
                row[PARAM_BASE + param::EXPORT_COUNTER] = BabyBear::new(*export_counter);

                // Compute swiss_number = hash(cell_id, hash(random_seed, counter))
                let inner_hash = hash_2_to_1(*random_seed, BabyBear::new(*export_counter));
                let swiss_number = hash_2_to_1(*cell_id, inner_hash);
                // Store computed swiss in aux[0] for constraint verification.
                row[AUX_BASE + 0] = swiss_number;

                // State: field[7] increments (export counter tracked there).
                new_state.fields[7] = new_state.fields[7] + BabyBear::ONE;
                new_state.nonce += 1;
            }
            Effect::EnlivenRef {
                swiss_number,
                presenter_id,
                expected_cell_id,
                expected_permissions,
            } => {
                row[PARAM_BASE + param::ENLIVEN_SWISS] = *swiss_number;
                row[PARAM_BASE + param::ENLIVEN_PRESENTER] = *presenter_id;
                row[PARAM_BASE + param::ENLIVEN_CELL_ID] = *expected_cell_id;
                row[PARAM_BASE + param::ENLIVEN_PERMISSIONS] = *expected_permissions;

                // Compute entry hash: hash(swiss, hash(cell_id, permissions))
                let inner = hash_2_to_1(*expected_cell_id, *expected_permissions);
                let entry_hash = hash_2_to_1(*swiss_number, inner);
                row[AUX_BASE + 0] = entry_hash;

                // State: field[6] increments (use_count tracked there).
                new_state.fields[6] = new_state.fields[6] + BabyBear::ONE;
                new_state.nonce += 1;
            }
            Effect::DropRef {
                cell_id,
                holder_federation,
                current_refcount,
            } => {
                row[PARAM_BASE + param::DROP_CELL_ID] = *cell_id;
                row[PARAM_BASE + param::DROP_HOLDER_FED] = *holder_federation;
                row[PARAM_BASE + param::DROP_REFCOUNT] = BabyBear::new(*current_refcount);

                // Prove refcount > 0: store inverse in aux[0].
                // The constraint checks refcount * inverse == 1.
                assert!(
                    *current_refcount > 0,
                    "DropRef: current_refcount must be > 0"
                );
                let rc_field = BabyBear::new(*current_refcount);
                // Compute modular inverse of refcount in BabyBear.
                row[AUX_BASE + 0] = rc_field.inverse().expect("refcount is non-zero");

                // State: field[5] decrements (refcount tracked there).
                new_state.fields[5] = new_state.fields[5] - BabyBear::ONE;
                new_state.nonce += 1;
            }
            Effect::ValidateHandoff {
                certificate_hash,
                recipient_pk,
                introducer_pk,
                approved_set_root,
            } => {
                row[PARAM_BASE + param::HANDOFF_CERT_HASH] = *certificate_hash;
                row[PARAM_BASE + param::HANDOFF_RECIPIENT_PK] = *recipient_pk;
                row[PARAM_BASE + param::HANDOFF_INTRODUCER_PK] = *introducer_pk;
                row[PARAM_BASE + param::HANDOFF_APPROVED_SET_ROOT] = *approved_set_root;

                // Membership proof: aux[0] = hash(cert_hash, approved_set_root)
                let membership = hash_2_to_1(*certificate_hash, *approved_set_root);
                row[AUX_BASE + 0] = membership;

                // State: cap_root updated with routing entry.
                // new_cap = hash(old_cap, hash(recipient_pk, cert_hash))
                let routing_entry = hash_2_to_1(*recipient_pk, *certificate_hash);
                new_state.capability_root = hash_2_to_1(new_state.capability_root, routing_entry);
                new_state.nonce += 1;
            }
            Effect::AllocateQueue {
                capacity,
                owner_quota_id,
                cost_per_slot,
            } => {
                row[PARAM_BASE + param::QUEUE_CAPACITY] = BabyBear::new(*capacity);
                row[PARAM_BASE + param::QUEUE_OWNER_QUOTA] = *owner_quota_id;
                row[PARAM_BASE + param::QUEUE_COST_PER_SLOT] = BabyBear::new(*cost_per_slot);

                // Allocation cost = capacity * cost_per_slot.
                let alloc_cost = (*capacity as u64) * (*cost_per_slot as u64);
                new_state.balance = new_state.balance.saturating_sub(alloc_cost);
                net_delta -= alloc_cost as i64;

                // Queue root = empty queue hash = hash_2_to_1(ZERO, ZERO).
                // Store in field[4] by convention (queue_root slot).
                let empty_queue_hash = hash_2_to_1(BabyBear::ZERO, BabyBear::ZERO);
                new_state.fields[4] = empty_queue_hash;

                // Store capacity in aux[0] for constraint verification.
                row[AUX_BASE + 0] = empty_queue_hash;

                new_state.nonce += 1;
            }
            Effect::EnqueueMessage {
                message_hash,
                deposit_amount,
                sender_id,
                queue_len,
                program_vk,
            } => {
                row[PARAM_BASE + param::ENQUEUE_MSG_HASH] = *message_hash;
                row[PARAM_BASE + param::ENQUEUE_DEPOSIT] = BabyBear::new(*deposit_amount);
                row[PARAM_BASE + param::ENQUEUE_SENDER] = *sender_id;
                row[PARAM_BASE + param::ENQUEUE_QUEUE_LEN] = BabyBear::new(*queue_len);
                row[PARAM_BASE + param::ENQUEUE_PROGRAM_VK] = *program_vk;

                // Queue root transition: new_root = hash(old_root, message_hash).
                let old_queue_root = new_state.fields[4];
                let new_queue_root = hash_2_to_1(old_queue_root, *message_hash);
                new_state.fields[4] = new_queue_root;

                // Deposit deducted from sender's balance.
                new_state.balance = new_state.balance.saturating_sub(*deposit_amount as u64);
                net_delta -= *deposit_amount as i64;

                // Store new queue root in aux[0] for constraint verification.
                row[AUX_BASE + 0] = new_queue_root;

                // Program validation hash binding (aux[6] and aux[7]).
                // NOTE: aux[2..5] are reserved for PI values on row 0.
                // When program_vk != 0, compute and store the validation hash.
                // When program_vk == 0, both are zero (backward compatible).
                if *program_vk != BabyBear::ZERO {
                    let inner = hash_2_to_1(*sender_id, *message_hash);
                    let validation_hash = hash_2_to_1(*program_vk, inner);
                    row[AUX_BASE + 6] = validation_hash;
                    // aux[7] = inverse of program_vk (for the zero-check constraint).
                    row[AUX_BASE + 7] = program_vk.inverse().expect("program_vk is non-zero");
                }
                // else: aux[6] and aux[7] remain ZERO (default).

                new_state.nonce += 1;
            }
            Effect::DequeueMessage {
                expected_message_hash,
                deposit_refund,
            } => {
                row[PARAM_BASE + param::DEQUEUE_EXPECTED_HASH] = *expected_message_hash;
                row[PARAM_BASE + param::DEQUEUE_DEPOSIT_REFUND] = BabyBear::new(*deposit_refund);

                // Non-empty queue proof: store inverse of expected_message_hash in aux[1].
                assert!(
                    *expected_message_hash != BabyBear::ZERO,
                    "DequeueMessage: expected_message_hash must be non-zero (non-empty queue)"
                );
                row[AUX_BASE + 1] = expected_message_hash
                    .inverse()
                    .expect("message hash is non-zero");

                // Queue root advances: new_root = hash(old_root, expected_message_hash).
                // (In a full implementation this would be a Merkle removal, but for
                // the circuit we use a hash chain advance for soundness.)
                let old_queue_root = new_state.fields[4];
                let new_queue_root = hash_2_to_1(old_queue_root, *expected_message_hash);
                new_state.fields[4] = new_queue_root;

                // Deposit refund credited to balance.
                new_state.balance = new_state.balance.saturating_add(*deposit_refund as u64);
                net_delta += *deposit_refund as i64;

                // Store new queue root in aux[0] for constraint verification.
                row[AUX_BASE + 0] = new_queue_root;

                new_state.nonce += 1;
            }
            Effect::ResizeQueue {
                new_capacity,
                queue_id,
                cost_per_slot,
                old_capacity,
            } => {
                row[PARAM_BASE + param::RESIZE_NEW_CAPACITY] = BabyBear::new(*new_capacity);
                row[PARAM_BASE + param::RESIZE_QUEUE_ID] = *queue_id;
                row[PARAM_BASE + param::RESIZE_COST_PER_SLOT] = BabyBear::new(*cost_per_slot);
                row[PARAM_BASE + param::RESIZE_OLD_CAPACITY] = BabyBear::new(*old_capacity);

                // Stage 2: signed-delta witness for sound shrink handling.
                let (delta_sign, delta_mag) = if *new_capacity >= *old_capacity {
                    (0u32, *new_capacity - *old_capacity)
                } else {
                    (1u32, *old_capacity - *new_capacity)
                };
                row[AUX_BASE + aux_off::RESIZE_DELTA_SIGN] = BabyBear::new(delta_sign);
                row[AUX_BASE + aux_off::RESIZE_DELTA_MAG] = BabyBear::new(delta_mag);

                // If growing, debit balance for delta * cost_per_slot.
                if *new_capacity > *old_capacity {
                    let delta = (*new_capacity - *old_capacity) as u64;
                    let cost = delta * (*cost_per_slot as u64);
                    new_state.balance = new_state.balance.saturating_sub(cost);
                    net_delta -= cost as i64;
                }
                // Capacity update is reflected in the state commitment via field[5]
                // (we use field[5] as the queue capacity slot for ResizeQueue).
                new_state.fields[5] = BabyBear::new(*new_capacity);

                new_state.nonce += 1;
            }
            Effect::AtomicQueueTx {
                op_count,
                tx_hash,
                combined_old_root,
                combined_new_root,
                net_deposit,
            } => {
                row[PARAM_BASE + param::ATOMIC_TX_OP_COUNT] = BabyBear::new(*op_count);
                row[PARAM_BASE + param::ATOMIC_TX_HASH] = *tx_hash;
                row[PARAM_BASE + param::ATOMIC_TX_COMBINED_OLD_ROOT] = *combined_old_root;
                row[PARAM_BASE + param::ATOMIC_TX_COMBINED_NEW_ROOT] = *combined_new_root;
                row[PARAM_BASE + param::ATOMIC_TX_NET_DEPOSIT] = BabyBear::new(*net_deposit);

                // State transition: field[4] changes from combined_old_root to combined_new_root.
                // The circuit constrains that field[4] == combined_old_root before and
                // becomes combined_new_root after, binding the atomic transition.
                new_state.fields[4] = *combined_new_root;

                // Balance debit by net_deposit (sum of deposits paid minus refunds received).
                new_state.balance = new_state.balance.saturating_sub(*net_deposit as u64);
                net_delta -= *net_deposit as i64;

                // Auxiliary witness: aux[0] = hash(tx_hash, hash(combined_old_root, combined_new_root))
                // This binds the transaction to the specific state transition.
                let inner = hash_2_to_1(*combined_old_root, *combined_new_root);
                let binding_hash = hash_2_to_1(*tx_hash, inner);
                row[AUX_BASE + 0] = binding_hash;

                new_state.nonce += 1;
            }
            Effect::PipelineStep {
                pipeline_id,
                source_old_root,
                source_new_root,
                sink_new_root,
                message_hash,
            } => {
                row[PARAM_BASE + param::PIPELINE_ID] = *pipeline_id;
                row[PARAM_BASE + param::PIPELINE_SOURCE_OLD_ROOT] = *source_old_root;
                row[PARAM_BASE + param::PIPELINE_SOURCE_NEW_ROOT] = *source_new_root;
                row[PARAM_BASE + param::PIPELINE_SINK_NEW_ROOT] = *sink_new_root;
                row[PARAM_BASE + param::PIPELINE_MESSAGE_HASH] = *message_hash;

                // State transition: field[4] (source queue root) changes from
                // source_old_root to source_new_root (message dequeued from source).
                new_state.fields[4] = *source_new_root;

                // Auxiliary witness:
                // aux[0] = hash(source_old_root, message_hash) = expected source_new_root
                //   (proves dequeue: source_new_root == hash_chain_dequeue(source_old, msg))
                // aux[1] = sink_new_root (stored for external verification of sink transition)
                // aux[6] = pipeline_id^-1 (P1-5 fix: forces pipeline_id != 0)
                let expected_source_new = hash_2_to_1(*source_old_root, *message_hash);
                row[AUX_BASE + 0] = expected_source_new;
                row[AUX_BASE + 1] = *sink_new_root;
                row[AUX_BASE + 6] = pipeline_id
                    .inverse()
                    .expect("PipelineStep pipeline_id must be non-zero");

                new_state.nonce += 1;
            }
        }

        // Refresh state commitment.
        new_state.refresh_commitment();

        // Fill state commitment tree intermediate columns (aux[8..10]).
        // These are constrained by the evaluator to match hash_4_to_1 computations
        // on the state_after columns.
        let (inter1, inter2, inter3) = CellState::compute_commitment_intermediates(
            new_state.balance,
            new_state.nonce,
            &new_state.fields,
            new_state.capability_root,
        );
        row[AUX_BASE + aux_off::STATE_INTER1] = inter1;
        row[AUX_BASE + aux_off::STATE_INTER2] = inter2;
        row[AUX_BASE + aux_off::STATE_INTER3] = inter3;

        // Stage 2 (sealing honesty): bit-decompose OLD reserved on every row.
        // The constraint in eval_constraints requires that
        //   Σ b_i * 2^i + mode * 256 == old_reserved
        // hold unconditionally for every row.
        fill_reserved_bits(&mut row, current_state.sealed_field_mask, current_state.mode_flag);

        // Write state_after.
        let state_after_cols = new_state.to_trace_cols();
        for (i, &val) in state_after_cols.iter().enumerate() {
            row[STATE_AFTER_BASE + i] = val;
        }

        trace.push(row);
        current_state = new_state;
    }

    // Compute effects hash and net delta for public inputs.
    let (effects_hash_lo, effects_hash_hi) = compute_effects_hash(effects);
    let (delta_mag, delta_sign) = if net_delta < 0 {
        ((-net_delta) as u32, 1u32)
    } else {
        (net_delta as u32, 0u32)
    };

    // Fill aux columns on the first row with public-input-bound values.
    // Stage 1: effects_hash is widened to 4 felts; positions 0..1 are bound
    // to AUX[4..5] via boundary constraints (preserves the legacy 2-felt
    // witness binding), positions 2..3 are PI-only (see AUDIT[stage1-pi-only-bound]).
    let effects_hash_4_witness = compute_effects_hash_4(effects);
    if !trace.is_empty() {
        trace[0][AUX_BASE + 2] = BabyBear::new(delta_mag);
        trace[0][AUX_BASE + 3] = BabyBear::new(delta_sign);
        trace[0][AUX_BASE + 4] = effects_hash_4_witness[0];
        trace[0][AUX_BASE + 5] = effects_hash_4_witness[1];
    }
    // Silence unused warnings on the legacy 2-felt return values.
    let _ = (effects_hash_lo, effects_hash_hi);

    // Pad with NoOp rows.
    for _ in n_effects..trace_height {
        let mut row = vec![BabyBear::ZERO; EFFECT_VM_WIDTH];
        row[sel::NOOP] = BabyBear::ONE; // NoOp selector

        // State before = current state (carried from last real row).
        let state_cols = current_state.to_trace_cols();
        for (i, &val) in state_cols.iter().enumerate() {
            row[STATE_BEFORE_BASE + i] = val;
        }
        // State after = same (NoOp doesn't change state).
        for (i, &val) in state_cols.iter().enumerate() {
            row[STATE_AFTER_BASE + i] = val;
        }

        // Fill state commitment tree intermediates for padding rows too.
        let (inter1, inter2, inter3) = CellState::compute_commitment_intermediates(
            current_state.balance,
            current_state.nonce,
            &current_state.fields,
            current_state.capability_root,
        );
        row[AUX_BASE + aux_off::STATE_INTER1] = inter1;
        row[AUX_BASE + aux_off::STATE_INTER2] = inter2;
        row[AUX_BASE + aux_off::STATE_INTER3] = inter3;

        // Stage 2 (sealing honesty): bit-decompose OLD reserved.
        fill_reserved_bits(&mut row, current_state.sealed_field_mask, current_state.mode_flag);

        trace.push(row);
        // current_state stays the same for padding.
    }

    // Stage 2 sum-check (REVIEW[stage1-acc-row0] resolution): populate
    // aux[CUSTOM_COUNT_ACC] as the EXCLUSIVE running sum of `s_custom`
    // indicators. Convention: acc[i] = count of s_custom rows in [0..i)
    // (NOT including row i). With this convention:
    //   - acc[0] == 0 always (pinned by row-0 boundary)
    //   - Transition: next.acc == this.acc + this.s_custom (Group 7)
    //   - acc[last] == total count, pinned to PI[CUSTOM_EFFECT_COUNT] by
    //     the last-row boundary.
    //
    // For the last-row boundary to equal the total custom count, the last
    // row must contribute 0 to the running sum — i.e., the last row must
    // be a NoOp pad row. The pad loop above already pads with NoOp; the
    // `need_extra_pad` check at trace_height computation guarantees a NoOp
    // slot exists when the last real effect is Custom.
    {
        let mut acc: u32 = 0;
        for i in 0..trace.len() {
            // Exclusive sum: record acc BEFORE adding this row's contribution.
            trace[i][AUX_BASE + aux_off::CUSTOM_COUNT_ACC] = BabyBear::new(acc);
            if trace[i][sel::CUSTOM] == BabyBear::ONE {
                acc = acc.saturating_add(1);
            }
        }
    }

    // Collect custom effect entries for public inputs.
    let custom_entries: Vec<_> = effects
        .iter()
        .filter_map(|e| {
            if let Effect::Custom {
                program_vk_hash,
                proof_commitment,
            } = e
            {
                Some((*program_vk_hash, *proof_commitment))
            } else {
                None
            }
        })
        .collect();
    let custom_count = custom_entries.len();
    assert!(
        custom_count <= context.max_custom_effects as usize,
        "Too many custom effects: {} (max {})",
        custom_count,
        context.max_custom_effects
    );
    assert!(
        context.max_custom_effects <= pi::MAX_CUSTOM_EFFECTS_HARD_CAP,
        "max_custom_effects {} exceeds hard cap {}",
        context.max_custom_effects,
        pi::MAX_CUSTOM_EFFECTS_HARD_CAP,
    );

    // Build public inputs in the Stage 1 widened layout (see `pi` module).
    let pi_len = pi::BASE_COUNT + custom_count * pi::CUSTOM_ENTRY_SIZE;
    let mut public_inputs = vec![BabyBear::ZERO; pi_len];

    // ---- Commitments (4 felts each) ----
    let old_commit_4 = CellState::compute_commitment_4(
        initial_state.balance,
        initial_state.nonce,
        &initial_state.fields,
        initial_state.capability_root,
    );
    let new_commit_4 = CellState::compute_commitment_4(
        current_state.balance,
        current_state.nonce,
        &current_state.fields,
        current_state.capability_root,
    );
    for i in 0..pi::OLD_COMMIT_LEN {
        public_inputs[pi::OLD_COMMIT_BASE + i] = old_commit_4[i];
    }
    for i in 0..pi::NEW_COMMIT_LEN {
        public_inputs[pi::NEW_COMMIT_BASE + i] = new_commit_4[i];
    }

    // ---- Effects hash (4 felts) ----
    let effects_hash_4 = compute_effects_hash_4(effects);
    for i in 0..pi::EFFECTS_HASH_LEN {
        public_inputs[pi::EFFECTS_HASH_BASE + i] = effects_hash_4[i];
    }
    // Suppress unused-variable warning for the legacy 2-felt form.
    let _ = (effects_hash_lo, effects_hash_hi);

    // ---- Balance limbs (P0-1) ----
    let (i_lo, i_hi) = split_u64(initial_state.balance);
    let (f_lo, f_hi) = split_u64(current_state.balance);
    public_inputs[pi::INIT_BAL_LO] = i_lo;
    public_inputs[pi::INIT_BAL_HI] = i_hi;
    public_inputs[pi::FINAL_BAL_LO] = f_lo;
    public_inputs[pi::FINAL_BAL_HI] = f_hi;

    // ---- Net delta (P0-1) ----
    public_inputs[pi::NET_DELTA_MAG] = BabyBear::new(delta_mag);
    public_inputs[pi::NET_DELTA_SIGN] = BabyBear::new(delta_sign);

    // ---- Stage 1 additions ----
    public_inputs[pi::CURRENT_BLOCK_HEIGHT] =
        BabyBear::new((context.current_block_height & 0x7FFF_FFFF) as u32);
    public_inputs[pi::MAX_CUSTOM_EFFECTS] = BabyBear::new(context.max_custom_effects as u32);
    public_inputs[pi::CUSTOM_EFFECT_COUNT] = BabyBear::new(custom_count as u32);
    for i in 0..pi::APPROVED_HANDOFFS_LEN {
        public_inputs[pi::APPROVED_HANDOFFS_BASE + i] = context.approved_handoffs_root[i];
    }

    // ---- Custom proof entries ----
    for (i, (vk_hash, proof_commit)) in custom_entries.iter().enumerate() {
        let base = pi::CUSTOM_PROOFS_BASE + i * pi::CUSTOM_ENTRY_SIZE;
        for j in 0..4 {
            public_inputs[base + j] = vk_hash[j];
        }
        for j in 0..4 {
            public_inputs[base + 4 + j] = proof_commit[j];
        }
    }

    assert_eq!(public_inputs.len(), pi_len);
    (trace, public_inputs)
}

/// Encode a signed balance delta as (magnitude, sign_bit) for public inputs.
pub fn encode_net_delta(delta: i64) -> (BabyBear, BabyBear) {
    if delta < 0 {
        (BabyBear::new((-delta) as u32), BabyBear::ONE)
    } else {
        (BabyBear::new(delta as u32), BabyBear::ZERO)
    }
}

/// Extract the net balance delta from public inputs.
pub fn extract_net_delta(public_inputs: &[BabyBear]) -> Option<i64> {
    if public_inputs.len() < pi::BASE_COUNT {
        return None;
    }
    let magnitude = public_inputs[pi::NET_DELTA_MAG].0 as i64;
    let sign_bit = public_inputs[pi::NET_DELTA_SIGN].0;
    if sign_bit == 1 {
        Some(-magnitude)
    } else {
        Some(magnitude)
    }
}

/// Extract the custom proof commitments from public inputs.
/// Returns a vec of (program_vk_hash, proof_commitment) tuples.
pub fn extract_custom_proof_commitments(
    public_inputs: &[BabyBear],
) -> Vec<([BabyBear; 4], [BabyBear; 4])> {
    if public_inputs.len() < pi::BASE_COUNT {
        return Vec::new();
    }
    let custom_count = public_inputs[pi::CUSTOM_EFFECT_COUNT].0 as usize;
    let mut result = Vec::with_capacity(custom_count);
    for i in 0..custom_count {
        let base = pi::CUSTOM_PROOFS_BASE + i * pi::CUSTOM_ENTRY_SIZE;
        if base + pi::CUSTOM_ENTRY_SIZE > public_inputs.len() {
            break;
        }
        let vk_hash = [
            public_inputs[base],
            public_inputs[base + 1],
            public_inputs[base + 2],
            public_inputs[base + 3],
        ];
        let proof_commit = [
            public_inputs[base + 4],
            public_inputs[base + 5],
            public_inputs[base + 6],
            public_inputs[base + 7],
        ];
        result.push((vk_hash, proof_commit));
    }
    result
}

// ============================================================================
// Verifier-side range validation (executor/relay nodes)
// ============================================================================

/// Verify that balance limbs in a CellState are within valid ranges.
///
/// This function implements the executor-side mitigation for the balance limb
/// overflow vulnerability (o1vm audit finding #1). The STARK proof alone does
/// NOT constrain balance limbs to their declared bit-widths. Verifiers MUST
/// call this after proof verification to ensure the final state is well-formed.
///
/// Returns `Ok(())` if limbs are valid, or an error describing the violation.
pub fn verify_balance_limb_ranges(state: &CellState) -> Result<(), String> {
    let (lo, hi) = split_u64(state.balance);

    // balance_lo must fit in 30 bits.
    if lo.0 >= (1 << 30) {
        return Err(format!(
            "balance_lo out of range: {} >= 2^30 (max {})",
            lo.0,
            (1u32 << 30) - 1
        ));
    }

    // balance_hi must fit in 34 bits AND be < BabyBear prime.
    // Since BabyBear prime is 2^31 - 2^27 + 1, and hi < 2^34 could exceed it,
    // we check that hi < 2^31 (conservative; BabyBear::new already reduces mod p).
    if hi.0 >= (1 << 31) {
        return Err(format!(
            "balance_hi out of range: {} >= 2^31 (exceeds BabyBear field)",
            hi.0
        ));
    }

    // Verify reconstruction: lo + hi * 2^30 == balance.
    let reconstructed = (lo.0 as u64) | ((hi.0 as u64) << 30);
    if reconstructed != state.balance {
        return Err(format!(
            "balance limb reconstruction mismatch: lo={} hi={} reconstructs to {} but balance is {}",
            lo.0, hi.0, reconstructed, state.balance
        ));
    }

    Ok(())
}

/// Verify that a final CellState (after proof verification) has a valid
/// state commitment matching its declared fields.
///
/// This is the executor-side defense against interior-row limb manipulation:
/// even if a malicious prover used out-of-range limbs on interior rows, the
/// final commitment must match the declared final state.
pub fn verify_state_integrity(state: &CellState) -> Result<(), String> {
    // Check balance limb ranges.
    verify_balance_limb_ranges(state)?;

    // Verify commitment matches the state.
    let expected_commit = CellState::compute_commitment(
        state.balance,
        state.nonce,
        &state.fields,
        state.capability_root,
    );
    if state.state_commitment != expected_commit {
        return Err(format!(
            "state_commitment mismatch: declared {:?} but computed {:?}",
            state.state_commitment, expected_commit
        ));
    }

    Ok(())
}

/// P2-2 / P0-1 helper: range-check the INIT_BAL_* and FINAL_BAL_* PIs that
/// were added in the P0-1 fix.
///
/// The Group 6 algebraic constraint binds `NET_DELTA = FINAL - INIT` over the
/// BabyBear field. Without range checks on the limbs, a verifier could (in
/// principle) accept PIs where `INIT_BAL_LO` exceeds 2^30, allowing the
/// modular subtraction in `actual_delta = (FINAL - INIT) mod p` to wrap and
/// satisfy a forged `NET_DELTA` value. The honest prover/executor never
/// produces such PIs (limb ranges are asserted at trace-generation time), but
/// an untrusted-prover scenario should call this on every received proof.
///
/// Returns Ok if the PIs are well-formed, or an Err describing the violation.
pub fn verify_balance_limb_pis(public_inputs: &[BabyBear]) -> Result<(), String> {
    if public_inputs.len() < pi::BASE_COUNT {
        return Err(format!(
            "PI vector too short: {} < {}",
            public_inputs.len(),
            pi::BASE_COUNT
        ));
    }
    for (label, idx) in &[
        ("INIT_BAL_LO", pi::INIT_BAL_LO),
        ("FINAL_BAL_LO", pi::FINAL_BAL_LO),
    ] {
        let v = public_inputs[*idx].0;
        if v >= (1u32 << 30) {
            return Err(format!(
                "{} out of range: {} >= 2^30 (boundary-pinned balance_lo \
                 must fit in 30 bits)",
                label, v
            ));
        }
    }
    for (label, idx) in &[
        ("INIT_BAL_HI", pi::INIT_BAL_HI),
        ("FINAL_BAL_HI", pi::FINAL_BAL_HI),
    ] {
        let v = public_inputs[*idx].0;
        if v >= (1u32 << 31) {
            return Err(format!(
                "{} out of range: {} >= 2^31 (exceeds BabyBear field)",
                label, v
            ));
        }
    }
    // NET_DELTA_SIGN must be boolean (Group 5 enforces this in-circuit, but
    // we also check externally for defense-in-depth).
    let sign = public_inputs[pi::NET_DELTA_SIGN].0;
    if sign > 1 {
        return Err(format!(
            "NET_DELTA_SIGN must be 0 or 1; got {}",
            sign
        ));
    }
    // NET_DELTA_MAG must fit in 30 bits to match the per-limb subtraction
    // domain (otherwise modular wrap could occur in the algebraic check).
    let mag = public_inputs[pi::NET_DELTA_MAG].0;
    if mag >= (1u32 << 30) {
        return Err(format!(
            "NET_DELTA_MAG out of range: {} >= 2^30",
            mag
        ));
    }
    Ok(())
}
