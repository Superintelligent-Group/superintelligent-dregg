//! Fold step AIR - with verified membership and root transition binding.
//!
//! Types, witnesses, and helpers live in [`crate::fold_types`]. This module
//! contains only the `StarkAir` implementation for `FoldStarkAir`.
//!
//! # DSL-native implementation
//!
//! The production prove/verify functions are now available via
//! `pyana_dsl_runtime::fold::{prove_fold_dsl, verify_fold_dsl}`.
//! The functions in this module (`prove_fold_stark`, `verify_fold_stark`) are
//! deprecated but retained for backward compatibility.

// Re-export everything from fold_types for backward compatibility.
pub use crate::fold_types::*;

use crate::field::BabyBear;
use crate::poseidon2::hash_fact;
use crate::stark::{BoundaryConstraint, StarkAir};

impl StarkAir for FoldStarkAir {
    fn width(&self) -> usize {
        FOLD_AIR_WIDTH
    }

    fn constraint_degree(&self) -> usize {
        // The highest-degree constraint is removal_count_increment which multiplies
        // three terms (is_removal * is_next_removal * diff) = degree 3.
        // The delta_nonempty constraint uses conditional branching, not polynomial degree.
        3
    }

    fn has_chain_continuity(&self) -> bool {
        false
    }

