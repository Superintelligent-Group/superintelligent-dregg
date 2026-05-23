//! # pyana-dao-treasury
//!
//! A programmable-queue-backed DAO treasury that:
//!
//! - Holds multi-asset balances ([`treasury::Treasury`]).
//! - Accepts spending proposals through a [`ProgrammableQueue`] gated by a
//!   custom quorum-check program (see [`governance::QuorumGate`]).
//! - Delegates execution to a single [`executor::BatchExecutor`] that batches
//!   approved proposals into one settlement turn for amortized proving.
//!
//! The novel piece is the *governance-gated queue*: the queue's
//! [`QueueConstraint::Custom`] variant is opaque to the storage layer, so the
//! gating is enforced at the application layer in [`governance::QuorumGate`]
//! and adversarially tested. See `REVIEW[P1]` markers for the underlying
//! framework gap.

pub mod executor;
pub mod governance;
pub mod proposal;
pub mod server;
pub mod treasury;

#[cfg(test)]
mod tests;

pub use executor::{BatchSummary, TreasuryBatchExecutor, TreasuryBatchExecutorError};
pub use governance::{GovernanceState, GovernanceError, QuorumGate, Voter};
pub use proposal::{Proposal, ProposalId, ProposalStatus, SpendOrder};
pub use treasury::{AssetId, Treasury, TreasuryError};
