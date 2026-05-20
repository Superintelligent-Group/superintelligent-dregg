//! Federation Bootstrap Demo
//!
//! Demonstrates:
//! 1. Generate 3 Ed25519 keypairs for federation nodes
//! 2. Create initial AttestedRoot with genesis state
//! 3. Each node signs the root, forming a quorum certificate
//! 4. Epoch reconfiguration: add a 4th node, remove a node
//! 5. Verify the attested root is verifiable by external parties

use pyana_federation::{
    ConsensusConfig, ConsensusOrchestrator, ConsensusState,
    Federation, ReconfigurationProposal,
    generate_keypair, sign,
};
use pyana_federation::types::{
    AttestedRoot, PublicKey, RevocationEvent, Signature,
};

fn short_hex(bytes: &[u8]) -> String {
    bytes[..4].iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    println!("=== Pyana Federation Bootstrap Demo ===\n");

    // =========================================================================
    // STEP 1: Generate 3 Ed25519 keypairs for federation nodes
    // =========================================================================
    println!("--- Step 1: GENERATE FEDERATION KEYPAIRS ---");

    let (sk_alpha, pk_alpha) = generate_keypair();
    let (sk_beta, pk_beta) = generate_keypair();
    let (sk_gamma, pk_gamma) = generate_keypair();

    println!("  Node alpha: pubkey = {}", short_hex(&pk_alpha.0));
    println!("  Node beta:  pubkey = {}", short_hex(&pk_beta.0));
    println!("  Node gamma: pubkey = {}", short_hex(&pk_gamma.0));
    println!();

    let initial_members = vec![pk_alpha, pk_beta, pk_gamma];

    // =========================================================================
    // STEP 2: Create initial AttestedRoot with genesis state
    // =========================================================================
    println!("--- Step 2: CREATE GENESIS ATTESTED ROOT ---");

    // The genesis Merkle root is the empty revocation tree (all zeros).
    let genesis_merkle_root = [0u8; 32];
    let genesis_height = 0u64;
    let genesis_timestamp = 1700000000i64;

    // Build the attested root structure (unsigned initially).
    let mut genesis_root = AttestedRoot {
        merkle_root: genesis_merkle_root,
        height: genesis_height,
        timestamp: genesis_timestamp,
        qc: None,
        quorum_signatures: Vec::new(),
        threshold: 2, // 2-of-3 quorum for genesis
    };

    println!("  Genesis root created:");
    println!("    merkle_root: {} (empty tree)", short_hex(&genesis_merkle_root));
    println!("    height: {}", genesis_height);
    println!("    timestamp: {}", genesis_timestamp);
    println!("    threshold: 2 of 3");
    println!();

    // =========================================================================
    // STEP 3: Each node signs the root, forming a quorum certificate
    // =========================================================================
    println!("--- Step 3: FORM QUORUM CERTIFICATE ---");

    // Each node signs the canonical message for this attested root.
    let signing_message = genesis_root.signing_message();

    let sig_alpha = sign(&sk_alpha, &signing_message);
    let sig_beta = sign(&sk_beta, &signing_message);
    let sig_gamma = sign(&sk_gamma, &signing_message);

    // Add signatures to form the quorum.
    genesis_root.quorum_signatures = vec![
        (pk_alpha, sig_alpha),
        (pk_beta, sig_beta),
        (pk_gamma, sig_gamma),
    ];

    // Verify: the root now has a valid quorum.
    assert!(genesis_root.has_quorum(), "Genesis root must have quorum");
    assert!(
        genesis_root.is_valid(&initial_members),
        "Genesis root must be cryptographically valid"
    );

    println!("  Signatures collected:");
    println!("    alpha: signed [VALID]");
    println!("    beta:  signed [VALID]");
    println!("    gamma: signed [VALID]");
    println!("  Quorum formed: 3/2 (exceeds threshold) [PASS]");
    println!("  Cryptographic verification: [PASS]");
    println!();

    // Demonstrate that an external party can verify this root.
    println!("  External verifier checks:");
    let external_valid = genesis_root.is_valid(&initial_members);
    println!("    is_valid(known_keys): {} [PASS]", external_valid);
    assert!(external_valid);

    // Demonstrate that unknown keys are rejected.
    let (_, unknown_pk) = generate_keypair();
    let wrong_keys = vec![unknown_pk, pk_beta, pk_gamma];
    let external_invalid = genesis_root.is_valid(&wrong_keys);
    println!("    is_valid(wrong_keys): {} [PASS - correctly rejected]", external_invalid);
    assert!(!external_invalid);
    println!();

    // =========================================================================
    // STEP 4: Epoch reconfiguration — add a 4th node
    // =========================================================================
    println!("--- Step 4: EPOCH RECONFIGURATION (add 4th node) ---");

    // Create the consensus infrastructure with explicit member tracking.
    let config = ConsensusConfig::genesis(initial_members.clone());
    let mut states: Vec<ConsensusState> = vec![
        ConsensusState::new(0, sk_alpha.clone(), config.clone()),
        ConsensusState::new(1, sk_beta.clone(), config.clone()),
        ConsensusState::new(2, sk_gamma.clone(), config.clone()),
    ];
    let mut orchestrator = ConsensusOrchestrator::new(config.clone());

    println!("  Current epoch: {}", orchestrator.config.epoch);
    println!("  Current members: {} (threshold: {})", orchestrator.config.num_nodes, orchestrator.config.threshold);

    // Generate a 4th node keypair.
    let (sk_delta, pk_delta) = generate_keypair();
    println!("  New node delta: pubkey = {}", short_hex(&pk_delta.0));

    // Propose reconfiguration: add delta to the member set.
    let mut new_members = initial_members.clone();
    new_members.push(pk_delta);

    let reconfig_msg = ReconfigurationProposal::signing_message(0, &new_members);
    let reconfig_sig = sign(&sk_alpha, &reconfig_msg);

    let proposal = ReconfigurationProposal {
        epoch: 0,
        new_members: new_members.clone(),
        proposer: pk_alpha,
        signature: reconfig_sig,
    };

    orchestrator.propose_reconfiguration(proposal).unwrap();
    println!("  Reconfiguration proposed by alpha (epoch 0 -> 1)");

    // Collect votes from beta and gamma.
    let proposal_hash = orchestrator.pending_reconfig.as_ref().unwrap().proposal_hash;
    orchestrator.vote_reconfiguration(proposal_hash, &sk_beta).unwrap();
    orchestrator.vote_reconfiguration(proposal_hash, &sk_gamma).unwrap();

    println!("  Votes: alpha (proposer) + beta + gamma = 3/{} [QUORUM]", config.threshold);
    assert!(orchestrator.reconfig_has_quorum());

    // Run a consensus round to trigger the epoch boundary.
    // Submit a dummy revocation event so the round has something to finalize.
    states[0].submit_revocation(RevocationEvent {
        token_id: "bootstrap-event".to_string(),
        authority_id: 0,
        signature: Signature([0xAA; 64]),
    });

    let round_result = orchestrator.run_round(&mut states);
    assert!(round_result.is_some(), "Consensus round should succeed");

    println!("  Consensus round finalized (block height 1)");
    println!("  Epoch advanced: {} -> {}", 0, orchestrator.config.epoch);
    println!("  New member count: {} (was 3)", orchestrator.config.num_nodes);
    println!("  New threshold: {} (BFT: f={})", orchestrator.config.threshold, orchestrator.config.max_faults);
    assert_eq!(orchestrator.config.epoch, 1);
    assert_eq!(orchestrator.config.num_nodes, 4);
    assert!(orchestrator.config.members.contains(&pk_delta));
    println!();

    // =========================================================================
    // STEP 5: Epoch reconfiguration — remove a node
    // =========================================================================
    println!("--- Step 5: EPOCH RECONFIGURATION (remove gamma) ---");

    // Add delta's consensus state to participate in the new epoch.
    let mut delta_state = ConsensusState::new(3, sk_delta.clone(), orchestrator.config.clone());
    delta_state.current_height = states[0].current_height;
    delta_state.current_view = states[0].current_view;
    delta_state.last_finalized_hash = states[0].last_finalized_hash;
    states.push(delta_state);

    // Propose removing gamma (keep alpha, beta, delta).
    let shrunk_members = vec![pk_alpha, pk_beta, pk_delta];
    let reconfig_msg2 = ReconfigurationProposal::signing_message(1, &shrunk_members);
    let reconfig_sig2 = sign(&sk_alpha, &reconfig_msg2);

    let proposal2 = ReconfigurationProposal {
        epoch: 1,
        new_members: shrunk_members.clone(),
        proposer: pk_alpha,
        signature: reconfig_sig2,
    };

    orchestrator.propose_reconfiguration(proposal2).unwrap();
    println!("  Reconfiguration proposed: remove gamma");

    // With 4 nodes, threshold is 3. Need 3 votes.
    let proposal_hash2 = orchestrator.pending_reconfig.as_ref().unwrap().proposal_hash;
    orchestrator.vote_reconfiguration(proposal_hash2, &sk_beta).unwrap();
    orchestrator.vote_reconfiguration(proposal_hash2, &sk_delta).unwrap();
    assert!(orchestrator.reconfig_has_quorum());
    println!("  Votes: alpha + beta + delta = 3/{} [QUORUM]", orchestrator.config.threshold);

    // Trigger epoch transition.
    states[0].submit_revocation(RevocationEvent {
        token_id: "remove-gamma-event".to_string(),
        authority_id: 0,
        signature: Signature([0xBB; 64]),
    });
    let round2 = orchestrator.run_round(&mut states);
    assert!(round2.is_some());

    println!("  Epoch advanced: 1 -> {}", orchestrator.config.epoch);
    println!("  Members: {} (alpha, beta, delta)", orchestrator.config.num_nodes);
    println!("  Gamma removed: {}", !orchestrator.config.members.contains(&pk_gamma));
    assert_eq!(orchestrator.config.epoch, 2);
    assert_eq!(orchestrator.config.num_nodes, 3);
    assert!(!orchestrator.config.members.contains(&pk_gamma));
    assert!(orchestrator.config.members.contains(&pk_delta));
    println!();

    // =========================================================================
    // STEP 6: Demonstrate external verification of attested roots
    // =========================================================================
    println!("--- Step 6: EXTERNAL VERIFICATION ---");

    // Use the high-level Federation API to show that attested roots
    // are verifiable by any party who knows the federation's public keys.
    let mut fed = Federation::new(&["node-a", "node-b", "node-c", "node-d"]);

    // Mint and revoke a token to produce a non-trivial attested root.
    let token = fed.mint_token(0, "Alice");
    fed.submit_revocation(0, &token.id);
    let (_, _qc) = fed.run_consensus_round().unwrap();

    // All nodes should now have an attested root.
    let attested = fed.nodes[0].get_attested_root().unwrap();
    println!("  Attested root from federation:");
    println!("    {}", attested);
    println!("    merkle_root: {}", short_hex(&attested.merkle_root));
    println!("    height: {}", attested.height);
    println!("    signatures: {}/{}", attested.quorum_signatures.len(), attested.threshold);

    // External party verifies using known federation keys.
    let federation_keys: Vec<PublicKey> = fed.nodes.iter().map(|n| n.identity.public_key).collect();
    let ext_valid = attested.is_valid_with_keys(&federation_keys);
    println!("    external verification: {} [PASS]", ext_valid);
    assert!(ext_valid);

    // All nodes agree on the same root.
    assert!(fed.roots_agree());
    println!("    all nodes agree: [PASS]");
    println!();

    // =========================================================================
    // Summary
    // =========================================================================
    println!("=== Federation Bootstrap Demo Complete ===");
    println!();
    println!("  Demonstrated:");
    println!("    [x] Ed25519 keypair generation for federation nodes");
    println!("    [x] Genesis AttestedRoot creation and quorum signing");
    println!("    [x] Cryptographic verification by external parties");
    println!("    [x] Epoch reconfiguration: adding a 4th node (3 -> 4)");
    println!("    [x] Epoch reconfiguration: removing a node (4 -> 3)");
    println!("    [x] BFT threshold adjustment across reconfigurations");
    println!("    [x] Federation roots verified by any party with public keys");
}
