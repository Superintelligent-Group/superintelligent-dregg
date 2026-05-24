//! Voter registry, quorum threshold logic, and the queue gating program.
//!
//! # Quorum semantics
//!
//! The threshold is `(total_weight * 2) / 3 + 1` (i.e. 2/3+1) — the same rule
//! that `apps/governed-namespace` uses. Voters are identified by 32-byte
//! identifiers (typically a public key). Each voter has an integer `weight`.
//!
//! # QuorumGate (queue program)
//!
//! `QueueProgram` from `pyana-storage` lets us attach validation constraints
//! to a queue. The relevant variant here is [`QueueConstraint::Custom`]:
//!
//! > Custom (arbitrary constraint expression — full DSL power).
//!
//! In the current storage implementation, `Custom` is a *pass-through* at
//! validation time:
//!
//! ```text
//! QueueConstraint::Custom { expr: _, description } => {
//!     // Custom constraints require external evaluation.
//!     // For now, they always pass during local validation.
//! }
//! ```
//!
//! So we **cannot** rely on `Custom` alone to enforce "this proposal has
//! quorum" at `enqueue_validated` time. Instead, [`QuorumGate`] performs the
//! check at the application layer in [`crate::server`] before the request
//! reaches the queue, and the `Custom` constraint is included on the queue's
//! program so that the queue's `vk_hash` content-addresses the gating rule.
//!
//! See `REVIEW[P1]:` in [`QuorumGate::queue_program`].

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use pyana_storage::programmable::{ProgrammableQueue, QueueConstraint, QueueProgram};

use crate::proposal::{Proposal, ProposalId, ProposalStatus};

/// A registered voter: 32-byte id + voting weight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Voter {
    pub id: [u8; 32],
    pub weight: u32,
}

/// Shared governance state: voter set, proposals indexed by id.
#[derive(Clone)]
pub struct GovernanceState {
    inner: Arc<RwLock<Inner>>,
}

struct Inner {
    voters: HashMap<[u8; 32], u32>,
    proposals: HashMap<ProposalId, Proposal>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GovernanceError {
    #[error("voter not registered")]
    NotVoter,
    #[error("proposal not found")]
    ProposalNotFound,
    #[error("proposal already exists")]
    ProposalDuplicate,
    #[error("voter already voted on this proposal")]
    AlreadyVoted,
    #[error("proposal is not in submitted state")]
    NotSubmitted,
    #[error("proposal does not meet quorum (have {have}, need {need})")]
    QuorumNotMet { have: u32, need: u32 },
}

impl GovernanceState {
    pub fn new(voters: Vec<Voter>) -> Self {
        let voters = voters.into_iter().map(|v| (v.id, v.weight)).collect();
        Self {
            inner: Arc::new(RwLock::new(Inner {
                voters,
                proposals: HashMap::new(),
            })),
        }
    }

    pub async fn total_weight(&self) -> u32 {
        self.inner.read().await.voters.values().sum()
    }

    /// 2/3+1 threshold (matches the governance reference in
    /// `apps/governed-namespace`).
    pub async fn threshold(&self) -> u32 {
        let total = self.total_weight().await;
        (total * 2) / 3 + 1
    }

    pub async fn is_voter(&self, id: &[u8; 32]) -> bool {
        self.inner.read().await.voters.contains_key(id)
    }

    pub async fn weight_of(&self, id: &[u8; 32]) -> u32 {
        self.inner.read().await.voters.get(id).copied().unwrap_or(0)
    }

    /// Insert a new proposal. Errors if the proposer is not a registered voter
    /// (so anonymous submissions can't fill the proposal table).
    pub async fn submit(&self, proposal: Proposal) -> Result<ProposalId, GovernanceError> {
        let mut g = self.inner.write().await;
        if !g.voters.contains_key(&proposal.proposer) {
            return Err(GovernanceError::NotVoter);
        }
        if g.proposals.contains_key(&proposal.id) {
            return Err(GovernanceError::ProposalDuplicate);
        }
        let id = proposal.id;
        g.proposals.insert(id, proposal);
        Ok(id)
    }

