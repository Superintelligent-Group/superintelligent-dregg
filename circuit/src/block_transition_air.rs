//! Block transition AIR constants.
//!
//! This module provides column layout constants used by the block-transition DSL circuit.

/// Total trace width for the block transition AIR.
pub const BLOCK_TRANSITION_WIDTH: usize = 6;

/// Column indices.
pub mod col {
    pub const OLD_ROOT: usize = 0;
    pub const NEW_LEAF: usize = 1;
    pub const POSITION: usize = 2;
    pub const NEW_ROOT: usize = 3;
    pub const SIBLING_HASH: usize = 4;
    pub const EVENT_INDEX: usize = 5;
}
