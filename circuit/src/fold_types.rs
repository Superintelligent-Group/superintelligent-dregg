//! Fold types, witnesses, and helpers extracted from fold_air.
//!
//! The `StarkAir` implementation for `FoldStarkAir` remains in [`super::fold_air`].

use crate::constraint_prover::{Air, Constraint, ConstraintProver};
use crate::field::BabyBear;
use crate::merkle_types::{MerkleAir, MerkleLevelWitness, MerkleWitness};
use crate::poseidon2::{hash_fact, hash_many};
use crate::stark::{self, StarkProof};

pub const FOLD_AIR_WIDTH: usize = 12;

pub mod col {
    pub const ROW_TYPE: usize = 0;
    pub const FACT_HASH: usize = 1;
    pub const MEMBERSHIP_ROOT: usize = 2;
    pub const OLD_ROOT: usize = 3;
    pub const NEW_ROOT: usize = 4;
    pub const REMOVAL_COUNT: usize = 5;
    pub const CHECK_COUNT: usize = 6;
    pub const FACT_PRED: usize = 7;
    pub const FACT_TERM_START: usize = 8;
    pub const HASH_VALID: usize = 11;
}

#[derive(Clone, Debug)]
pub struct RemovedFact {
    pub predicate: BabyBear,
    pub terms: [BabyBear; 3],
    pub membership_proof: Option<MerkleWitness>,
}

impl RemovedFact {
    pub fn hash(&self) -> BabyBear {
        hash_fact(self.predicate, &self.terms)
    }

    pub fn verify_membership(&self, old_root: BabyBear) -> Option<BabyBear> {
        let proof = self.membership_proof.as_ref()?;
        if proof.leaf_hash != self.hash() {
            return None;
        }
        let air = MerkleAir::new(proof.clone());
        let result = ConstraintProver::verify(&air);
        if !result.is_valid() {
            return None;
        }
        if proof.expected_root != old_root {
            return None;
        }
        Some(proof.expected_root)
    }
}

#[derive(Clone, Debug)]
pub struct FoldWitness {
    pub old_root: BabyBear,
    pub new_root: BabyBear,
    pub removed_facts: Vec<RemovedFact>,
    pub num_added_checks: usize,
    /// Cryptographic commitment to the added checks (124-bit WideHash).
    ///
    /// This binds the actual check content to the fold proof, preventing a malicious
    /// prover from claiming checks they didn't actually add. When `num_added_checks == 0`,
    /// this MUST be `WideHash::ZERO` (backwards-compatible identity).
    ///
    /// Computed as: `WideHash::from_poseidon2("pyana-checks-v1", &[check_1_hash, ...])`.
    pub added_checks_commitment: crate::binding::WideHash,
}

pub fn compute_root_transition_hash(
    old_root: BabyBear,
    new_root: BabyBear,
    removed_fact_hashes: &[BabyBear],
    added_checks_commitment: &crate::binding::WideHash,
) -> BabyBear {
    let mut elements = Vec::with_capacity(3 + removed_fact_hashes.len() + 4);
    elements.push(old_root);
    elements.push(new_root);
    elements.extend_from_slice(removed_fact_hashes);
    // Include the 4 elements of the checks commitment in the transition hash.
    // When no checks are added, these are all ZERO (identity element for hashing),
    // but still included to ensure the hash domain is consistent.
    elements.extend_from_slice(added_checks_commitment.as_slice());
    hash_many(&elements)
}

