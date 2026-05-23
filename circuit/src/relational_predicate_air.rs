//! Backward-compatible re-exports for relational predicate AIR types.
//!
//! The production implementation lives in [`crate::dsl::predicates::relational`].

pub use crate::dsl::predicates::{
    RelationalOp as RelationType, RelationalPredicateProof, RelationalPredicateWitness,
    RelationalProof, RelationalWitness, compute_value_commitment, prove_relational,
    prove_relational_dsl, verify_relational, verify_relational_dsl,
};

use crate::field::BabyBear;

/// Backward-compatible function: prove a value comparison.
///
/// Proves that `my_value <relation> their_value` using blinding factors.
pub fn prove_value_comparison(
    my_value: BabyBear,
    my_blinding: BabyBear,
    their_value: BabyBear,
    their_blinding: BabyBear,
    relation: RelationType,
) -> Option<RelationalPredicateProof> {
    let witness = RelationalWitness {
        value_a: my_value.0,
        blinding_a: my_blinding.0,
        value_b: their_value.0,
        blinding_b: their_blinding.0,
        op: relation,
        verify_commitments: true,
    };
    prove_relational(&witness).ok()
}
