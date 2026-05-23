//! Constitutional Consensus: Democratic Membership Amendment Protocol.
//!
//! From the Constitutional Consensus paper (arXiv:2505.19216): participants in a
//! federation can propose and vote on membership changes. The constitution defines
//! the participant set, supermajority threshold, and rules for amendment.
//!
//! Key concepts:
//! - **Constitution**: the current participant set + threshold + version.
//! - **MembershipProposal**: a proposal to join, leave, or amend the threshold.
//! - **H-Rule**: changing the threshold from T to T' requires max(T, T') votes.
//! - **Auto-eviction**: equivocation proofs immediately remove the equivocator.
//! - **Voting via blocks**: votes reference the proposal block in their causal past.

use serde::{Deserialize, Serialize};

use crate::finality::{BlockId, EquivocationProof};

// ─── Constitution ──────────────────────────────────────────────────────────────

/// The federation's constitution (amendable by participants).
///
/// Tracks the current participant set, supermajority threshold, and version.
/// Each amendment increments the version, providing a linearizable history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constitution {
    /// Current participant set (public keys, sorted for determinism).
    pub participants: Vec<[u8; 32]>,
    /// Supermajority threshold (default: 2n/3 + 1).
    pub threshold: usize,
    /// Timeout delta for view advancement (milliseconds).
    pub timeout_ms: u64,
    /// Constitution version (incremented on each amendment).
    pub version: u64,
}

impl Constitution {
    /// Create a new constitution with the given initial participants.
    ///
    /// Threshold defaults to 2n/3 + 1 (supermajority).
    pub fn new(mut participants: Vec<[u8; 32]>, timeout_ms: u64) -> Self {
        participants.sort();
        participants.dedup();
        let threshold = compute_threshold(participants.len());
        Constitution {
            participants,
            threshold,
            timeout_ms,
            version: 0,
        }
    }

    /// Number of participants in the federation.
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    /// Check if a key is a current participant.
    pub fn is_participant(&self, key: &[u8; 32]) -> bool {
        self.participants.contains(key)
    }

    /// The number of votes required for a given proposal to pass.
    ///
    /// Implements the H-rule: amending the threshold from T to T' requires
    /// max(T, T') votes. This prevents a minority from lowering the threshold
    /// to seize control, or a majority from raising it to lock others out.
    pub fn required_votes_for(&self, proposal: &MembershipProposal) -> usize {
        match proposal {
            MembershipProposal::AmendThreshold { new_threshold } => {
                // H-rule: need max(current, new) votes
                std::cmp::max(self.threshold, *new_threshold)
            }
            _ => self.threshold,
        }
    }

    /// Apply a membership proposal to the constitution.
    ///
    /// This mutates the participant set, recomputes the threshold (for
    /// join/leave), increments the version, and returns true if the change
    /// was actually applied (e.g., false if trying to add an existing member).
    pub fn apply_proposal(&mut self, proposal: &MembershipProposal) -> bool {
        match proposal {
            MembershipProposal::Join {
                node_key,
                justification: _,
            } => {
                if self.participants.contains(node_key) {
                    return false; // Already a member
                }
                self.participants.push(*node_key);
                self.participants.sort();
                self.threshold = compute_threshold(self.participants.len());
                self.version += 1;
                true
            }
            MembershipProposal::Leave {
                node_key,
                reason: _,
            } => {
                let before = self.participants.len();
                self.participants.retain(|k| k != node_key);
                if self.participants.len() == before {
                    return false; // Not a member
                }
                self.threshold = compute_threshold(self.participants.len());
                self.version += 1;
                true
            }
            MembershipProposal::AmendThreshold { new_threshold } => {
                if *new_threshold == self.threshold {
                    return false; // No change
                }
                if *new_threshold == 0 || *new_threshold > self.participants.len() {
                    return false; // Invalid threshold
                }
                self.threshold = *new_threshold;
                self.version += 1;
                true
            }
        }
    }