/// Verify that the fold's root transition is sound.
///
/// Checks:
/// 1. Each removed fact has a valid Merkle membership proof against `old_root`.
/// 2. Computes the root transition hash binding (old_root, new_root, fact_hashes).
///
/// The returned hash is used as public input pi[4] in the FoldAir. The verifier
/// (bridge layer) MUST independently compute `new_root` by rebuilding the Poseidon2
/// Merkle tree from old leaves minus removed leaves. This binding via pi[4] ensures
/// a malicious prover cannot claim an arbitrary `new_root` without producing a
/// Poseidon2 collision.
///
/// Returns `None` if any membership proof is missing or invalid.
pub fn verify_root_transition(witness: &FoldWitness) -> Option<BabyBear> {
    for fact in &witness.removed_facts {
        if fact.verify_membership(witness.old_root).is_none() {
            return None;
        }
    }
    let fact_hashes: Vec<BabyBear> = witness.removed_facts.iter().map(|f| f.hash()).collect();
    Some(compute_root_transition_hash(
        witness.old_root,
        witness.new_root,
        &fact_hashes,
        &witness.added_checks_commitment,
    ))
}

pub struct FoldAir {
    pub witness: FoldWitness,
}

impl FoldAir {
    pub fn new(witness: FoldWitness) -> Self {
        Self { witness }
    }
}

impl Air for FoldAir {
    fn trace_width(&self) -> usize {
        FOLD_AIR_WIDTH
    }
    fn num_public_inputs(&self) -> usize {
        6
    }

    fn constraints(&self) -> Vec<Constraint> {
        vec![
            Constraint {
                name: "row_type_binary".into(),
                eval: Box::new(|row, _, _| {
                    let rt = row[col::ROW_TYPE];
                    rt * (rt - BabyBear::ONE)
                }),
            },
            Constraint {
                name: "membership_root_matches_old_root".into(),
                eval: Box::new(|row, _, _| {
                    let is_removal = BabyBear::ONE - row[col::ROW_TYPE];
                    is_removal * (row[col::MEMBERSHIP_ROOT] - row[col::OLD_ROOT])
                }),
            },
            Constraint {
                name: "hash_valid_binary".into(),
                eval: Box::new(|row, _, _| {
                    let hv = row[col::HASH_VALID];
                    hv * (hv - BabyBear::ONE)
                }),
            },
            Constraint {
                name: "removal_hash_required".into(),
                eval: Box::new(|row, _, _| {
                    let is_removal = BabyBear::ONE - row[col::ROW_TYPE];
                    is_removal * (BabyBear::ONE - row[col::HASH_VALID])
                }),
            },
            Constraint {
                name: "fact_hash_correct".into(),
                eval: Box::new(|row, _, _| {
                    let is_removal = BabyBear::ONE - row[col::ROW_TYPE];
                    let expected = hash_fact(
                        row[col::FACT_PRED],
                        &[
                            row[col::FACT_TERM_START],
                            row[col::FACT_TERM_START + 1],
                            row[col::FACT_TERM_START + 2],
                        ],
                    );
                    is_removal * (row[col::FACT_HASH] - expected)
                }),
            },
            Constraint {
                name: "old_root_consistent".into(),
                eval: Box::new(|row, _, pi| row[col::OLD_ROOT] - pi[0]),
            },
            Constraint {
                name: "new_root_consistent".into(),
                eval: Box::new(|row, _, pi| row[col::NEW_ROOT] - pi[1]),
            },
            Constraint {
                name: "removal_count_increment".into(),
                eval: Box::new(|row, next_row, _| {
                    if let Some(next) = next_row {
                        let is_removal = BabyBear::ONE - row[col::ROW_TYPE];
                        let is_next_removal = BabyBear::ONE - next[col::ROW_TYPE];
                        is_removal
                            * is_next_removal
                            * (next[col::REMOVAL_COUNT] - row[col::REMOVAL_COUNT] - BabyBear::ONE)
                    } else {
                        BabyBear::ZERO
                    }
                }),
            },
            Constraint {
                name: "delta_nonempty".into(),
                eval: Box::new(|row, _, _| {
                    let is_summary = row[col::ROW_TYPE];
                    let total = row[col::REMOVAL_COUNT] + row[col::CHECK_COUNT];
                    if is_summary == BabyBear::ONE && total == BabyBear::ZERO {
                        BabyBear::ONE
                    } else {
                        BabyBear::ZERO
                    }
                }),
            },
            // SECURITY: This constraint binds the summary row's MEMBERSHIP_ROOT
            // to pi[4], which is computed as:
            //   Poseidon2(old_root || new_root || removed_fact_hashes || checks_commitment)
            //
            // Since pi[4] commits to new_root AND added_checks_commitment via a
            // collision-resistant hash, a malicious prover cannot substitute a different
            // new_root or different checks without producing a Poseidon2 collision.
            // The verifier MUST independently compute pi[4] from the actual tree rebuild
            // and the actual checks commitment, which is done by build_fold_witnesses()
            // in the bridge.
            //
            // pi[5] is the added_checks_commitment: Poseidon2(check_1_hash || check_2_hash || ...).
            // This is included in the root transition hash (pi[4]), cryptographically binding
            // the actual check content to the proof.
            Constraint {
                name: "root_transition_binding".into(),
                eval: Box::new(|row, _, pi| {
                    let is_summary = row[col::ROW_TYPE];
                    is_summary * (row[col::MEMBERSHIP_ROOT] - pi[4])
                }),
            },
            // SECURITY: When check_count == 0, the checks commitment must be ZERO.
            // This prevents a prover from sneaking a non-zero commitment with zero count.
            // When check_count > 0, the commitment is non-zero (enforced by Poseidon2
            // producing non-zero outputs for non-empty inputs, assuming no collisions
            // with zero). The meaningful binding comes from pi[4] including the commitment.
            Constraint {
                name: "checks_commitment_zero_when_no_checks".into(),
                eval: Box::new(|row, _, pi| {
                    let is_summary = row[col::ROW_TYPE];
                    // When check_count (pi[3]) is zero, checks_commitment (pi[5]) must be zero.
                    // We only enforce this on summary rows to avoid over-constraining.
                    // Note: this is a public-input-only constraint (doesn't touch trace values
                    // beyond ROW_TYPE), so it acts as a well-formedness check on the verifier's inputs.
                    if pi[3] == BabyBear::ZERO {
                        is_summary * pi[5]
                    } else {
                        BabyBear::ZERO
                    }
                }),
            },
        ]
    }