    fn air_name(&self) -> &'static str {
        "pyana-fold-v1"
    }

    fn eval_constraints(
        &self,
        local: &[BabyBear],
        next: &[BabyBear],
        public_inputs: &[BabyBear],
        alpha: BabyBear,
    ) -> BabyBear {
        let mut result = BabyBear::ZERO;
        let mut alpha_power = BabyBear::ONE;

        // C1: row_type_binary: rt * (rt - 1) = 0
        let rt = local[col::ROW_TYPE];
        result = result + alpha_power * (rt * (rt - BabyBear::ONE));
        alpha_power = alpha_power * alpha;

        // C2: membership_root_matches_old_root
        let is_removal = BabyBear::ONE - local[col::ROW_TYPE];
        result = result
            + alpha_power * (is_removal * (local[col::MEMBERSHIP_ROOT] - local[col::OLD_ROOT]));
        alpha_power = alpha_power * alpha;

        // C3: hash_valid_binary
        let hv = local[col::HASH_VALID];
        result = result + alpha_power * (hv * (hv - BabyBear::ONE));
        alpha_power = alpha_power * alpha;

        // C4: removal_hash_required
        result = result + alpha_power * (is_removal * (BabyBear::ONE - local[col::HASH_VALID]));
        alpha_power = alpha_power * alpha;

        // C5: fact_hash_correct
        let expected_hash = hash_fact(
            local[col::FACT_PRED],
            &[
                local[col::FACT_TERM_START],
                local[col::FACT_TERM_START + 1],
                local[col::FACT_TERM_START + 2],
            ],
        );
        result = result + alpha_power * (is_removal * (local[col::FACT_HASH] - expected_hash));
        alpha_power = alpha_power * alpha;

        // C6: old_root_consistent (binds to public input)
        result = result + alpha_power * (local[col::OLD_ROOT] - public_inputs[0]);
        alpha_power = alpha_power * alpha;

        // C7: new_root_consistent (binds to public input)
        result = result + alpha_power * (local[col::NEW_ROOT] - public_inputs[1]);
        alpha_power = alpha_power * alpha;

        // C8: removal_count_increment (transition constraint)
        // Only enforced when both current and next are removal rows.
        let is_next_removal = BabyBear::ONE - next[col::ROW_TYPE];
        result = result
            + alpha_power
                * (is_removal
                    * is_next_removal
                    * (next[col::REMOVAL_COUNT] - local[col::REMOVAL_COUNT] - BabyBear::ONE));
        alpha_power = alpha_power * alpha;

        // C9: root_transition_binding (on summary row)
        let is_summary = local[col::ROW_TYPE];
        result =
            result + alpha_power * (is_summary * (local[col::MEMBERSHIP_ROOT] - public_inputs[4]));
        alpha_power = alpha_power * alpha;

        // C10: checks_commitment_zero_when_no_checks
        // When check_count (pi[3]) is zero, checks_commitment (pi[5]) must be zero.
        if public_inputs[3] == BabyBear::ZERO {
            result = result + alpha_power * (is_summary * public_inputs[5]);
        }

        result
    }

    fn boundary_constraints(
        &self,
        public_inputs: &[BabyBear],
        trace_len: usize,
    ) -> Vec<BoundaryConstraint> {
        let mut constraints = vec![];
        if public_inputs.len() >= 6 {
            // First row: old_root must match pi[0]
            constraints.push(BoundaryConstraint {
                row: 0,
                col: col::OLD_ROOT,
                value: public_inputs[0],
            });
            // First row: new_root must match pi[1]
            constraints.push(BoundaryConstraint {
                row: 0,
                col: col::NEW_ROOT,
                value: public_inputs[1],
            });
            // Last row: must be summary (ROW_TYPE = 1)
            constraints.push(BoundaryConstraint {
                row: trace_len - 1,
                col: col::ROW_TYPE,
                value: BabyBear::ONE,
            });
            // Last row: removal_count = pi[2]
            constraints.push(BoundaryConstraint {
                row: trace_len - 1,
                col: col::REMOVAL_COUNT,
                value: public_inputs[2],
            });
            // Last row: check_count = pi[3]
            constraints.push(BoundaryConstraint {
                row: trace_len - 1,
                col: col::CHECK_COUNT,
                value: public_inputs[3],
            });
        }
        constraints
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint_prover::ConstraintProver;

    #[test]
    fn fold_air_valid_single_removal() {
        let witness = create_test_fold(1, 0);
        let air = FoldAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(
            result.is_valid(),
            "Fold AIR should verify: {:?}",
            result.violations()
        );
    }

    #[test]
    fn fold_air_valid_multiple_removals() {
        let witness = create_test_fold(3, 2);
        let air = FoldAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(
            result.is_valid(),
            "Fold AIR should verify: {:?}",
            result.violations()
        );
    }

    #[test]
    fn fold_air_valid_checks_only() {
        let witness = FoldWitness {
            old_root: BabyBear::new(100),
            new_root: BabyBear::new(200),
            removed_facts: vec![],
            num_added_checks: 3,
            added_checks_commitment: compute_test_checks_commitment(3),
        };
        let air = FoldAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(
            result.is_valid(),
            "Fold AIR (checks only) should verify: {:?}",
            result.violations()
        );
    }

    #[test]
    fn fold_air_empty_delta_fails() {
        let witness = FoldWitness {
            old_root: BabyBear::new(100),
            new_root: BabyBear::new(200),
            removed_facts: vec![],
            num_added_checks: 0,
            added_checks_commitment: crate::binding::WideHash::ZERO,
        };
        let air = FoldAir::new(witness);
        assert!(!ConstraintProver::verify(&air).is_valid());
    }

    #[test]
    fn fold_air_no_membership_proof_fails() {
        let witness = FoldWitness {
            old_root: BabyBear::new(111111),
            new_root: BabyBear::new(222222),
            removed_facts: vec![RemovedFact {
                predicate: BabyBear::new(10),
                terms: [BabyBear::new(20), BabyBear::new(30), BabyBear::ZERO],
                membership_proof: None,
            }],
            num_added_checks: 0,
            added_checks_commitment: crate::binding::WideHash::ZERO,
        };
        assert!(
            !ConstraintProver::verify(&FoldAir::new(witness)).is_valid(),
            "Missing membership proof should fail"
        );
    }

    #[test]
    fn fold_air_wrong_membership_root_fails() {
        let predicate = BabyBear::new(10);
        let terms = [BabyBear::new(20), BabyBear::new(30), BabyBear::ZERO];
        let fact_hash = hash_fact(predicate, &terms);
        let proof = build_membership_proof(fact_hash, 4);
        let witness = FoldWitness {
            old_root: BabyBear::new(999999),
            new_root: BabyBear::new(222222),
            removed_facts: vec![RemovedFact {
                predicate,
                terms,
                membership_proof: Some(proof),
            }],
            num_added_checks: 0,
            added_checks_commitment: crate::binding::WideHash::ZERO,
        };
        assert!(
            !ConstraintProver::verify(&FoldAir::new(witness)).is_valid(),
            "Wrong root should fail"
        );
    }

    #[test]
    fn fold_air_forged_membership_proof_fails() {
        let predicate = BabyBear::new(10);
        let terms = [BabyBear::new(20), BabyBear::new(30), BabyBear::ZERO];
        let wrong_leaf = BabyBear::new(99999);
        let proof = build_membership_proof(wrong_leaf, 4);
        let witness = FoldWitness {
            old_root: proof.expected_root,
            new_root: BabyBear::new(222222),
            removed_facts: vec![RemovedFact {
                predicate,
                terms,
                membership_proof: Some(proof),
            }],
            num_added_checks: 0,
            added_checks_commitment: crate::binding::WideHash::ZERO,
        };
        assert!(
            !ConstraintProver::verify(&FoldAir::new(witness)).is_valid(),
            "Forged proof should fail"
        );
    }

    // ========================================================================
    // FoldStarkAir STARK proof generation/verification tests
    // ========================================================================

    #[test]
    fn fold_stark_proof_single_removal() {
        let witness = create_test_fold(1, 0);
        let proof = prove_fold_stark(&witness).expect("fold STARK proof should generate");

        // Verify against the correct public inputs
        let fold_air = FoldAir::new(witness.clone());
        let (_, public_inputs) = fold_air.generate_trace();
        assert!(
            verify_fold_stark(&proof, &public_inputs).is_ok(),
            "fold STARK proof should verify"
        );
    }

    #[test]
    fn fold_stark_proof_multiple_removals() {
        let witness = create_test_fold(3, 2);
        let proof = prove_fold_stark(&witness).expect("fold STARK proof should generate");

        let fold_air = FoldAir::new(witness.clone());
        let (_, public_inputs) = fold_air.generate_trace();
        assert!(
            verify_fold_stark(&proof, &public_inputs).is_ok(),
            "fold STARK proof should verify"
        );
    }

    #[test]
    fn fold_stark_proof_checks_only() {
        let witness = FoldWitness {
            old_root: BabyBear::new(100),
            new_root: BabyBear::new(200),
            removed_facts: vec![],
            num_added_checks: 3,
            added_checks_commitment: compute_test_checks_commitment(3),
        };
        let proof = prove_fold_stark(&witness).expect("fold STARK proof should generate");

        let fold_air = FoldAir::new(witness.clone());
        let (_, public_inputs) = fold_air.generate_trace();
        assert!(
            verify_fold_stark(&proof, &public_inputs).is_ok(),
            "fold STARK proof (checks only) should verify"
        );
    }

    #[test]
    fn fold_stark_proof_wrong_public_inputs_fails() {
        let witness = create_test_fold(1, 0);
        let proof = prove_fold_stark(&witness).expect("fold STARK proof should generate");

        // Tamper with public inputs: wrong old_root
        let fold_air = FoldAir::new(witness.clone());
        let (_, mut public_inputs) = fold_air.generate_trace();
        public_inputs[0] = BabyBear::new(99999); // wrong old_root
        assert!(
            verify_fold_stark(&proof, &public_inputs).is_err(),
            "fold STARK proof with wrong public inputs should fail"
        );
    }

    #[test]
    fn fold_stark_proof_tampered_commitment_fails() {
        let witness = create_test_fold(2, 1);
        let mut proof = prove_fold_stark(&witness).expect("fold STARK proof should generate");

        // Tamper with the trace commitment
        proof.trace_commitment[0] ^= 0xFF;

        let fold_air = FoldAir::new(witness.clone());
        let (_, public_inputs) = fold_air.generate_trace();
        assert!(
            verify_fold_stark(&proof, &public_inputs).is_err(),
            "tampered fold STARK proof should fail"
        );
    }

    // ========================================================================
    // Added checks commitment binding tests
    // ========================================================================

    #[test]
    fn fold_air_checks_commitment_binds_content() {
        let real_commitment_for_2 = compute_test_checks_commitment(2);
        let witness = FoldWitness {
            old_root: BabyBear::new(100),
            new_root: BabyBear::new(200),
            removed_facts: vec![],
            num_added_checks: 3,                            // claims 3...
            added_checks_commitment: real_commitment_for_2, // ...but commitment is for 2
        };
        let air = FoldAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(
            result.is_valid(),
            "AIR should pass (it trusts its own public inputs): {:?}",
            result.violations()
        );
    }

    #[test]
    fn fold_stark_wrong_checks_commitment_fails() {
        let witness = create_test_fold(0, 3);
        let proof = prove_fold_stark(&witness).expect("fold STARK proof should generate");

        let fold_air = FoldAir::new(witness.clone());
        let (_, mut public_inputs) = fold_air.generate_trace();
        let wrong_commitment = compute_test_checks_commitment(2);
        public_inputs[5] = wrong_commitment.to_narrow();
        public_inputs[4] = compute_root_transition_hash(
            public_inputs[0],
            public_inputs[1],
            &[], // no removals
            &wrong_commitment,
        );
        assert!(
            verify_fold_stark(&proof, &public_inputs).is_err(),
            "STARK proof with wrong checks commitment should fail verification"
        );
    }

    #[test]
    fn fold_stark_forged_count_with_zero_commitment_fails() {
        let witness_no_checks = FoldWitness {
            old_root: BabyBear::new(100),
            new_root: BabyBear::new(200),
            removed_facts: vec![],
            num_added_checks: 1,
            added_checks_commitment: compute_test_checks_commitment(1),
        };
        let proof = prove_fold_stark(&witness_no_checks).expect("should generate");

        let fold_air = FoldAir::new(witness_no_checks.clone());
        let (_, mut public_inputs) = fold_air.generate_trace();
        let forged_commitment =
            crate::binding::WideHash::from_poseidon2("forged", &[BabyBear::new(9999)]);
        public_inputs[5] = forged_commitment.to_narrow();
        public_inputs[4] = compute_root_transition_hash(
            public_inputs[0],
            public_inputs[1],
            &[],
            &forged_commitment,
        );
        assert!(
            verify_fold_stark(&proof, &public_inputs).is_err(),
            "STARK proof with forged checks commitment should fail"
        );
    }

    #[test]
    fn fold_air_nonzero_commitment_with_zero_count_fails() {
        let witness = FoldWitness {
            old_root: BabyBear::new(100),
            new_root: BabyBear::new(200),
            removed_facts: vec![],
            num_added_checks: 0,
            added_checks_commitment: crate::binding::WideHash::from_poseidon2(
                "test",
                &[BabyBear::new(42)],
            ), // non-zero!
        };
        let air = FoldAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(
            !result.is_valid(),
            "Non-zero commitment with zero check count should fail"
        );
    }

    #[test]
    fn fold_air_zero_commitment_with_nonzero_count_is_invalid_binding() {
        let witness = FoldWitness {
            old_root: BabyBear::new(100),
            new_root: BabyBear::new(200),
            removed_facts: vec![],
            num_added_checks: 2,
            added_checks_commitment: crate::binding::WideHash::ZERO,
        };
        let air = FoldAir::new(witness.clone());
        let result = ConstraintProver::verify(&air);
        assert!(
            result.is_valid(),
            "AIR is self-consistent even with zero commitment"
        );

        let proof = prove_fold_stark(&witness).expect("should generate");
        let fold_air = FoldAir::new(witness.clone());
        let (_, mut public_inputs) = fold_air.generate_trace();
        let real_commitment = compute_test_checks_commitment(2);
        public_inputs[5] = real_commitment.to_narrow();
        public_inputs[4] =
            compute_root_transition_hash(public_inputs[0], public_inputs[1], &[], &real_commitment);
        assert!(
            verify_fold_stark(&proof, &public_inputs).is_err(),
            "STARK should fail when verifier supplies correct commitment that differs from proof"
        );
    }
}
