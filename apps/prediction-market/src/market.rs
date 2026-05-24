//! Market state and lifecycle.
//!
//! A `Market` is a binary-or-multi-outcome prediction event. It is created in
//! the `Open` state. Bettors place commitments while it is `Open`. The market
//! advances to `Resolving` when the oracle posts a result, then `Resolved`
//! once all winning bets have been settled.

use serde::{Deserialize, Serialize};

/// Identifier for a market (content-addressed).
pub type MarketId = [u8; 32];

/// Identifier for a single outcome within a market.
///
/// Outcome IDs are domain-separated hashes of `(market_id || label)` so the
/// same string label across different markets does not collide.
pub type OutcomeId = [u8; 32];

/// The lifecycle states of a market.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus {
    /// Accepting bet commitments.
    Open,
    /// Oracle has reported; bettors must consume (reveal) before
    /// `claim_deadline`.
    Resolving {
        winning_outcome: OutcomeId,
        claim_deadline: u64,
    },
    /// All winning bets paid, or deadline passed.
    Resolved {
        winning_outcome: OutcomeId,
        total_pool: u64,
        total_winning_stake: u64,
    },
}

/// A prediction market.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Market {
    pub id: MarketId,
    /// Human-readable question (e.g., "Will it rain Friday?").
    pub question: String,
    /// All allowed outcomes, by content-addressed id.
    pub outcomes: Vec<OutcomeId>,
    /// Human-readable outcome labels in the same order as `outcomes`.
    pub outcome_labels: Vec<String>,
    pub status: MarketStatus,
    /// Total escrowed stake across all bets.
    pub total_pool: u64,
    /// Block height at which the oracle is expected to report; markets close
    /// to new bets at this height.
    pub close_height: u64,
}

impl Market {
    /// Construct a new market with a deterministic id derived from the
    /// question + outcomes + close_height.
    pub fn new(
        question: impl Into<String>,
        outcome_labels: Vec<String>,
        close_height: u64,
    ) -> Self {
        let question = question.into();
        // Pre-compute market id (so outcome ids can be derived from it).
        let mut hasher = blake3::Hasher::new_derive_key("pyana-prediction-market-id-v1");
        hasher.update(question.as_bytes());
        for label in &outcome_labels {
            hasher.update(label.as_bytes());
            hasher.update(&[0u8]);
        }
        hasher.update(&close_height.to_le_bytes());
        let id: [u8; 32] = *hasher.finalize().as_bytes();

        let outcomes = outcome_labels
            .iter()
            .map(|label| derive_outcome_id(&id, label))
            .collect();

        Self {
            id,
            question,
            outcomes,
            outcome_labels,
            status: MarketStatus::Open,
            total_pool: 0,
            close_height,
        }
    }

    /// Look up outcome by label.
    pub fn outcome_for_label(&self, label: &str) -> Option<OutcomeId> {
        self.outcome_labels
            .iter()
            .position(|l| l == label)
            .map(|i| self.outcomes[i])
    }

    /// Is `outcome` one of this market's allowed outcomes?
    pub fn has_outcome(&self, outcome: &OutcomeId) -> bool {
        self.outcomes.contains(outcome)
    }

    /// Transition into `Resolving` after the oracle has reported.
    pub fn begin_resolution(
        &mut self,
        winning_outcome: OutcomeId,
        claim_deadline: u64,
    ) -> Result<(), MarketError> {
        if !matches!(self.status, MarketStatus::Open) {
            return Err(MarketError::NotOpen);
        }
        if !self.has_outcome(&winning_outcome) {
            return Err(MarketError::UnknownOutcome);
        }
        self.status = MarketStatus::Resolving {
            winning_outcome,
            claim_deadline,
        };
        Ok(())
    }

    /// Finalize the market after settlement.
    pub fn finalize(&mut self, total_winning_stake: u64) -> Result<(), MarketError> {
        match &self.status {
            MarketStatus::Resolving {
                winning_outcome, ..
            } => {
                let winning_outcome = *winning_outcome;
                self.status = MarketStatus::Resolved {
                    winning_outcome,
                    total_pool: self.total_pool,
                    total_winning_stake,
                };
                Ok(())
            }
            _ => Err(MarketError::NotResolving),
        }
    }
}

/// Derive a content-addressed outcome id from a market id and a label.
pub fn derive_outcome_id(market_id: &MarketId, label: &str) -> OutcomeId {
    let mut hasher = blake3::Hasher::new_derive_key("pyana-prediction-market-outcome-id-v1");
    hasher.update(market_id);
    hasher.update(label.as_bytes());
    *hasher.finalize().as_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MarketError {
    #[error("market is not in the Open state")]
    NotOpen,
    #[error("outcome is not one of the market's declared outcomes")]
    UnknownOutcome,
    #[error("market is not in the Resolving state")]
    NotResolving,
}