    /// Cast a vote. Returns the proposal's status after the vote is applied.
    pub async fn vote(
        &self,
        proposal_id: &ProposalId,
        voter_id: [u8; 32],
        approve: bool,
    ) -> Result<ProposalStatus, GovernanceError> {
        let total = self.total_weight().await;
        let threshold = (total * 2) / 3 + 1;
        let mut g = self.inner.write().await;
        let weight = *g.voters.get(&voter_id).ok_or(GovernanceError::NotVoter)?;
        let proposal = g
            .proposals
            .get_mut(proposal_id)
            .ok_or(GovernanceError::ProposalNotFound)?;
        if proposal.status != ProposalStatus::Submitted {
            return Err(GovernanceError::NotSubmitted);
        }
        if proposal.voted_by.contains(&voter_id) {
            return Err(GovernanceError::AlreadyVoted);
        }
        proposal.voted_by.push(voter_id);
        if approve {
            proposal.approve_weight = proposal.approve_weight.saturating_add(weight);
        } else {
            proposal.reject_weight = proposal.reject_weight.saturating_add(weight);
        }
        if proposal.approve_weight >= threshold {
            proposal.status = ProposalStatus::Approved;
        } else if proposal.reject_weight > total - threshold {
            proposal.status = ProposalStatus::Rejected;
        }
        Ok(proposal.status)
    }

    /// Read-only snapshot of a single proposal.
    pub async fn get(&self, id: &ProposalId) -> Option<Proposal> {
        self.inner.read().await.proposals.get(id).cloned()
    }

    /// All proposals currently in `Approved` status (eligible for execution).
    pub async fn approved(&self) -> Vec<Proposal> {
        self.inner
            .read()
            .await
            .proposals
            .values()
            .filter(|p| p.status == ProposalStatus::Approved)
            .cloned()
            .collect()
    }

    /// Mark a proposal as executed. Called by the batch executor on success.
    pub async fn mark_executed(&self, id: &ProposalId) -> Result<(), GovernanceError> {
        let mut g = self.inner.write().await;
        let p = g
            .proposals
            .get_mut(id)
            .ok_or(GovernanceError::ProposalNotFound)?;
        p.status = ProposalStatus::Executed;
        Ok(())
    }

    /// Verify a proposal exists AND meets quorum. This is the gate the queue
    /// endpoint runs at the application layer (see [`QuorumGate`]).
    pub async fn check_quorum(&self, id: &ProposalId) -> Result<(), GovernanceError> {
        let threshold = self.threshold().await;
        let g = self.inner.read().await;
        let p = g
            .proposals
            .get(id)
            .ok_or(GovernanceError::ProposalNotFound)?;
        if !p.meets_quorum(threshold) {
            return Err(GovernanceError::QuorumNotMet {
                have: p.approve_weight,
                need: threshold,
            });
        }
        Ok(())
    }
}

/// The queue-program gate: declares to the world (via content-addressed
/// `vk_hash`) that this queue only accepts quorum-met proposals.
pub struct QuorumGate;

impl QuorumGate {
    /// Build the `QueueProgram` whose `Custom` constraint commits to the
    /// quorum-gating rule.
    ///
    /// # REVIEW[P1]: the `Custom` constraint is unenforced at the storage layer
    ///
    /// `pyana_storage::programmable::ProgrammableQueue` (see
    /// `storage/src/programmable.rs`) treats `QueueConstraint::Custom` as a
    /// pass-through:
    /// ```text
    /// QueueConstraint::Custom { .. } => { /* always passes */ }
    /// ```
    /// so the `Custom { expr: "approve_weight >= threshold", .. }` we attach
    /// here does NOT cause the storage layer to reject under-quorum entries.
    /// It only causes the queue's `vk_hash` to commit to the gating rule.
    ///
    /// Until the storage layer can dispatch to a host-provided evaluator for
    /// `Custom` constraints, the actual gating is enforced at the application
    /// layer in [`crate::server::handle_proposal_enqueue`] by calling
    /// [`GovernanceState::check_quorum`] before forwarding to the queue.
    ///
    /// **Adversarial test**: `crate::tests::under_quorum_proposal_rejected_at_queue_time`
    /// submits a proposal without quorum and asserts the application-layer
    /// gate rejects it. Without that test, the property would be aspirational.
    pub fn queue_program() -> QueueProgram {
        QueueProgram {
            name: "dao-treasury-quorum-gate".to_string(),
            constraints: vec![
                QueueConstraint::MinDeposit { amount: 0 },
                QueueConstraint::Custom {
                    expr: "proposal.approve_weight >= ceil(total_weight * 2 / 3) + 1".to_string(),
                    description:
                        "proposal must meet the 2/3+1 quorum threshold before it can be enqueued"
                            .to_string(),
                },
            ],
            lookup_tables: Vec::new(),
        }
    }

