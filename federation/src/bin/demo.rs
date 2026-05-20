//! PYANA Multi-Node Federation Demo
//!
//! Spawns N federation nodes (in-process) and demonstrates:
//! 1. Federation setup with consensus parameters
//! 2. Token minting across authorities
//! 3. Revocation via consensus
//! 4. Non-membership verification with attested roots
//! 5. Byzantine fault tolerance

use pyana_federation::node::Federation;
use pyana_federation::revocation::RevocationVerifier;
use pyana_federation::types::hex_encode;

fn main() {
    print_header();

    // Step 1: Spawn federation nodes.
    let mut fed = step_1_spawn_nodes();

    // Step 2: Issue tokens.
    let (t1, t2) = step_2_issue_tokens(&mut fed);

    // Step 3: Revoke T1 via consensus.
    step_3_revoke_token(&mut fed, &t1.id);

    // Step 4: Verification with attested root.
    step_4_verify_tokens(&fed, &t1.id, &t2.id);

    // Step 5: Byzantine fault tolerance.
    step_5_byzantine_fault(&mut fed, &t2.id);

    print_footer(&fed);
}

// =============================================================================
// Step 1: Spawn Federation Nodes
// =============================================================================

fn step_1_spawn_nodes() -> Federation {
    print_step(1, 5, "Spawning 4 federation nodes...");

    let fed = Federation::new(&["alpha.org", "beta.corp", "gamma.edu", "delta.gov"]);

    for node in &fed.nodes {
        println!(
            "  {} Node {} \"{}\" ready (pubkey: {})",
            arrow(),
            node.identity.id,
            node.identity.name,
            node.identity.public_key.short_hex()
        );
    }
    println!(
        "  {} Consensus: {} nodes, threshold {} (tolerates {} Byzantine)",
        arrow(),
        fed.config.num_nodes,
        fed.config.threshold,
        fed.config.max_faults
    );
    println!();

    fed
}

// =============================================================================
// Step 2: Issue Tokens
// =============================================================================

#[allow(dead_code)]
struct MintedToken {
    id: String,
    holder: String,
    issuer_name: String,
}

fn step_2_issue_tokens(fed: &mut Federation) -> (MintedToken, MintedToken) {
    print_step(2, 5, "Normal operation -- issuing tokens...");

    let token1 = fed.mint_token(0, "engineer Alice");
    let token2 = fed.mint_token(1, "contractor Bob");

    println!(
        "  {} alpha.org mints token T1 for engineer Alice (id: {}...)",
        arrow(),
        &token1.id[..8]
    );
    println!(
        "  {} beta.corp mints token T2 for contractor Bob (id: {}...)",
        arrow(),
        &token2.id[..8]
    );

    // Show current state: 0 revocations.
    // All nodes start with the same empty revocation tree root.
    let root_display = fed.nodes[0].revocation_tree.root();
    println!(
        "  {} All nodes see: 0 revocations, root = {}...",
        arrow(),
        hex_encode(&root_display[..4])
    );
    println!();

    let t1 = MintedToken {
        id: token1.id,
        holder: "engineer Alice".to_string(),
        issuer_name: "alpha.org".to_string(),
    };
    let t2 = MintedToken {
        id: token2.id,
        holder: "contractor Bob".to_string(),
        issuer_name: "beta.corp".to_string(),
    };

    (t1, t2)
}

// =============================================================================
// Step 3: Revocation via Consensus
// =============================================================================

fn step_3_revoke_token(fed: &mut Federation, token_id: &str) {
    print_step(3, 5, "Revocation -- alpha.org revokes T1...");

    println!(
        "  {} alpha.org submits Revoke(T1) to consensus",
        arrow()
    );

    // Submit the revocation.
    fed.submit_revocation(0, token_id);

    // Run consensus.
    let result = fed.run_consensus_round();

    match result {
        Some((block, qc)) => {
            let voters: Vec<String> = qc
                .votes
                .iter()
                .map(|(id, _)| format!("{}", id))
                .collect();
            let _ = voters;

            println!(
                "  {} Morpheus: block finalized in view {} ({}/{} nodes voted)",
                arrow(),
                block.view,
                qc.votes.len(),
                fed.config.num_nodes
            );

            // Show the new attested root.
            let attested = fed.nodes[0].get_attested_root().unwrap();
            println!(
                "  {} New attested root: {}... (height={}, {} signatures)",
                arrow(),
                hex_encode(&attested.merkle_root[..4]),
                attested.height,
                attested.quorum_signatures.len()
            );

            // Verify all online nodes updated their tree.
            assert!(fed.roots_agree(), "BUG: nodes disagree on root after consensus");
            println!(
                "  {} All nodes updated their Merkle tree",
                arrow()
            );
        }
        None => {
            println!(
                "  {} ERROR: consensus failed (not enough nodes online)",
                arrow()
            );
        }
    }

    println!();
}

// =============================================================================
// Step 4: Verification with Attested Root
// =============================================================================