    /// Auto-evict an equivocator based on cryptographic proof.
    ///
    /// Since equivocation proofs are self-evident (two conflicting signed blocks),
    /// this does NOT require a vote -- it applies immediately.
    ///
    /// Returns true if the equivocator was actually a participant and was removed.
    pub fn auto_evict_equivocator(&mut self, proof: &EquivocationProof) -> bool {
        let evicted = proof.creator;
        if !self.participants.contains(&evicted) {
            return false;
        }
        self.participants.retain(|k| k != &evicted);
        self.threshold = compute_threshold(self.participants.len());
        self.version += 1;
        true
    }
}

// ─── Membership Proposals ──────────────────────────────────────────────────────

/// A proposal to change federation membership or rules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembershipProposal {
    /// Add a new participant to the federation.
    Join {
        node_key: [u8; 32],
        /// Justification (e.g., stake proof, governance vote).
        justification: Vec<u8>,
    },
    /// Remove a participant (voluntary leave or eviction).
    Leave {
        node_key: [u8; 32],
        reason: LeaveReason,
    },
    /// Amend the supermajority threshold.
    /// The H-rule applies: changing from T to T' requires max(T, T') votes.
    AmendThreshold { new_threshold: usize },
}

/// Reason for a participant leaving the federation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaveReason {
    /// Participant chose to leave.
    Voluntary,
    /// Participant was evicted due to equivocation.
    Evicted {
        /// The two conflicting blocks (serialized for compactness).
        block_a_bytes: Vec<u8>,
        block_b_bytes: Vec<u8>,
    },
}

// ─── Membership Votes ──────────────────────────────────────────────────────────

/// A vote on a membership proposal.
///
/// Votes are expressed as block payloads that reference the proposal block
/// in their causal past. A proposal passes when `threshold` distinct approving
/// votes exist in the blocklace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipVote {
    /// The block ID containing the proposal being voted on.
    pub proposal_block: BlockId,
    /// Whether this vote approves (true) or rejects (false) the proposal.
    pub approve: bool,
}

// ─── Vote Tracker ──────────────────────────────────────────────────────────────

/// Tracks votes for pending membership proposals.
///
/// A proposal passes when it accumulates the required number of approving votes
/// from distinct participants AND is in the causal past of a finalized leader.
#[derive(Clone, Debug, Default)]
pub struct VoteTracker {
    /// proposal_block_id -> set of approving voter keys.
    approvals: std::collections::HashMap<BlockId, std::collections::HashSet<[u8; 32]>>,
    /// proposal_block_id -> set of rejecting voter keys.
    rejections: std::collections::HashMap<BlockId, std::collections::HashSet<[u8; 32]>>,
    /// proposal_block_id -> the proposal itself (for lookup).
    proposals: std::collections::HashMap<BlockId, MembershipProposal>,
    /// Proposals that have been applied (to prevent double-application).
    applied: std::collections::HashSet<BlockId>,
}

impl VoteTracker {
    /// Create a new empty vote tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a proposal. Returns false if already registered.
    pub fn register_proposal(
        &mut self,
        proposal_block: BlockId,
        proposal: MembershipProposal,
    ) -> bool {
        if self.proposals.contains_key(&proposal_block) {
            return false;
        }
        self.proposals.insert(proposal_block, proposal);
        self.approvals.entry(proposal_block).or_default();
        self.rejections.entry(proposal_block).or_default();
        true
    }

    /// Record a vote. The voter must be a current participant.
    ///
    /// Returns the current approval count for this proposal.
    pub fn record_vote(
        &mut self,
        vote: &MembershipVote,
        voter: [u8; 32],
        constitution: &Constitution,
    ) -> usize {
        // Only current participants can vote.
        if !constitution.is_participant(&voter) {
            return self.approval_count(&vote.proposal_block);
        }

        // Only vote on known proposals.
        if !self.proposals.contains_key(&vote.proposal_block) {
            return 0;
        }

        if vote.approve {
            self.approvals
                .entry(vote.proposal_block)
                .or_default()
                .insert(voter);
        } else {
            self.rejections
                .entry(vote.proposal_block)
                .or_default()
                .insert(voter);
        }

        self.approval_count(&vote.proposal_block)
    }

