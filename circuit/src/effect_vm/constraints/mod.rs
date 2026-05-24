//! Per-variant constraint blocks for the Effect VM AIR.
//!
//! Each submodule exposes free functions that accumulate constraints for one
//! or more effect selector blocks into the `(combined, alpha_pow)` running
//! product. The main `eval_constraints` in `air.rs` calls these in order.

pub mod captp;
pub mod custom;
pub mod factory;
pub mod obligation;
pub mod queues;
pub mod sealing;
pub mod state;
pub mod value;