fn step_4_verify_tokens(fed: &Federation, revoked_id: &str, valid_id: &str) {
    print_step(4, 5, "Verification with attested root...");

    // gamma.edu (node 2) verifies T2 (Bob's token).
    println!(
        "  {} gamma.edu verifies T2 (Bob's token): ",
        arrow()
    );

    let proof_t2 = fed.verify_non_membership_from(2, valid_id);
    match proof_t2 {
        Some(proof) => {
            let verification = RevocationVerifier::verify(&proof);
            if verification.valid {
                println!(
                    "      VALID {}",
                    checkmark()
                );
                println!(
                    "      (non-membership proof for T2 against root {}...)",
                    hex_encode(&proof.attested_root.merkle_root[..4])
                );
            } else {
                println!(
                    "      INVALID {} ({})",
                    cross(),
                    verification.reason
                );
            }
        }
        None => {
            println!(
                "      REVOKED {} (T2 IS in the revocation tree)",
                cross()
            );
        }
    }

    // gamma.edu verifies T1 (Alice's revoked token).
    println!(
        "  {} gamma.edu verifies T1 (Alice's revoked token): ",
        arrow()
    );

    let proof_t1 = fed.verify_non_membership_from(2, revoked_id);
    match proof_t1 {
        Some(proof) => {
            let verification = RevocationVerifier::verify(&proof);
            if verification.valid {
                println!(
                    "      VALID {} (unexpected!)",
                    checkmark()
                );
            } else {
                println!(
                    "      REVOKED {} ({})",
                    cross(),
                    verification.reason
                );
            }
        }
        None => {
            println!(
                "      REVOKED {}",
                cross()
            );
            println!(
                "      (T1 IS in the revocation tree -- cannot prove non-membership)"
            );
        }
    }

    println!();
}

// =============================================================================
// Step 5: Byzantine Fault Tolerance
// =============================================================================

fn step_5_byzantine_fault(fed: &mut Federation, token_id: &str) {
    print_step(5, 5, "Byzantine fault tolerance...");

    // Crash delta.gov (node 3).
    println!(
        "  {} delta.gov goes offline (simulated crash)",
        arrow()
    );
    fed.crash_node(3);

    // Submit another revocation.
    println!(
        "  {} alpha.org submits Revoke(T2)",
        arrow()
    );
    fed.submit_revocation(0, token_id);

    // Run consensus with a crashed node.
    let result = fed.run_consensus_round();

    match result {
        Some((block, qc)) => {
            println!(
                "  {} Morpheus: block finalized with {}/{} nodes (1 offline tolerated)",
                arrow(),
                qc.votes.len(),
                fed.config.num_nodes
            );

            let attested = fed.nodes[0].get_attested_root().unwrap();
            println!(
                "  {} New attested root: {}... (height={})",
                arrow(),
                hex_encode(&attested.merkle_root[..4]),
                attested.height
            );

            // Verify both tokens are now revoked.
            println!(
                "  {} Verification: both T1 and T2 now revoked",
                arrow()
            );

            // Count revocations.
            let revocation_count = fed.nodes[0].revocation_tree.len();
            println!(
                "  {} Revocation tree size: {} tokens revoked",
                arrow(),
                revocation_count
            );

            // Ensure the events in the block are correct.
            assert_eq!(block.events.len(), 1);
            assert_eq!(block.events[0].token_id, token_id);
        }
        None => {
            println!(
                "  {} ERROR: consensus failed even with 1 fault tolerance",
                arrow()
            );
            println!(
                "  {} (this would indicate a bug in the demo)",
                arrow()
            );
        }
    }

    println!();
}

// =============================================================================
// Display Helpers
// =============================================================================

fn print_header() {
    println!();
    println!(
        "\x1b[1m{}\x1b[0m",
        "==============================================================="
    );
    println!(
        "\x1b[1m  PYANA MULTI-NODE FEDERATION DEMO\x1b[0m"
    );
    println!(
        "\x1b[1m{}\x1b[0m",
        "==============================================================="
    );
    println!();
}

fn print_footer(fed: &Federation) {
    let total_revocations = fed.nodes[0].revocation_tree.len();
    let total_blocks = fed.finalized_history.len();

    println!(
        "\x1b[1m{}\x1b[0m",
        "==============================================================="
    );
    println!(
        "\x1b[1m  Federation consensus demo complete.\x1b[0m"
    );
    println!(
        "\x1b[1m  {} nodes, {} revocations, {} blocks finalized,\x1b[0m",
        fed.config.num_nodes, total_revocations, total_blocks
    );
    println!(
        "\x1b[1m  {} Byzantine fault tolerated.\x1b[0m",
        fed.config.max_faults
    );
    println!(
        "\x1b[1m{}\x1b[0m",
        "==============================================================="
    );
    println!();
}

fn print_step(step: u32, total: u32, description: &str) {
    println!(
        "\x1b[1;36m[{}/{}]\x1b[0m \x1b[1m{}\x1b[0m",
        step, total, description
    );
}

fn arrow() -> &'static str {
    "\x1b[33m->\x1b[0m"
}

fn checkmark() -> &'static str {
    "\x1b[32m[VALID]\x1b[0m"
}

fn cross() -> &'static str {
    "\x1b[31m[REVOKED]\x1b[0m"
}
