//! Bet placement and reveal.
//!
//! A bet is `(market_id, outcome_id, stake, bettor_pubkey, secret)`. The
//! commitment that enters the blinded queue is the blake3 hash of these
//! fields together with a domain separator. The bettor keeps `secret` private
//! until the market resolves; at that point they reveal the bet alongside a
//! consumption proof (which contains the nullifier `blake3("...nullifier..."
//! || commitment || secret || position)`).
//!
//! ## Naming honesty
//!
//! No type is called "Encrypted" — bets are **plaintext-but-committed**. The
//! privacy guarantee is that the operator sees only `Commitment` until reveal,
//! which is exactly what `BlindedQueue` provides. If we ever wrap reveals in
//! actual ciphertext we will introduce a separate `EncryptedReveal` type with
//! a real cipher.

use pyana_storage::blinded::crypto as blinded_crypto;
use pyana_storage::commitment::BlindedItemCommitment;
use serde::{Deserialize, Serialize};

use crate::market::{MarketId, OutcomeId};

/// A bettor's identifier (their cell-id / pubkey).
pub type BettorId = [u8; 32];

/// The cleartext payload a bettor commits to.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BetPayload {
    pub market_id: MarketId,
    pub outcome_id: OutcomeId,
    pub stake: u64,
    pub bettor: BettorId,
}

impl BetPayload {
    /// Canonical byte serialization for commitments (length-prefixed fields
    /// so commitments are unambiguous even if encodings change).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + 32 + 8 + 32);
        out.extend_from_slice(&self.market_id);
        out.extend_from_slice(&self.outcome_id);
        out.extend_from_slice(&self.stake.to_le_bytes());
        out.extend_from_slice(&self.bettor);
        out
    }
}

/// A bet that has been placed but not revealed.
///
/// Holds the data needed to consume the commitment when resolution happens.
#[derive(Clone, Debug)]
pub struct PendingBet {
    pub payload: BetPayload,
    pub commitment: BlindedItemCommitment,
    pub secret: [u8; 32],
    /// Position in the blinded queue (assigned when committed).
    pub position: usize,
}

/// Compute the bet commitment.
///
/// Uses the same blinded-queue commitment derivation as
/// [`pyana_storage::blinded::crypto::create_commitment`], so the resulting
/// hash is what the queue will store. The "randomness" input is the bettor's
/// `secret`, which they will reveal at consume time.
pub fn create_bet_commitment(payload: &BetPayload, secret: &[u8; 32]) -> BlindedItemCommitment {
    blinded_crypto::create_commitment(&payload.canonical_bytes(), secret)
}

/// Build the consumption proof for a pending bet at its assigned position,
/// given the merkle siblings.
pub fn build_consumption_proof(
    pending: &PendingBet,
    merkle_siblings: Vec<[u8; 32]>,
) -> pyana_storage::blinded::ConsumptionProof {
    blinded_crypto::build_consumption_proof(
        pending.commitment,
        pending.secret,
        pending.position,
        merkle_siblings.into_iter().map(Into::into).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commitment_is_deterministic() {
        let payload = BetPayload {
            market_id: [1u8; 32],
            outcome_id: [2u8; 32],
            stake: 1000,
            bettor: [3u8; 32],
        };
        let secret = [4u8; 32];
        let a = create_bet_commitment(&payload, &secret);
        let b = create_bet_commitment(&payload, &secret);
        assert_eq!(a, b);
    }

    #[test]
    fn different_secrets_produce_different_commitments() {
        let payload = BetPayload {
            market_id: [1u8; 32],
            outcome_id: [2u8; 32],
            stake: 1000,
            bettor: [3u8; 32],
        };
        assert_ne!(
            create_bet_commitment(&payload, &[4u8; 32]),
            create_bet_commitment(&payload, &[5u8; 32]),
        );
    }
}