    fn first_row_constraints(&self) -> Vec<Constraint> {
        vec![Constraint {
            name: "first_removal_count".into(),
            eval: Box::new(|row, _, _| {
                let is_removal = BabyBear::ONE - row[col::ROW_TYPE];
                is_removal * (row[col::REMOVAL_COUNT] - BabyBear::ONE)
            }),
        }]
    }

    fn last_row_constraints(&self) -> Vec<Constraint> {
        vec![
            Constraint {
                name: "last_row_is_summary".into(),
                eval: Box::new(|row, _, _| row[col::ROW_TYPE] - BabyBear::ONE),
            },
            Constraint {
                name: "total_removals_match".into(),
                eval: Box::new(|row, _, pi| row[col::REMOVAL_COUNT] - pi[2]),
            },
            Constraint {
                name: "total_checks_match".into(),
                eval: Box::new(|row, _, pi| row[col::CHECK_COUNT] - pi[3]),
            },
        ]
    }

    fn generate_trace(&self) -> (Vec<Vec<BabyBear>>, Vec<BabyBear>) {
        let w = &self.witness;
        let mut trace = Vec::new();
        let root_transition_hash = if !w.removed_facts.is_empty() {
            verify_root_transition(w).unwrap_or(BabyBear::ZERO)
        } else {
            compute_root_transition_hash(w.old_root, w.new_root, &[], &w.added_checks_commitment)
        };

        for (i, fact) in w.removed_facts.iter().enumerate() {
            let mut row = vec![BabyBear::ZERO; FOLD_AIR_WIDTH];
            row[col::ROW_TYPE] = BabyBear::ZERO;
            row[col::FACT_HASH] = fact.hash();
            row[col::MEMBERSHIP_ROOT] =
                fact.verify_membership(w.old_root).unwrap_or(BabyBear::ZERO);
            row[col::OLD_ROOT] = w.old_root;
            row[col::NEW_ROOT] = w.new_root;
            row[col::REMOVAL_COUNT] = BabyBear::new((i + 1) as u32);
            row[col::CHECK_COUNT] = BabyBear::new(w.num_added_checks as u32);
            row[col::FACT_PRED] = fact.predicate;
            row[col::FACT_TERM_START] = fact.terms[0];
            row[col::FACT_TERM_START + 1] = fact.terms[1];
            row[col::FACT_TERM_START + 2] = fact.terms[2];
            row[col::HASH_VALID] = BabyBear::ONE;
            trace.push(row);
        }

        let mut summary = vec![BabyBear::ZERO; FOLD_AIR_WIDTH];
        summary[col::ROW_TYPE] = BabyBear::ONE;
        summary[col::MEMBERSHIP_ROOT] = root_transition_hash;
        summary[col::OLD_ROOT] = w.old_root;
        summary[col::NEW_ROOT] = w.new_root;
        summary[col::REMOVAL_COUNT] = BabyBear::new(w.removed_facts.len() as u32);
        summary[col::CHECK_COUNT] = BabyBear::new(w.num_added_checks as u32);
        summary[col::HASH_VALID] = BabyBear::ONE;
        trace.push(summary);

        let fact_hashes: Vec<BabyBear> = w.removed_facts.iter().map(|f| f.hash()).collect();
        let expected_rt = compute_root_transition_hash(
            w.old_root,
            w.new_root,
            &fact_hashes,
            &w.added_checks_commitment,
        );
        // pi[5] is the narrow (single-element) representation of the checks commitment,
        // used only for the zero-check constraint. The full 124-bit binding is through
        // pi[4] (root transition hash) which includes all 4 WideHash elements.
        let narrow_checks = w.added_checks_commitment.to_narrow();
        let public_inputs = vec![
            w.old_root,
            w.new_root,
            BabyBear::new(w.removed_facts.len() as u32),
            BabyBear::new(w.num_added_checks as u32),
            expected_rt,
            narrow_checks,
        ];
        (trace, public_inputs)
    }
}

