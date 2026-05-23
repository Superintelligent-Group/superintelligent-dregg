//! Revocation list and non-revocation proof generation.
//!
//! When an issuer revokes a credential, its revocation hash is added to a
//! sorted Merkle tree. Holders must provide non-revocation proofs (proving
//! their credential's hash is NOT in the tree) when presenting to verifiers.
//!
//! Uses the DSL-based non-revocation circuit (30-bit range checks, sound).

use pyana_circuit::field::BabyBear;
use pyana_circuit::stark::{self, StarkProof};
use pyana_dsl_tests::non_revocation_dsl::{
    DslRevocationTree, generate_non_revocation_trace, non_revocation_dsl_circuit,
};

/// Re-export the tree depth constant.
pub use pyana_dsl_tests::non_revocation_dsl::TREE_DEPTH as REVOCATION_TREE_DEPTH;

/// A non-revocation proof: demonstrates that a credential has not been revoked.
#[derive(Clone, Debug)]
pub struct NonRevocationProof {
    /// The revocation tree root this proof is against.
    pub revocation_root: BabyBear,
    /// Whether the proof is valid (credential is NOT revoked).
    pub is_valid: bool,
}

/// Manage a revocation list and generate proofs.
pub struct RevocationManager {
    /// Current revocation hashes.
    revoked_hashes: Vec<BabyBear>,
    /// The DSL-compatible sorted revocation tree.
    tree: DslRevocationTree,
    /// Tree depth.
    depth: usize,
}

impl RevocationManager {
    /// Create a new empty revocation manager.
    pub fn new(depth: usize) -> Self {
        Self {
            revoked_hashes: Vec::new(),
            tree: DslRevocationTree::new(Vec::new(), depth),
            depth,
        }
    }

    /// Create from existing revocation hashes.
    pub fn from_hashes(hashes: Vec<BabyBear>, depth: usize) -> Self {
        let tree = DslRevocationTree::new(hashes.clone(), depth);
        Self {
            revoked_hashes: hashes,
            tree,
            depth,
        }
    }

    /// Add a revocation hash (revoke a credential).
    pub fn revoke(&mut self, hash: BabyBear) {
        if !self.revoked_hashes.contains(&hash) {
            self.revoked_hashes.push(hash);
            self.tree = DslRevocationTree::new(self.revoked_hashes.clone(), self.depth);
        }
    }

    /// Check if a hash is revoked.
    pub fn is_revoked(&self, hash: &BabyBear) -> bool {
        self.tree.contains(hash)
    }

    /// Get the current revocation tree root.
    pub fn root(&self) -> BabyBear {
        self.tree.root()
    }

    /// Get a reference to the underlying DSL tree.
    pub fn tree(&self) -> &DslRevocationTree {
        &self.tree
    }

    /// Generate a non-revocation proof for a credential hash.
    ///
    /// Returns a STARK proof that the given hash is NOT in the revocation tree.
    /// Returns None if the hash IS revoked (proof is impossible).
    pub fn prove_non_revocation(&self, credential_hash: BabyBear) -> Option<StarkProof> {
        let witness = self.tree.prove_non_membership(&credential_hash)?;
        let root = self.tree.root();
        let (trace, public_inputs) = generate_non_revocation_trace(&witness, root);
        let circuit = non_revocation_dsl_circuit();
        Some(stark::prove(&circuit, &trace, &public_inputs))
    }

    /// Verify a non-revocation proof against the current root.
    pub fn verify_proof(&self, proof: &StarkProof) -> bool {
        let circuit = non_revocation_dsl_circuit();
        let pi = vec![self.root()];
        stark::verify(&circuit, proof, &pi).is_ok()
    }

    /// Number of revoked credentials.
    pub fn num_revoked(&self) -> usize {
        self.revoked_hashes.len()
    }
}