    /// Build a queue using [`Self::queue_program`].
    pub fn make_queue(
        name: impl Into<String>,
        owner: [u8; 32],
        capacity: usize,
    ) -> ProgrammableQueue {
        ProgrammableQueue::new(name.into(), owner, Self::queue_program(), None, capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposal::SpendOrder;

    fn voters() -> Vec<Voter> {
        vec![
            Voter {
                id: [1; 32],
                weight: 1,
            },
            Voter {
                id: [2; 32],
                weight: 1,
            },
            Voter {
                id: [3; 32],
                weight: 1,
            },
        ]
    }

    fn proposal(proposer: [u8; 32]) -> Proposal {
        Proposal::new(
            proposer,
            vec![SpendOrder {
                asset: [0xAA; 32],
                amount: 1,
                recipient: [9; 32],
            }],
        )
    }

    #[tokio::test]
    async fn threshold_is_two_thirds_plus_one() {
        let g = GovernanceState::new(voters());
        // 3 voters * weight 1 = 3 total. (3*2)/3+1 = 3.
        assert_eq!(g.threshold().await, 3);
    }

    #[tokio::test]
    async fn submit_then_vote_to_quorum() {
        let g = GovernanceState::new(voters());
        let p = proposal([1; 32]);
        let id = g.submit(p).await.unwrap();
        assert_eq!(
            g.vote(&id, [1; 32], true).await.unwrap(),
            ProposalStatus::Submitted
        );
        assert_eq!(
            g.vote(&id, [2; 32], true).await.unwrap(),
            ProposalStatus::Submitted
        );
        assert_eq!(
            g.vote(&id, [3; 32], true).await.unwrap(),
            ProposalStatus::Approved
        );
        assert!(g.check_quorum(&id).await.is_ok());
    }

    #[tokio::test]
    async fn under_quorum_check_quorum_errors() {
        let g = GovernanceState::new(voters());
        let p = proposal([1; 32]);
        let id = g.submit(p).await.unwrap();
        // Only 1/3 votes — well under threshold of 3.
        g.vote(&id, [1; 32], true).await.unwrap();
        let err = g.check_quorum(&id).await.unwrap_err();
        assert!(matches!(
            err,
            GovernanceError::QuorumNotMet { have: 1, need: 3 }
        ));
    }

    #[tokio::test]
    async fn non_voter_cannot_submit_or_vote() {
        let g = GovernanceState::new(voters());
        let p = proposal([0xFF; 32]);
        let err = g.submit(p).await.unwrap_err();
        assert_eq!(err, GovernanceError::NotVoter);

        let p = proposal([1; 32]);
        let id = g.submit(p).await.unwrap();
        let err = g.vote(&id, [0xFF; 32], true).await.unwrap_err();
        assert_eq!(err, GovernanceError::NotVoter);
    }

    #[tokio::test]
    async fn duplicate_vote_rejected() {
        let g = GovernanceState::new(voters());
        let p = proposal([1; 32]);
        let id = g.submit(p).await.unwrap();
        g.vote(&id, [1; 32], true).await.unwrap();
        let err = g.vote(&id, [1; 32], false).await.unwrap_err();
        assert_eq!(err, GovernanceError::AlreadyVoted);
    }

    #[test]
    fn quorum_gate_program_is_distinct_from_open() {
        let gate = QuorumGate::queue_program();
        let open = pyana_storage::programmable::programs::open(0);
        assert_ne!(gate.name, open.name);
        // The gate has a Custom constraint encoding the quorum rule.
        assert!(
            gate.constraints
                .iter()
                .any(|c| matches!(c, QueueConstraint::Custom { .. }))
        );
    }
}
