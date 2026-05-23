//! Backward-compatible re-exports for arithmetic predicate AIR types.
//!
//! The production implementation lives in [`crate::dsl::predicates::arithmetic`].

pub use crate::dsl::predicates::{
    ArithExpr, ArithPredicate, ArithmeticPredicateProof, CompareOp,
    compute_arithmetic_fact_commitment, prove_arithmetic_dsl, verify_arithmetic_dsl,
    verify_arithmetic_predicate,
};

/// Backward-compatible witness struct for arithmetic predicates.
pub struct ArithmeticPredicateWitness {
    pub inputs: Vec<u32>,
    pub predicate: ArithPredicate,
    pub fact_commitment: crate::field::BabyBear,
}

/// Backward-compatible prove function.
///
/// Wraps `prove_arithmetic_dsl` with the old API shape.
pub fn prove_arithmetic_predicate(
    witness: ArithmeticPredicateWitness,
) -> Option<ArithmeticPredicateProof> {
    prove_arithmetic_dsl(&witness.inputs, &witness.predicate, witness.fact_commitment).ok()
}