// ============================================================================
// FoldStarkAir: Real STARK proof generation/verification for fold steps.
// ============================================================================

/// StarkAir implementation for the Fold step.
///
/// This enables generating real STARK proofs (polynomial commitment + FRI)
/// for fold operations, replacing the ConstraintProof (BLAKE3 trace digest)
/// that provides no cryptographic soundness.
///
/// The fold AIR uses per-row constraints (binary checks, hash correctness,
/// public input binding) and one transition constraint (removal_count_increment).
/// For the small trace sizes typical of fold steps (2-8 rows), the custom STARK
/// handles this correctly.
pub struct FoldStarkAir {
    pub witness: FoldWitness,
}

impl FoldStarkAir {
    pub fn new(witness: FoldWitness) -> Self {
        Self { witness }
    }
}

/// Generate a real STARK proof for a fold step.
///
/// This produces a cryptographically sound proof (polynomial commitment + FRI)
/// that the fold operation was performed correctly. The verifier can check this
/// without seeing the witness.
///
/// Returns `None` if the witness fails constraint checking or the trace is too
/// small for the STARK prover (requires >= 2 rows, power-of-two padded).
///
/// DEPRECATED: Use `crate::dsl::fold::prove_fold_dsl` instead.
/// This function is retained for backward compatibility with code that has not
/// yet migrated to the DSL-native API.
#[deprecated(note = "Use crate::dsl::fold::prove_fold_dsl instead")]
pub fn prove_fold_stark(witness: &FoldWitness) -> Option<StarkProof> {
    let air = FoldStarkAir::new(witness.clone());
    let fold_air = FoldAir::new(witness.clone());

    // Generate trace and public inputs
    let (trace, public_inputs) = fold_air.generate_trace();

    // Pad trace to power-of-two (>= 2 rows required by STARK prover)
    let padded_len = trace.len().next_power_of_two().max(2);
    let mut padded_trace = trace;
    while padded_trace.len() < padded_len {
        padded_trace.push(padded_trace.last().unwrap().clone());
    }

    Some(stark::prove(&air, &padded_trace, &public_inputs))
}

