//! # pyana-prediction-market
//!
//! A prediction-market app that demonstrates several "new world" platform
//! features in combination:
//!
//! 1. **Blinded queue of outcome commitments** — bettors submit
//!    `Com(market_id, outcome_id, stake, secret)` so that nobody (including
//!    the operator) can read market sentiment until resolution. On oracle
//!    resolution, bettors reveal by consuming with a nullifier; the resolver
//!    pays winners from the escrowed pool.
//! 2. **Positional oracle feed** — a Merkle-rooted positional sequence with
//!    O(log n) inclusion proofs (stand-in for KZG when the `kzg` feature is
//!    unavailable — see `oracle.rs`). The oracle signs report tuples
//!    `(market_id, position, outcome_id, timestamp)` with Ed25519, and the
//!    server only accepts reports from a pre-configured pubkey.
//! 3. **Ring-trade participant** — `RingMarketParticipant` implements
//!    [`pyana_app_framework::RingTradeParticipant`] so bettors with offsetting
//!    positions on different outcomes can settle atomically.
//!
//! See `tests.rs` for adversarial tests of each upgrade claim.

pub mod bets;
pub mod market;
pub mod oracle;
pub mod ring;
pub mod server;
pub mod settlement;

#[cfg(test)]
pub mod tests;
