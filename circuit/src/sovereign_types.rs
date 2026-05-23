//! Sovereign transition types and helpers.
//!
//! This module re-exports externally-used types from [`super::sovereign_transition_air`]
//! so that consumers can import from a dedicated types module separate from
//! the AIR constraint implementation.

pub use crate::sovereign_transition_air::{
    DELTA_PI_LEN, DELTA_PI_OFFSET, SOVEREIGN_PUBLIC_INPUTS, SOVEREIGN_TRANSITION_WIDTH,
    SovereignTransitionAir, bytes32_to_babybear, compute_cell_id_hash,
    compute_transfer_effects_hash, encode_balance_delta, extract_balance_delta,
    generate_sovereign_transition_trace,
};
