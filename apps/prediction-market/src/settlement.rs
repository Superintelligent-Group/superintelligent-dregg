//! Settlement: given a resolved market and the revealed winning bets,
//! compute payouts.
//!
//! Payout rule: each winner receives `stake_i * (total_pool / total_winning_stake)`.
//! Any rounding remainder is awarded to the first winner (or burned if no
//! winners — but we forbid empty winner sets in `Resolver::settle`).
//!
//! ## What "consulting the blinded queue" means here
//!
//! `Resolver::settle` accepts the live, post-consume set of revealed bets
//! (built up by the server when each consume succeeds). The function is
//! pure — given that set + the winning outcome + total pool, it produces a
//! deterministic `Vec<Payout>`. The integration test in `tests.rs` exercises
//! the full path: bets → queue commits → oracle reports → consumes →
//! settle, asserting that only matching-outcome bets get paid.

use serde::{Deserialize, Serialize};

use crate::bets::{BetPayload, BettorId};
use crate::market::OutcomeId;

/// A single payout to a bettor.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Payout {
    pub bettor: BettorId,
    pub amount: u64,
}

/// A revealed bet that has been consumed from the blinded queue.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevealedBet {
    pub payload: BetPayload,
    /// The nullifier that was published when the bet was consumed. Storing it
    /// lets the server deduplicate at this layer too.
    pub nullifier: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SettlementError {
    #[error("no revealed bets matched the winning outcome")]
    NoWinners,
    #[error("total winning stake is zero")]
    ZeroStake,
}

/// Compute the list of payouts for a resolved market.
pub fn settle(
    revealed: &[RevealedBet],
    winning_outcome: &OutcomeId,
    total_pool: u64,
) -> Result<Vec<Payout>, SettlementError> {
    let winners: Vec<&RevealedBet> = revealed
        .iter()
        .filter(|r| &r.payload.outcome_id == winning_outcome)
        .collect();

    if winners.is_empty() {
        return Err(SettlementError::NoWinners);
    }
    let total_winning_stake: u128 = winners.iter().map(|w| w.payload.stake as u128).sum();
    if total_winning_stake == 0 {
        return Err(SettlementError::ZeroStake);
    }

    let pool = total_pool as u128;
    let mut payouts = Vec::with_capacity(winners.len());
    let mut paid_so_far: u128 = 0;
    for (i, w) in winners.iter().enumerate() {
        let amount = if i + 1 == winners.len() {
            // Last winner takes the remainder so the books always balance.
            (pool - paid_so_far) as u64
        } else {
            let share = (w.payload.stake as u128) * pool / total_winning_stake;
            paid_so_far += share;
            share as u64
        };
        payouts.push(Payout {
            bettor: w.payload.bettor,
            amount,
        });
    }

    Ok(payouts)
}

/// Total stake on the winning outcome (helper for status endpoints).
pub fn total_winning_stake(revealed: &[RevealedBet], winning_outcome: &OutcomeId) -> u64 {
    revealed
        .iter()
        .filter(|r| &r.payload.outcome_id == winning_outcome)
        .map(|r| r.payload.stake)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rev(outcome: [u8; 32], bettor: [u8; 32], stake: u64) -> RevealedBet {
        RevealedBet {
            payload: BetPayload {
                market_id: [9u8; 32],
                outcome_id: outcome,
                stake,
                bettor,
            },
            nullifier: [0u8; 32],
        }
    }

    #[test]
    fn proportional_payouts_sum_to_pool() {
        let out_a = [1u8; 32];
        let out_b = [2u8; 32];
        let revealed = vec![
            rev(out_a, [10u8; 32], 100),
            rev(out_b, [11u8; 32], 50),
            rev(out_a, [12u8; 32], 300),
        ];
        // Pool was the sum of all stakes: 450.
        let payouts = settle(&revealed, &out_a, 450).unwrap();
        assert_eq!(payouts.len(), 2);
        let total: u64 = payouts.iter().map(|p| p.amount).sum();
        assert_eq!(total, 450);
        // The 100-stake winner should be paid less than the 300-stake winner.
        let p10 = payouts.iter().find(|p| p.bettor == [10u8; 32]).unwrap();
        let p12 = payouts.iter().find(|p| p.bettor == [12u8; 32]).unwrap();
        assert!(p12.amount > p10.amount);
    }

    #[test]
    fn no_winners_is_an_error() {
        let out_a = [1u8; 32];
        let out_b = [2u8; 32];
        let revealed = vec![rev(out_b, [11u8; 32], 50)];
        assert_eq!(settle(&revealed, &out_a, 50), Err(SettlementError::NoWinners));
    }
}
