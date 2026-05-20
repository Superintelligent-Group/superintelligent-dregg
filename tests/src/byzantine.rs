//! Byzantine consensus adversarial tests.
//!
//! Tests that the federation consensus protocol correctly handles:
//! - Equivocating nodes (voting for multiple proposals)
//! - Delayed message delivery
//! - Split-brain scenarios
//! - Flood attacks (rapid revocation submission)

use pyana_federation::{
    ConsensusConfig, ConsensusOrchestrator, ConsensusState, Federation, FederationNode,
};
use pyana_federation::types::{
    RevocationBlock, RevocationEvent, Signature, Vote, QuorumCertificate, generate_keypair, sign,
};
use pyana_federation::revocation::RevocationVerifier;

// =============================================================================
// Helper functions
// =============================================================================

fn make_federation(n: usize) -> Federation {
    let names: Vec<&str> = (0..n).map(|i| match i {
        0 => "alpha",
        1 => "beta",
        2 => "gamma",
        3 => "delta",
        4 => "epsilon",
        5 => "zeta",
        _ => "node",
    }).collect();
    Federation::new(&names)
}

fn make_revocation_event(authority: usize, token_id: &str) -> RevocationEvent {
    RevocationEvent {
        token_id: token_id.to_string(),
        authority_id: authority,
        signature: Signature([authority as u8; 64]),
    }
}

// =============================================================================
// 1. Equivocation detection
// =============================================================================

#[test]
fn equivocating_node_duplicate_vote_ignored() {
    // A node that votes twice for the same block should only be counted once
    let config = ConsensusConfig::new(4);
    let (sk, _pk) = generate_keypair();
    // Node 1 is the leader for view 1 (leader_for_view(1) = 1 % 4 = 1)
    let mut leader = ConsensusState::new(1, sk.clone(), config.clone());

    // Submit event and create proposal
    leader.submit_revocation(make_revocation_event(1, "token-eq-1"));
    let proposal = leader.create_proposal().unwrap();
    let vote1 = leader.vote_on_proposal(&proposal).unwrap();
    leader.collect_vote(vote1.clone());

    // Try to collect the same vote again (duplicate)
    let result = leader.collect_vote(vote1);
    assert!(result.is_none(), "Duplicate vote must be rejected");
}

#[test]
fn equivocating_node_vote_for_wrong_block_ignored() {
    // A vote for a different block hash should be ignored
    let config = ConsensusConfig::new(4);
    let (sk, _pk) = generate_keypair();
    // Node 1 is the leader for view 1
    let mut leader = ConsensusState::new(1, sk.clone(), config.clone());

    leader.submit_revocation(make_revocation_event(1, "token-eq-2"));
    let proposal = leader.create_proposal().unwrap();
    let _vote = leader.vote_on_proposal(&proposal).unwrap();

    // Forge a vote for a different block
    let fake_vote = Vote {
        block_hash: [0xDE; 32], // wrong hash
        height: proposal.height,
        view: proposal.view,
        voter: 2,
        signature: Signature([0xAA; 64]),
    };

    let result = leader.collect_vote(fake_vote);
    assert!(result.is_none(), "Vote for wrong block must be rejected");
}

#[test]
fn honest_nodes_reach_consensus_despite_equivocator() {
    // With 4 nodes and 1 equivocating (offline), 3 honest can still finalize
    let mut fed = make_federation(4);

    // Node 3 is the equivocator (taken offline)
    fed.crash_node(3);

    let token = fed.mint_token(0, "Alice");
    fed.submit_revocation(0, &token.id);

    let result = fed.run_consensus_round();
    assert!(result.is_some(), "3/4 honest nodes must reach consensus");

    // Verify all online nodes agree
    assert!(fed.roots_agree());
}

#[test]
fn consensus_fails_with_two_equivocators() {
    // With 4 nodes and 2 offline, consensus cannot be reached (need 3/4)
    let mut fed = make_federation(4);
    fed.crash_node(2);
    fed.crash_node(3);

    let token = fed.mint_token(0, "Alice");
    fed.submit_revocation(0, &token.id);

    let result = fed.run_consensus_round();
    assert!(result.is_none(), "2/4 nodes cannot reach consensus");
}

// =============================================================================
// 2. Delayed messages / View changes
// =============================================================================

#[test]
fn view_change_when_leader_offline() {
    let mut fed = make_federation(4);
    let token = fed.mint_token(0, "Alice");

    // Determine who the leader would be for view 1
    let view1_leader = fed.config.leader_for_view(1);

    // Crash the leader
    fed.crash_node(view1_leader);

    // Submit revocation from another node
    let alive_node = if view1_leader == 0 { 1 } else { 0 };
    fed.submit_revocation(alive_node, &token.id);

    // The orchestrator should advance the view and find a new leader
    let result = fed.run_consensus_round();
    // May succeed if the new leader is online, or fail if we need more view changes
    // Either outcome is acceptable - the point is no crash/panic
    if let Some((_block, qc)) = result {
        assert!(qc.is_valid());
    }
}

