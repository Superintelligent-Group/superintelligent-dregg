//! Spending proposals and their lifecycle states.
//!
//! A [`Proposal`] is the basic governance object: a request to debit one or
//! more assets from the treasury and send them to a recipient. Proposals carry
//! a status that tracks:
//!
//! - `Submitted` — created, awaiting votes
//! - `Approved` — quorum-met, eligible for batch execution
//! - `Executed` — the executor has settled the batch
//! - `Rejected` — explicitly voted down or the executor refused (e.g. funds)
//!
//! The proposal is the *unit of validation*: the queue program (see
//! [`crate::governance::QuorumGate`]) examines a proposal's approval count
//! before enqueueing.

use serde::{Deserialize, Serialize};

use crate::treasury::AssetId;

/// 32-byte content-addressed proposal id (blake3 of canonical bytes).
pub type ProposalId = [u8; 32];

/// A single asset/amount/recipient spend instruction inside a proposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpendOrder {
    pub asset: AssetId,
    pub amount: u128,
    /// 32-byte recipient identifier (CellId, pubkey, whatever the host app uses).
    pub recipient: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Submitted,
    Approved,
    Executed,
    Rejected,
}

/// A spending proposal.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub id: ProposalId,
    /// 32-byte identifier of the proposer (who staked to submit).
    pub proposer: [u8; 32],
    /// One or more spend instructions; the proposal is atomic across them.
    pub orders: Vec<SpendOrder>,
    /// Approval weight tallied so far.
    pub approve_weight: u32,
    /// Reject weight tallied so far.
    pub reject_weight: u32,
    /// Voters who have already cast a vote (no double-voting).
    pub voted_by: Vec<[u8; 32]>,
    pub status: ProposalStatus,
}

impl Proposal {
    /// Create a new submitted proposal.
    pub fn new(proposer: [u8; 32], orders: Vec<SpendOrder>) -> Self {
        let id = compute_id(&proposer, &orders);
        Self {
            id,
            proposer,
            orders,
            approve_weight: 0,
            reject_weight: 0,
            voted_by: Vec::new(),
            status: ProposalStatus::Submitted,
        }
    }

    /// Whether the proposal has enough approve weight to be eligible for
    /// execution under the supplied `threshold`. This is the property the
    /// queue's custom constraint gates on.
    pub fn meets_quorum(&self, threshold: u32) -> bool {
        self.approve_weight >= threshold
    }
}

/// Canonical proposal id: blake3(domain || proposer || orders).
fn compute_id(proposer: &[u8; 32], orders: &[SpendOrder]) -> ProposalId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"pyana-dao-treasury-proposal-v1");
    hasher.update(proposer);
    for o in orders {
        hasher.update(&o.asset);
        hasher.update(&o.amount.to_le_bytes());
        hasher.update(&o.recipient);
    }
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_deterministic() {
        let orders = vec![SpendOrder {
            asset: [1; 32],
            amount: 100,
            recipient: [2; 32],
        }];
        let p1 = Proposal::new([3; 32], orders.clone());
        let p2 = Proposal::new([3; 32], orders);
        assert_eq!(p1.id, p2.id);
    }

    #[test]
    fn quorum_check_is_strictly_geq() {
        let mut p = Proposal::new([0; 32], vec![]);
        p.approve_weight = 2;
        assert!(!p.meets_quorum(3));
        p.approve_weight = 3;
        assert!(p.meets_quorum(3));
    }
}