    /// Get the number of approvals for a proposal.
    pub fn approval_count(&self, proposal_block: &BlockId) -> usize {
        self.approvals
            .get(proposal_block)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// Get the number of rejections for a proposal.
    pub fn rejection_count(&self, proposal_block: &BlockId) -> usize {
        self.rejections
            .get(proposal_block)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// Check if a proposal has reached the required vote threshold.
    pub fn has_passed(&self, proposal_block: &BlockId, constitution: &Constitution) -> bool {
        if self.applied.contains(proposal_block) {
            return false; // Already applied
        }
        let proposal = match self.proposals.get(proposal_block) {
            Some(p) => p,
            None => return false,
        };
        let required = constitution.required_votes_for(proposal);
        self.approval_count(proposal_block) >= required
    }

    /// Get the proposal for a given block ID.
    pub fn get_proposal(&self, proposal_block: &BlockId) -> Option<&MembershipProposal> {
        self.proposals.get(proposal_block)
    }

    /// Mark a proposal as applied (prevents double-application).
    pub fn mark_applied(&mut self, proposal_block: &BlockId) {
        self.applied.insert(*proposal_block);
    }

    /// Check if a proposal has already been applied.
    pub fn is_applied(&self, proposal_block: &BlockId) -> bool {
        self.applied.contains(proposal_block)
    }

    /// Get all proposals that have passed but not yet been applied.
    pub fn pending_passed(&self, constitution: &Constitution) -> Vec<BlockId> {
        self.proposals
            .keys()
            .filter(|id| self.has_passed(id, constitution))
            .copied()
            .collect()
    }
}

// ─── Constitution Manager ──────────────────────────────────────────────────────

/// Manages the full lifecycle of constitutional amendments.
///
/// Integrates the Constitution, VoteTracker, and history of past constitutions
/// (for verifying blocks against the constitution that was active when they
/// were created).
#[derive(Clone, Debug)]
pub struct ConstitutionManager {
    /// The current (active) constitution.
    pub current: Constitution,
    /// Vote tracker for pending proposals.
    pub votes: VoteTracker,
    /// History of past constitutions (version -> constitution snapshot).
    /// Used to verify blocks against the constitution active at their creation time.
    history: Vec<Constitution>,
}

impl ConstitutionManager {
    /// Create a new constitution manager with the given initial constitution.
    pub fn new(constitution: Constitution) -> Self {
        let history = vec![constitution.clone()];
        ConstitutionManager {
            current: constitution,
            votes: VoteTracker::new(),
            history,
        }
    }

    /// Create from initial participants with default timeout.
    pub fn from_participants(participants: Vec<[u8; 32]>, timeout_ms: u64) -> Self {
        Self::new(Constitution::new(participants, timeout_ms))
    }

    /// Get the constitution at a specific version.
    pub fn constitution_at_version(&self, version: u64) -> Option<&Constitution> {
        self.history.get(version as usize)
    }

    /// Process a proposal: register it in the vote tracker.
    pub fn submit_proposal(
        &mut self,
        proposal_block: BlockId,
        proposal: MembershipProposal,
    ) -> bool {
        self.votes.register_proposal(proposal_block, proposal)
    }

    /// Process a vote on a proposal.
    ///
    /// Returns `Some(proposal_block)` if the proposal has now reached threshold
    /// and is ready to be applied (pending finality confirmation).
    pub fn submit_vote(&mut self, vote: &MembershipVote, voter: [u8; 32]) -> Option<BlockId> {
        self.votes.record_vote(vote, voter, &self.current);
        if self.votes.has_passed(&vote.proposal_block, &self.current) {
            Some(vote.proposal_block)
        } else {
            None
        }
    }

    /// Apply a proposal that has passed AND been confirmed via finality.
    ///
    /// This is called when the proposal is in the causal past of a finalized
    /// leader (Cordial Miners finality). Returns true if successfully applied.
    pub fn apply_if_passed(&mut self, proposal_block: &BlockId) -> bool {
        if !self.votes.has_passed(proposal_block, &self.current) {
            return false;
        }

        let proposal = match self.votes.get_proposal(proposal_block) {
            Some(p) => p.clone(),
            None => return false,
        };

        if self.current.apply_proposal(&proposal) {
            self.votes.mark_applied(proposal_block);
            self.history.push(self.current.clone());
            true
        } else {
            false
        }
    }

    /// Auto-evict an equivocator. Does not require voting.
    ///
    /// Returns true if the equivocator was removed from the constitution.
    pub fn auto_evict(&mut self, proof: &EquivocationProof) -> bool {
        if self.current.auto_evict_equivocator(proof) {
            self.history.push(self.current.clone());
            true
        } else {
            false
        }
    }

    /// Get the current participant list (for use in ordering/cordiality checks).
    pub fn participants(&self) -> &[[u8; 32]] {
        &self.current.participants
    }

    /// Get the current threshold.
    pub fn threshold(&self) -> usize {
        self.current.threshold
    }

    /// Get the current constitution version.
    pub fn version(&self) -> u64 {
        self.current.version
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────────

/// Compute the default supermajority threshold for n participants.
///
/// Uses floor(2n/3) + 1, matching the BFT requirement of tolerating < n/3 faults.
pub fn compute_threshold(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    (n * 2 / 3) + 1
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finality::{Block, Payload};
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn random_key() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    fn make_node_key(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn make_participants(n: u8) -> Vec<[u8; 32]> {
        (1..=n).map(|i| make_node_key(i)).collect()
    }

    // ─── Constitution basics ────────────────────────────────────────────────

    #[test]
    fn constitution_new_computes_threshold() {
        let c = Constitution::new(make_participants(4), 1000);
        assert_eq!(c.threshold, 3); // floor(2*4/3) + 1 = 3
        assert_eq!(c.participant_count(), 4);
        assert_eq!(c.version, 0);
    }

    #[test]
    fn constitution_threshold_values() {
        assert_eq!(compute_threshold(3), 3); // 2*3/3 + 1 = 3
        assert_eq!(compute_threshold(4), 3); // 2*4/3 + 1 = 3
        assert_eq!(compute_threshold(7), 5); // 2*7/3 + 1 = 5
        assert_eq!(compute_threshold(10), 7); // 2*10/3 + 1 = 7
        assert_eq!(compute_threshold(1), 1); // 2*1/3 + 1 = 1
        assert_eq!(compute_threshold(0), 0);
    }

    // ─── Propose join → threshold approvals → member added ──────────────────

    #[test]
    fn propose_join_threshold_approvals_member_added() {
        let participants = make_participants(3);
        let mut mgr = ConstitutionManager::from_participants(participants.clone(), 1000);

        // threshold for 3 participants = 3
        assert_eq!(mgr.threshold(), 3);

        // Propose adding node 4
        let new_node = make_node_key(4);
        let proposal = MembershipProposal::Join {
            node_key: new_node,
            justification: b"stake proof".to_vec(),
        };
        let proposal_block = BlockId([0xAA; 32]);
        mgr.submit_proposal(proposal_block, proposal);

        // Vote from participant 1
        let vote = MembershipVote {
            proposal_block,
            approve: true,
        };
        let result = mgr.submit_vote(&vote, make_node_key(1));
        assert_eq!(result, None); // Not yet passed

        // Vote from participant 2
        let result = mgr.submit_vote(&vote, make_node_key(2));
        assert_eq!(result, None); // Still not passed (need 3)

        // Vote from participant 3 -- reaches threshold
        let result = mgr.submit_vote(&vote, make_node_key(3));
        assert_eq!(result, Some(proposal_block));

        // Apply (simulating finality confirmation)
        assert!(mgr.apply_if_passed(&proposal_block));
        assert!(mgr.current.is_participant(&new_node));
        assert_eq!(mgr.current.participant_count(), 4);
        assert_eq!(mgr.current.version, 1);
        // Threshold updated: floor(2*4/3) + 1 = 3
        assert_eq!(mgr.current.threshold, 3);
    }

    // ─── Propose leave → threshold approvals → member removed ───────────────

    #[test]
    fn propose_leave_threshold_approvals_member_removed() {
        let participants = make_participants(4);
        let mut mgr = ConstitutionManager::from_participants(participants.clone(), 1000);

        // threshold for 4 = 3
        assert_eq!(mgr.threshold(), 3);

        let leaving_node = make_node_key(4);
        let proposal = MembershipProposal::Leave {
            node_key: leaving_node,
            reason: LeaveReason::Voluntary,
        };
        let proposal_block = BlockId([0xBB; 32]);
        mgr.submit_proposal(proposal_block, proposal);

        let vote = MembershipVote {
            proposal_block,
            approve: true,
        };

        mgr.submit_vote(&vote, make_node_key(1));
        mgr.submit_vote(&vote, make_node_key(2));
        let result = mgr.submit_vote(&vote, make_node_key(3));
        assert_eq!(result, Some(proposal_block));

        assert!(mgr.apply_if_passed(&proposal_block));
        assert!(!mgr.current.is_participant(&leaving_node));
        assert_eq!(mgr.current.participant_count(), 3);
        assert_eq!(mgr.current.version, 1);
    }

    // ─── H-rule: amend threshold requires max(current, new) votes ───────────

    #[test]
    fn h_rule_amend_threshold_from_2_to_3_requires_3_votes() {
        // Start with 4 participants, threshold manually set to 2.
        let mut constitution = Constitution::new(make_participants(4), 1000);
        constitution.threshold = 2; // Override default for this test

        let mut mgr = ConstitutionManager::new(constitution);
        assert_eq!(mgr.threshold(), 2);

        let proposal = MembershipProposal::AmendThreshold { new_threshold: 3 };

        // H-rule: max(2, 3) = 3 votes needed
        assert_eq!(mgr.current.required_votes_for(&proposal), 3);

        let proposal_block = BlockId([0xCC; 32]);
        mgr.submit_proposal(proposal_block, proposal);

        let vote = MembershipVote {
            proposal_block,
            approve: true,
        };

        // 2 votes not enough
        mgr.submit_vote(&vote, make_node_key(1));
        let result = mgr.submit_vote(&vote, make_node_key(2));
        assert_eq!(result, None);

        // 3rd vote passes
        let result = mgr.submit_vote(&vote, make_node_key(3));
        assert_eq!(result, Some(proposal_block));

        assert!(mgr.apply_if_passed(&proposal_block));
        assert_eq!(mgr.current.threshold, 3);
        assert_eq!(mgr.current.version, 1);
    }

    #[test]
    fn h_rule_amend_threshold_from_3_to_2_also_requires_3_votes() {
        // Start with 4 participants, threshold = 3 (default)
        let constitution = Constitution::new(make_participants(4), 1000);
        let mut mgr = ConstitutionManager::new(constitution);
        assert_eq!(mgr.threshold(), 3);

        let proposal = MembershipProposal::AmendThreshold { new_threshold: 2 };

        // H-rule: max(3, 2) = 3 votes needed (current threshold wins)
        assert_eq!(mgr.current.required_votes_for(&proposal), 3);

        let proposal_block = BlockId([0xDD; 32]);
        mgr.submit_proposal(proposal_block, proposal);

        let vote = MembershipVote {
            proposal_block,
            approve: true,
        };

        mgr.submit_vote(&vote, make_node_key(1));
        mgr.submit_vote(&vote, make_node_key(2));
        let result = mgr.submit_vote(&vote, make_node_key(3));
        assert_eq!(result, Some(proposal_block));

        assert!(mgr.apply_if_passed(&proposal_block));
        assert_eq!(mgr.current.threshold, 2);
    }

    // ─── Auto-eviction: equivocator detected → immediately removed ──────────

    #[test]
    fn auto_eviction_equivocator_immediately_removed() {
        let participants = make_participants(4);
        let mut mgr = ConstitutionManager::from_participants(participants, 1000);

        let equivocator_key = random_key();
        let equivocator_pub = equivocator_key.verifying_key().to_bytes();

        // First, add the equivocator as a participant
        mgr.current.participants.push(equivocator_pub);
        mgr.current.participants.sort();
        mgr.current.threshold = compute_threshold(mgr.current.participant_count());
        mgr.current.version += 1;

        assert!(mgr.current.is_participant(&equivocator_pub));
        let count_before = mgr.current.participant_count();

        // Create equivocation proof: two blocks at same seq with different content
        let block_a = Block::new(
            &equivocator_key,
            1,
            Payload::Data(b"version A".to_vec()),
            vec![],
        );
        let block_b = Block::new(
            &equivocator_key,
            1,
            Payload::Data(b"version B".to_vec()),
            vec![],
        );

        let proof = EquivocationProof {
            creator: equivocator_pub,
            block_a,
            block_b,
        };

        // Auto-evict: no voting needed
        assert!(mgr.auto_evict(&proof));
        assert!(!mgr.current.is_participant(&equivocator_pub));
        assert_eq!(mgr.current.participant_count(), count_before - 1);
    }

    #[test]
    fn auto_eviction_non_participant_returns_false() {
        let participants = make_participants(3);
        let mut mgr = ConstitutionManager::from_participants(participants, 1000);

        let non_member_key = random_key();
        let non_member_pub = non_member_key.verifying_key().to_bytes();

        let block_a = Block::new(&non_member_key, 1, Payload::Data(b"A".to_vec()), vec![]);
        let block_b = Block::new(&non_member_key, 1, Payload::Data(b"B".to_vec()), vec![]);

        let proof = EquivocationProof {
            creator: non_member_pub,
            block_a,
            block_b,
        };

        assert!(!mgr.auto_evict(&proof));
    }

    // ─── Constitution versioning ────────────────────────────────────────────

    #[test]
    fn constitution_versioning_history_preserved() {
        let participants = make_participants(3);
        let mut mgr = ConstitutionManager::from_participants(participants.clone(), 1000);

        // Version 0: initial 3 participants
        assert_eq!(mgr.version(), 0);
        let v0 = mgr.constitution_at_version(0).unwrap().clone();
        assert_eq!(v0.participant_count(), 3);

        // Add a member (simulating full flow)
        let new_node = make_node_key(4);
        let proposal = MembershipProposal::Join {
            node_key: new_node,
            justification: vec![],
        };
        let proposal_block = BlockId([0xEE; 32]);
        mgr.submit_proposal(proposal_block, proposal);

        let vote = MembershipVote {
            proposal_block,
            approve: true,
        };
        mgr.submit_vote(&vote, make_node_key(1));
        mgr.submit_vote(&vote, make_node_key(2));
        mgr.submit_vote(&vote, make_node_key(3));
        mgr.apply_if_passed(&proposal_block);

        // Version 1: 4 participants
        assert_eq!(mgr.version(), 1);
        let v1 = mgr.constitution_at_version(1).unwrap();
        assert_eq!(v1.participant_count(), 4);

        // Old version still accessible
        let v0_again = mgr.constitution_at_version(0).unwrap();
        assert_eq!(v0_again.participant_count(), 3);
    }

    // ─── Non-participants cannot vote ───────────────────────────────────────

    #[test]
    fn non_participant_vote_ignored() {
        let participants = make_participants(3);
        let mut mgr = ConstitutionManager::from_participants(participants, 1000);

        let proposal = MembershipProposal::Join {
            node_key: make_node_key(4),
            justification: vec![],
        };
        let proposal_block = BlockId([0xFF; 32]);
        mgr.submit_proposal(proposal_block, proposal);

        // Non-participant tries to vote
        let vote = MembershipVote {
            proposal_block,
            approve: true,
        };
        let result = mgr.submit_vote(&vote, make_node_key(99));
        assert_eq!(result, None);
        assert_eq!(mgr.votes.approval_count(&proposal_block), 0);
    }

    // ─── Double-application prevention ──────────────────────────────────────

    #[test]
    fn proposal_cannot_be_applied_twice() {
        let participants = make_participants(3);
        let mut mgr = ConstitutionManager::from_participants(participants, 1000);

        let proposal = MembershipProposal::Join {
            node_key: make_node_key(4),
            justification: vec![],
        };
        let proposal_block = BlockId([0x11; 32]);
        mgr.submit_proposal(proposal_block, proposal);

        let vote = MembershipVote {
            proposal_block,
            approve: true,
        };
        mgr.submit_vote(&vote, make_node_key(1));
        mgr.submit_vote(&vote, make_node_key(2));
        mgr.submit_vote(&vote, make_node_key(3));

        // First application succeeds.
        assert!(mgr.apply_if_passed(&proposal_block));
        assert_eq!(mgr.current.participant_count(), 4);

        // Second application fails (already applied).
        assert!(!mgr.apply_if_passed(&proposal_block));
        assert_eq!(mgr.current.participant_count(), 4); // unchanged
    }

    // ─── Duplicate vote ignored ─────────────────────────────────────────────

    #[test]
    fn duplicate_vote_from_same_participant_counted_once() {
        let participants = make_participants(3);
        let mut mgr = ConstitutionManager::from_participants(participants, 1000);

        let proposal = MembershipProposal::Join {
            node_key: make_node_key(4),
            justification: vec![],
        };
        let proposal_block = BlockId([0x22; 32]);
        mgr.submit_proposal(proposal_block, proposal);

        let vote = MembershipVote {
            proposal_block,
            approve: true,
        };

        // Same participant votes multiple times
        mgr.submit_vote(&vote, make_node_key(1));
        mgr.submit_vote(&vote, make_node_key(1));
        mgr.submit_vote(&vote, make_node_key(1));

        // Only counted once
        assert_eq!(mgr.votes.approval_count(&proposal_block), 1);
    }

    // ─── Integration: membership change with wave boundary ──────────────────

    #[test]
    fn membership_change_updates_participant_list_for_ordering() {
        let participants = make_participants(3);
        let mut mgr = ConstitutionManager::from_participants(participants.clone(), 1000);

        // Verify initial state matches what ordering uses
        assert_eq!(mgr.participants().len(), 3);

        // After adding a member, ordering should use new list
        let new_node = make_node_key(4);
        let proposal = MembershipProposal::Join {
            node_key: new_node,
            justification: vec![],
        };
        let proposal_block = BlockId([0x33; 32]);
        mgr.submit_proposal(proposal_block, proposal);

        let vote = MembershipVote {
            proposal_block,
            approve: true,
        };
        mgr.submit_vote(&vote, make_node_key(1));
        mgr.submit_vote(&vote, make_node_key(2));
        mgr.submit_vote(&vote, make_node_key(3));
        mgr.apply_if_passed(&proposal_block);

        // Now ordering should use 4 participants
        assert_eq!(mgr.participants().len(), 4);
        assert!(mgr.participants().contains(&new_node));

        // Wave leader computation uses the new set
        let leader = crate::ordering::wave_leader(0, mgr.participants());
        assert!(mgr.current.is_participant(&leader));
    }
}