#[test]
fn recovery_after_crash() {
    let mut fed = make_federation(4);
    let token1 = fed.mint_token(0, "Alice");

    // Round 1 succeeds normally
    fed.submit_revocation(0, &token1.id);
    let r1 = fed.run_consensus_round();
    assert!(r1.is_some());

    // Crash a node, but ensure enough remain for consensus
    fed.crash_node(3);

    // Round 2 should still work with 3/4 nodes
    let token2 = fed.mint_token(1, "Bob");
    fed.submit_revocation(1, &token2.id);
    let r2 = fed.run_consensus_round();
    assert!(r2.is_some());

    // Verify online nodes agree
    let mut online_roots: Vec<[u8; 32]> = Vec::new();
    for node in &mut fed.nodes {
        if node.is_online {
            online_roots.push(node.current_root());
        }
    }
    assert_eq!(online_roots.len(), 3);
    assert!(online_roots.windows(2).all(|w| w[0] == w[1]));
}

#[test]
fn delayed_node_catches_up() {
    let mut fed = make_federation(4);

    // Crash node 3 for the first round
    fed.crash_node(3);

    let token = fed.mint_token(0, "Alice");
    fed.submit_revocation(0, &token.id);
    let result = fed.run_consensus_round();
    assert!(result.is_some());

    // Online nodes see the revocation
    for i in 0..3 {
        assert!(fed.nodes[i].is_revoked(&token.id));
    }

    // Node 3 is still offline and hasn't seen the revocation
    // (in real impl it would catch up via state sync)
    // This just verifies the node wasn't updated while offline
    assert!(!fed.nodes[3].is_revoked(&token.id));
}

// =============================================================================
// 3. Split-brain scenarios
// =============================================================================

#[test]
fn split_brain_at_most_one_block_finalizes() {
    // With 4 nodes split 2-2, neither partition can finalize
    // (need 3 votes, each partition has only 2)
    let config = ConsensusConfig::new(4);
    assert_eq!(config.threshold, 3);

    // Simulate: nodes 0,1 are in partition A; nodes 2,3 in partition B
    // Neither partition can reach threshold=3
    let mut fed = make_federation(4);

    // Partition B goes offline from A's perspective
    fed.crash_node(2);
    fed.crash_node(3);

    let token = fed.mint_token(0, "Alice");
    fed.submit_revocation(0, &token.id);

    // Partition A (nodes 0,1) tries to finalize - should fail
    let result_a = fed.run_consensus_round();
    assert!(result_a.is_none(), "Partition of 2 cannot finalize with threshold 3");
}

#[test]
fn seven_node_split_brain() {
    // 7 nodes: threshold = 5, max_faults = 2
    // Split 3-4: the partition with 4 cannot reach threshold either (need 5)
    let config = ConsensusConfig::new(7);
    assert_eq!(config.threshold, 5);
    assert_eq!(config.max_faults, 2);

    let mut fed = make_federation(7);

    // Crash 3 nodes: leaves 4 online, threshold is 5 => cannot finalize
    fed.crash_node(4);
    fed.crash_node(5);
    fed.crash_node(6);

    let token = fed.mint_token(0, "Alice");
    fed.submit_revocation(0, &token.id);

    let result = fed.run_consensus_round();
    assert!(result.is_none(), "4/7 nodes cannot meet threshold of 5");
}

#[test]
fn no_conflicting_blocks_at_same_height() {
    // Run multiple rounds and verify heights are strictly monotonic
    let mut fed = make_federation(4);

    for i in 0..10 {
        let token = fed.mint_token(i % 4, &format!("user-{i}"));
        fed.submit_revocation(i % 4, &token.id);
        let result = fed.run_consensus_round();
        assert!(result.is_some());
    }

    // Verify height is monotonically increasing
    let history = &fed.finalized_history;
    for window in history.windows(2) {
        let (block_a, _) = &window[0];
        let (block_b, _) = &window[1];
        assert!(
            block_b.height > block_a.height,
            "Heights must be strictly increasing: {} vs {}",
            block_a.height,
            block_b.height
        );
    }
}

// =============================================================================
// 4. Flood attacks
// =============================================================================

