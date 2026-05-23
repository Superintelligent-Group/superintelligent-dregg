//! Multi-step derivation types and helpers extracted from multi_step_air.
//!
//! The `StarkAir` and `Air` implementations remain in [`super::multi_step_air`].
//!
//! This module re-exports externally-used types from `multi_step_air` so that
//! consumers can import from a dedicated types module.

pub use crate::multi_step_air::{
    ALLOW_PREDICATE, MAX_STEPS, MULTI_STEP_AIR_WIDTH, MultiStepDerivationAir, MultiStepStarkAir,
    MultiStepWitness, build_multi_step_witness, col, generate_multi_step_trace, pi,
    prove_authorization, prove_authorization_stark, verify_authorization_stark,
};
