//! Note spending types, witnesses, and helpers.
//!
//! This module re-exports externally-used types from [`super::note_spending_air`]
//! so that consumers can import from a dedicated types module separate from
//! the AIR constraint implementation.

pub use crate::note_spending_air::{
    MIN_MERKLE_DEPTH, NOTE_SPENDING_WIDTH, NoteSpendingAir, NoteSpendingWitness,
    SPENDING_KEY_LIMBS, col, create_test_witness, key_to_field_elements, merkle_col, pi,
    prove_note_spend, test_spending_key, verify_note_spend,
};