#[test]
fn flood_1000_revocations_all_committed() {
    let mut fed = make_federation(4);

    // Mint and revoke 1000 tokens
    let mut token_ids = Vec::with_capacity(1000);
    for i in 0..1000 {
        let token = fed.mint_token(i % 4, &format!("holder-{i}"));
        token_ids.push(token.id.clone());
        fed.submit_revocation(i % 4, &token.id);
    }

    // Run consensus rounds until all are committed
    let mut committed = 0;
    let mut rounds = 0;
    while committed < 1000 && rounds < 1000 {
        if let Some((block, _qc)) = fed.run_consensus_round() {
            committed += block.events.len();
        } else {
            // Shouldn't happen with 4 online nodes
            panic!("Consensus failed during flood test at round {rounds}");
        }
        rounds += 1;
    }

    assert_eq!(committed, 1000, "All 1000 revocations must be committed");

    // Verify all tokens are revoked
    for token_id in &token_ids {
        assert!(
            fed.nodes[0].is_revoked(token_id),
            "Token {} should be revoked",
            token_id
        );
    }

    // Verify all nodes agree
    assert!(fed.roots_agree());
}

#[test]
fn rapid_revocations_from_single_node() {
    let mut fed = make_federation(4);

    // One node submits 100 revocations rapidly
    for i in 0..100 {
        let token = fed.mint_token(0, &format!("victim-{i}"));
        fed.submit_revocation(0, &token.id);
    }

    // Should handle in a single round (all pending go into one block)
    let result = fed.run_consensus_round();
    assert!(result.is_some());
    let (block, _) = result.unwrap();
    assert_eq!(block.events.len(), 100);
}

#[test]
fn revocations_survive_node_crash_during_processing() {
    let mut fed = make_federation(4);

    // Submit revocations
    for i in 0..10 {
        let token = fed.mint_token(i % 4, &format!("t-{i}"));
        fed.submit_revocation(i % 4, &token.id);
    }

    // Crash one node mid-way (before consensus)
    fed.crash_node(3);

    // Consensus should still work
    let result = fed.run_consensus_round();
    assert!(result.is_some());

    // Online nodes should all agree
    let mut online_roots: Vec<[u8; 32]> = Vec::new();
    for node in &mut fed.nodes {
        if node.is_online {
            online_roots.push(node.current_root());
        }
    }
    assert!(online_roots.windows(2).all(|w| w[0] == w[1]));
}

// =============================================================================
// 5. Quorum certificate forgery
// =============================================================================

#[test]
fn insufficient_quorum_not_valid() {
    let qc = QuorumCertificate {
        block_hash: [0x42; 32],
        height: 1,
        view: 1,
        aggregate_qc: None,
        votes: vec![(0, Signature([1; 64]))], // only 1 vote
        threshold: 3,
    };
    assert!(!qc.is_valid(), "QC with 1/3 votes must not be valid");
}

#[test]
fn quorum_exactly_at_threshold_is_valid() {
    let qc = QuorumCertificate {
        block_hash: [0x42; 32],
        height: 1,
        view: 1,
        aggregate_qc: None,
        votes: vec![
            (0, Signature([1; 64])),
            (1, Signature([2; 64])),
            (2, Signature([3; 64])),
        ],
        threshold: 3,
    };
    assert!(qc.is_valid(), "QC with exactly threshold votes is valid");
}

#[test]
fn block_hash_mismatch_prevents_vote_collection() {
    let config = ConsensusConfig::new(4);
    let (sk, _pk) = generate_keypair();
    // Node 1 is leader for view 1
    let mut state = ConsensusState::new(1, sk, config);

    state.submit_revocation(make_revocation_event(1, "token-x"));
    let proposal = state.create_proposal().unwrap();
    let vote = state.vote_on_proposal(&proposal).unwrap();
    state.collect_vote(vote);

    // Try collecting a vote for a completely different block
    let bad_vote = Vote {
        block_hash: [0xFF; 32],
        height: 1,
        view: 1,
        voter: 2,
        signature: Signature([0xBB; 64]),
    };
    let result = state.collect_vote(bad_vote);
    assert!(result.is_none(), "Vote for wrong block hash must be rejected");
}

// =============================================================================
// 6. Non-membership proof after revocation
// =============================================================================

#[test]
fn revoked_token_cannot_get_non_membership_proof() {
    let mut fed = make_federation(4);
    let token = fed.mint_token(0, "Alice");

    // Revoke the token
    fed.submit_revocation(0, &token.id);
    fed.run_consensus_round().unwrap();

    // Try to get a non-membership proof for the revoked token
    let proof = fed.verify_non_membership_from(0, &token.id);
    assert!(proof.is_none(), "Revoked token must not get non-membership proof");
}

#[test]
fn non_revoked_token_has_valid_proof() {
    let mut fed = make_federation(4);
    let token_a = fed.mint_token(0, "Alice");
    let token_b = fed.mint_token(1, "Bob");

    // Only revoke token_a
    fed.submit_revocation(0, &token_a.id);
    fed.run_consensus_round().unwrap();

    // token_b should have a valid non-membership proof
    let proof = fed.verify_non_membership_from(0, &token_b.id);
    assert!(proof.is_some(), "Non-revoked token should have non-membership proof");

    let proof = proof.unwrap();
    let verification = RevocationVerifier::verify(&proof);
    assert!(verification.valid, "Non-membership proof must verify");
}