/// Verify a STARK proof for a fold step.
///
/// Returns `Ok(())` if the proof is valid, or an error message describing the failure.
///
/// # Soundness: new_root verification
///
/// The STARK proves that `pi[4] = Poseidon2(old_root || new_root || removed_fact_hashes)`,
/// which binds `new_root` to the proof via collision resistance. However, the AIR does NOT
/// independently recompute `new_root` from "old tree minus removed leaves" — doing so would
/// require encoding the full Merkle rebuild inside the STARK (expensive and impractical for
/// our tree sizes).
///
/// Instead, the protocol relies on the **bridge layer** to independently compute `new_root`:
/// `build_fold_witnesses()` rebuilds the Poseidon2 Merkle tree over the new state's facts
/// and provides the resulting root as `FoldWitness.new_root`. The prover cannot forge this
/// because:
/// 1. The verifier (bridge or validated IVC) independently constructs the Merkle tree from
///    the actual facts after removal.
/// 2. The STARK proof then binds `new_root` into the transition hash via `pi[4]`.
/// 3. A forged `new_root` would require a Poseidon2 collision to produce a valid `pi[4]`.
///
/// For remote verification without the bridge layer, use `prove_validated_ivc()` which
/// provides per-step Merkle membership STARKs proving each removal was valid against
/// the claimed `old_root`. The `new_root` claim is then verified by the hash-chain STARK's
/// continuity: `step[i].new_root == step[i+1].old_root`, with membership proofs proving
/// each `old_root` is honest.
///
/// DEPRECATED: Use `crate::dsl::fold::verify_fold_dsl` instead.
/// This function is retained for backward compatibility with code that has not
/// yet migrated to the DSL-native API.
#[deprecated(note = "Use crate::dsl::fold::verify_fold_dsl instead")]
pub fn verify_fold_stark(proof: &StarkProof, public_inputs: &[BabyBear]) -> Result<(), String> {
    let dummy_witness = FoldWitness {
        old_root: BabyBear::ZERO,
        new_root: BabyBear::ZERO,
        removed_facts: vec![],
        num_added_checks: 0,
        added_checks_commitment: crate::binding::WideHash::ZERO,
    };
    let air = FoldStarkAir::new(dummy_witness);
    stark::verify(&air, proof, public_inputs)
}

