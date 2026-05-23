//! Revocation types, witnesses, and helpers.
//!
//! This module re-exports externally-used types from [`super::non_revocation_air`]
//! and [`super::accumulator_air`] so that consumers can import from a dedicated
//! types module separate from the AIR constraint implementations.

pub use crate::non_revocation_air::{
    HALF_P_MINUS_1, MAX_ANCESTORS, NON_REVOCATION_WIDTH, NonMembershipWitness, NonRevocationAir,
    NonRevocationWitness, ORDERING_DIFF_BITS, REVOCATION_TREE_DEPTH, SENTINEL_MAX, SENTINEL_MIN,
    SortedRevocationTree, col, pi, prove_non_revocation, revocation_hash_to_field,
    verify_non_revocation,
};

pub use crate::accumulator_air::{
    ACCUMULATOR_WIDTH, AccumulatorNonMembershipWitness, AccumulatorNonRevocationAir,
    AccumulatorNonRevocationWitness, ExtElem, col as accumulator_col, compute_accumulator,
    derive_alpha, pi as accumulator_pi, prove_accumulator_non_revocation,
    verify_accumulator_non_revocation,
};
