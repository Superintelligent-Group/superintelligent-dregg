/// Public input layout.
///
/// Stage 1 widening (`EFFECT-VM-SHAPE-A.md`): commitments grow from 1 felt
/// (~31-bit binding) to 4 felts (~124-bit binding), via the typed
/// `Commitment4<T>` framework (`pyana_commit::typed`). Position 0 of each
/// 4-tuple corresponds to the in-trace `state::STATE_COMMIT` continuity
/// column; positions 1..3 are bound to the canonical cell state by the
/// executor's PI matching loop (it recomputes all 4 deterministically from
/// the stored canonical bytes via `pyana_commit::typed::canonical_32_to_felts_4`).
///
/// AUDIT[stage1-trace-widen]: For Stage 1, the trace `state::STATE_COMMIT`
/// remains a 1-column continuity hash (Constraint Group 4 unchanged). The
/// extra 3 PI elements get their security from the executor PI matching
/// loop. Stage 2 (`EFFECT-VM-SHAPE-A.md` Phase 1) widens the trace column.

// ---- Commitments (Stage 1 widened to 4 felts each, ~124-bit) ----
/// Old state commitment, 4-felt Poseidon2 form.
pub const OLD_COMMIT_BASE: usize = 0;
pub const OLD_COMMIT_LEN: usize = 4;
/// New state commitment, 4-felt Poseidon2 form.
pub const NEW_COMMIT_BASE: usize = 4;
pub const NEW_COMMIT_LEN: usize = 4;
/// Effects-tree hash, 4-felt Poseidon2 form. Promotes the prior 2-felt
/// (lo+synthetic-hi) form to 4 felts; synthetic-hi is dropped.
pub const EFFECTS_HASH_BASE: usize = 8;
pub const EFFECTS_HASH_LEN: usize = 4;

// ---- Backwards-compatible aliases (position 0 only) ----
/// Legacy alias: position 0 of OLD_COMMIT_BASE (single-felt continuity binding).
pub const OLD_COMMIT: usize = OLD_COMMIT_BASE;
/// Legacy alias: position 0 of NEW_COMMIT_BASE.
pub const NEW_COMMIT: usize = NEW_COMMIT_BASE;
/// Legacy alias: position 0 of EFFECTS_HASH_BASE.
pub const EFFECTS_HASH_LO: usize = EFFECTS_HASH_BASE;
/// Legacy alias: position 1 of EFFECTS_HASH_BASE. AUDIT[stage1-effects-hash]:
/// callers reading this should switch to absorbing all 4 elements via the
/// EFFECTS_HASH_LEN range; the prior synthetic-hi binding is replaced by
/// independent Poseidon2 squeezes.
pub const EFFECTS_HASH_HI: usize = EFFECTS_HASH_BASE + 1;

// ---- Per-cell balance limbs (P0-1 net_delta binding) ----
/// Initial balance low limb (30 bits) — pinned to row 0 state_before.
pub const INIT_BAL_LO: usize = 12;
/// Initial balance high limb — pinned to row 0 state_before.
pub const INIT_BAL_HI: usize = 13;
/// Final balance low limb — pinned to last row state_after.
pub const FINAL_BAL_LO: usize = 14;
/// Final balance high limb — pinned to last row state_after.
pub const FINAL_BAL_HI: usize = 15;

// ---- Net balance delta (P0-1 binding) ----
pub const NET_DELTA_MAG: usize = 16;
pub const NET_DELTA_SIGN: usize = 17;

// ---- Stage 1 additions (per EFFECT-VM-SHAPE-A.md G, E, F) ----
/// Federation block height supplied by the verifier. Used by effects
/// that take a timeout (escrow refund, bridge cancel) — those land in
/// later stages; the PI slot exists now so they have it.
pub const CURRENT_BLOCK_HEIGHT: usize = 18;
/// Per-cell maximum custom effects (from cell program manifest).
/// Verifier supplies from `cell.program.max_custom_effects`.
pub const MAX_CUSTOM_EFFECTS: usize = 19;
/// Number of custom effects in this turn (0 if none). The AIR enforces
/// `Σ s_custom == PI[CUSTOM_EFFECT_COUNT]` (sum-check, soundness
/// prerequisite per `DESIGN-max-custom-effects.md` §7 threat 3).
pub const CUSTOM_EFFECT_COUNT: usize = 20;

// ---- CapTP federation-state root (Stage 1 prep; populated in Stage 7) ----
/// Federation-scoped approved-handoffs Merkle root, 4-felt Poseidon2 form.
/// Initial value: empty-tree sentinel (Commitment4::empty()).
pub const APPROVED_HANDOFFS_BASE: usize = 21;
pub const APPROVED_HANDOFFS_LEN: usize = 4;

// ---- Custom proof commitments ----
/// For each custom effect i (0..custom_count):
///   PI[CUSTOM_PROOFS_BASE + i*8 + 0..4] = custom_program_vk_hash (4 elements)
///   PI[CUSTOM_PROOFS_BASE + i*8 + 4..8] = custom_proof_commitment (4 elements)
pub const CUSTOM_PROOFS_BASE: usize = 25;
/// Base public inputs (without custom proof data).
pub const BASE_COUNT: usize = 25;
/// Elements per custom effect entry in PI (4 vk_hash + 4 proof_commit).
pub const CUSTOM_ENTRY_SIZE: usize = 8;

// ---- Hard cap on declared max_custom_effects ----
/// Hard ceiling: a cell declaring more than this is refused at registration
/// time. Per `DESIGN-max-custom-effects.md` §5, bounds worst-case verifier
/// child-proof work to ~3.2s/turn at 50ms/proof.
pub const MAX_CUSTOM_EFFECTS_HARD_CAP: u8 = 64;
/// Soft cap: the recommended workspace ceiling. Cells declaring up to this
/// are uncontroversial; cells declaring 17..64 should justify the choice.
pub const MAX_CUSTOM_EFFECTS_SOFT_CAP: u8 = 16;
/// Default value for cells that don't declare a per-cell max. Matches the
/// pre-Stage-1 workspace constant.
pub const MAX_CUSTOM_EFFECTS_DEFAULT: u8 = 4;

// AUDIT[stage1-pi-only-bound]: PI[OLD_COMMIT_BASE+1..+4],
// PI[NEW_COMMIT_BASE+1..+4], PI[EFFECTS_HASH_BASE+1..+4], and the entire
// PI[APPROVED_HANDOFFS_BASE..+4] are bound only by the executor's PI
// matching loop (deterministic recomputation from cell/federation
// state), not by per-row AIR constraints. Stage 2 may add aux columns
// to anchor positions 1..3 of state-commit forms inside the trace.
