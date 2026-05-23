//! Garbled circuit types, column layout, and AIR struct.
//!
//! This module re-exports externally-used types from [`super::garbled_air`]
//! so that consumers can import from a dedicated types module separate from
//! the AIR constraint implementation.

pub use crate::garbled_air::{GARBLED_EVAL_AIR_WIDTH, GarbledEvaluationAir, col};
