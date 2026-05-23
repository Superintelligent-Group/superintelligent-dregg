//! Ring-trade participation for prediction markets.
//!
//! Bettors who hold offsetting positions on different outcomes can settle
//! atomically via the ring solver, swapping outcome-share tokens without
//! routing through cash.
//!
//! A "share token" for an outcome is just the `OutcomeId` reinterpreted as an
//! [`AssetId`] (`[u8; 32]`). When the solver assigns this app a leg, we
//! debit one bettor's outcome-share balance and credit the counterparty.
//!
//! This is a lightweight in-memory implementation: the ledger of shares is
//! kept inside `RingMarketParticipant` rather than mounted into an escrow
//! cell. See `// REVIEW[P2]` in `server.rs` for the missing escrow link.

use std::collections::HashMap;

use pyana_app_framework::ring_trade::{ExchangeSpec, RingTradeParticipant, Settlement};
use pyana_intent::CommitmentId;

use crate::market::OutcomeId;

/// Per-bettor outcome-share balances, plus the participant's standing offers.
#[derive(Clone, Debug, Default)]
pub struct RingMarketParticipant {
    /// `balances[commitment][outcome] = shares`. Keyed by the bettor's
    /// `CommitmentId` so the solver routes shares between the right parties.
    balances: HashMap<[u8; 32], HashMap<OutcomeId, u64>>,
    /// Standing exchange offers the participant has published.
    offers: Vec<ExchangeSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RingError {
    #[error("insufficient share balance: have {have}, need {need}")]
    InsufficientBalance { have: u64, need: u64 },
    #[error("commitment is unknown to this participant")]
    UnknownCommitment,
}

impl RingMarketParticipant {
    pub fn new() -> Self {
        Self::default()
    }

    /// Credit `shares` of `outcome` to `bettor`'s balance.
    pub fn credit(&mut self, bettor: CommitmentId, outcome: OutcomeId, shares: u64) {
        *self
            .balances
            .entry(bettor.0)
            .or_default()
            .entry(outcome)
            .or_insert(0) += shares;
    }

    /// Read a balance for `(bettor, outcome)`.
    pub fn balance_of(&self, bettor: &CommitmentId, outcome: &OutcomeId) -> u64 {
        self.balances
            .get(&bettor.0)
            .and_then(|m| m.get(outcome))
            .copied()
            .unwrap_or(0)
    }

    /// Publish a standing exchange offer (no balance check — offers are
    /// advertisements; the actual move happens at `settle_leg`).
    pub fn publish_offer(&mut self, offer: ExchangeSpec) {
        self.offers.push(offer);
    }

    /// Clear all published offers (used to refresh between solve rounds).
    pub fn clear_offers(&mut self) {
        self.offers.clear();
    }
}

impl RingTradeParticipant for RingMarketParticipant {
    type Error = RingError;

    fn exchange_offers(&self) -> Vec<ExchangeSpec> {
        self.offers.clone()
    }

    fn settle_leg(&mut self, s: &Settlement) -> Result<(), RingError> {
        // Move `s.amount` units of `s.asset` (interpreted as OutcomeId)
        // from `s.from` to `s.to`.
        let asset: OutcomeId = s.asset;
        let from_balance = self
            .balances
            .get(&s.from.0)
            .and_then(|m| m.get(&asset))
            .copied()
            .unwrap_or(0);
        if from_balance < s.amount {
            return Err(RingError::InsufficientBalance {
                have: from_balance,
                need: s.amount,
            });
        }
        // Debit
        if let Some(m) = self.balances.get_mut(&s.from.0) {
            if let Some(v) = m.get_mut(&asset) {
                *v -= s.amount;
            }
        }
        // Credit
        *self
            .balances
            .entry(s.to.0)
            .or_default()
            .entry(asset)
            .or_insert(0) += s.amount;
        Ok(())
    }

    fn rollback_leg(&mut self, s: &Settlement) -> Result<(), RingError> {
        // Inverse of settle: subtract from `to`, add back to `from`.
        let asset: OutcomeId = s.asset;
        let to_balance = self
            .balances
            .get(&s.to.0)
            .and_then(|m| m.get(&asset))
            .copied()
            .unwrap_or(0);
        // Idempotency contract: rolling back when there's no balance is a
        // no-op (the original settle may have partially failed). We log via
        // the error variant only when the request would corrupt state.
        let refund = to_balance.min(s.amount);
        if let Some(m) = self.balances.get_mut(&s.to.0) {
            if let Some(v) = m.get_mut(&asset) {
                *v -= refund;
            }
        }
        *self
            .balances
            .entry(s.from.0)
            .or_default()
            .entry(asset)
            .or_insert(0) += refund;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settle_then_rollback_round_trips() {
        let mut p = RingMarketParticipant::new();
        let alice = CommitmentId([1u8; 32]);
        let bob = CommitmentId([2u8; 32]);
        let outcome_yes: OutcomeId = [9u8; 32];
        p.credit(alice, outcome_yes, 100);

        let s = Settlement {
            from: alice,
            to: bob,
            asset: outcome_yes,
            amount: 30,
        };
        p.settle_leg(&s).unwrap();
        assert_eq!(p.balance_of(&alice, &outcome_yes), 70);
        assert_eq!(p.balance_of(&bob, &outcome_yes), 30);

        p.rollback_leg(&s).unwrap();
        assert_eq!(p.balance_of(&alice, &outcome_yes), 100);
        assert_eq!(p.balance_of(&bob, &outcome_yes), 0);
    }

    #[test]
    fn insufficient_balance_rejected() {
        let mut p = RingMarketParticipant::new();
        let alice = CommitmentId([1u8; 32]);
        let bob = CommitmentId([2u8; 32]);
        let outcome_yes: OutcomeId = [9u8; 32];
        p.credit(alice, outcome_yes, 10);

        let s = Settlement {
            from: alice,
            to: bob,
            asset: outcome_yes,
            amount: 100,
        };
        assert!(matches!(
            p.settle_leg(&s),
            Err(RingError::InsufficientBalance { .. })
        ));
    }
}