pub fn build_shared_tree(leaves: &[BabyBear], depth: usize) -> (BabyBear, Vec<MerkleWitness>) {
    use crate::poseidon2::hash_4_to_1;
    let fan_out = 4usize;
    let max_leaves = fan_out.pow(depth as u32);
    let mut levels: Vec<Vec<BabyBear>> = Vec::with_capacity(depth + 1);
    let mut bottom = Vec::with_capacity(max_leaves);
    for &leaf in leaves.iter().take(max_leaves) {
        bottom.push(leaf);
    }
    while bottom.len() < max_leaves {
        bottom.push(BabyBear::ZERO);
    }
    levels.push(bottom);
    for _ in 0..depth {
        let prev = levels.last().unwrap();
        let mut next = Vec::with_capacity(prev.len() / fan_out);
        for chunk in prev.chunks(fan_out) {
            next.push(hash_4_to_1(&[chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        levels.push(next);
    }
    let root = levels[depth][0];
    let mut proofs = Vec::with_capacity(leaves.len());
    for (leaf_idx, &leaf_hash) in leaves.iter().enumerate() {
        let mut proof_levels = Vec::with_capacity(depth);
        let mut idx = leaf_idx;
        for level in 0..depth {
            let position = (idx % fan_out) as u8;
            let group_start = idx - (idx % fan_out);
            let mut siblings = Vec::with_capacity(3);
            for j in 0..fan_out {
                if j as u8 != position {
                    siblings.push(levels[level][group_start + j]);
                }
            }
            proof_levels.push(MerkleLevelWitness {
                position,
                siblings: [siblings[0], siblings[1], siblings[2]],
            });
            idx /= fan_out;
        }
        proofs.push(MerkleWitness {
            leaf_hash,
            levels: proof_levels,
            expected_root: root,
        });
    }
    (root, proofs)
}

pub fn build_membership_proof(leaf_hash: BabyBear, depth: usize) -> MerkleWitness {
    let mut current = leaf_hash;
    let mut levels = Vec::with_capacity(depth);
    for i in 0..depth {
        let position = (i % 4) as u8;
        let siblings = [
            BabyBear::new((leaf_hash.0.wrapping_add(i as u32 * 7 + 1)) % crate::field::BABYBEAR_P),
            BabyBear::new((leaf_hash.0.wrapping_add(i as u32 * 7 + 2)) % crate::field::BABYBEAR_P),
            BabyBear::new((leaf_hash.0.wrapping_add(i as u32 * 7 + 3)) % crate::field::BABYBEAR_P),
        ];
        let parent = MerkleAir::compute_parent(current, position, &siblings);
        levels.push(MerkleLevelWitness { position, siblings });
        current = parent;
    }
    MerkleWitness {
        leaf_hash,
        levels,
        expected_root: current,
    }
}

/// Compute the checks commitment for a set of test checks.
///
/// Each test check is modeled as `hash_fact(predicate=CHECK_BASE+i, terms=[i, 0, 0])`.
/// Returns `WideHash::ZERO` when `num_checks == 0` (backwards-compatible identity).
pub fn compute_test_checks_commitment(num_checks: usize) -> crate::binding::WideHash {
    if num_checks == 0 {
        return crate::binding::WideHash::ZERO;
    }
    let check_hashes: Vec<BabyBear> = (0..num_checks)
        .map(|i| {
            hash_fact(
                BabyBear::new(900 + i as u32), // CHECK_BASE + i
                &[BabyBear::new(i as u32), BabyBear::ZERO, BabyBear::ZERO],
            )
        })
        .collect();
    crate::binding::WideHash::from_poseidon2("pyana-checks-v1", &check_hashes)
}

pub fn create_test_fold(num_removals: usize, num_checks: usize) -> FoldWitness {
    let new_root = BabyBear::new(222222);
    let checks_commitment = compute_test_checks_commitment(num_checks);
    if num_removals == 0 {
        return FoldWitness {
            old_root: BabyBear::new(111111),
            new_root,
            removed_facts: vec![],
            num_added_checks: num_checks,
            added_checks_commitment: checks_commitment,
        };
    }
    let facts_data: Vec<(BabyBear, [BabyBear; 3])> = (0..num_removals)
        .map(|i| {
            (
                BabyBear::new((i * 100 + 10) as u32),
                [
                    BabyBear::new((i * 100 + 20) as u32),
                    BabyBear::new((i * 100 + 30) as u32),
                    BabyBear::ZERO,
                ],
            )
        })
        .collect();
    let fact_hashes: Vec<BabyBear> = facts_data
        .iter()
        .map(|(pred, terms)| hash_fact(*pred, terms))
        .collect();
    let (old_root, proofs) = build_shared_tree(&fact_hashes, 4);
    let removed_facts: Vec<RemovedFact> = facts_data
        .into_iter()
        .zip(proofs.into_iter())
        .map(|((predicate, terms), proof)| RemovedFact {
            predicate,
            terms,
            membership_proof: Some(proof),
        })
        .collect();
    FoldWitness {
        old_root,
        new_root,
        removed_facts,
        num_added_checks: num_checks,
        added_checks_commitment: checks_commitment,
    }
}
