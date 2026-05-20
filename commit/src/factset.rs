//! FactSet: an ordered set of facts backed by a 4-ary Merkle tree.
//!
//! The FactSet maintains a sorted collection of facts and provides:
//! - Insertion and removal with automatic root recomputation.
//! - Membership and non-membership proofs.
//! - A commitment (Merkle root) to the entire fact set.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::fact::Fact;
use crate::hash::hash_leaf;
use crate::merkle::{MerkleProof, MerkleTree, NonMembershipProof};

/// An ordered set of facts with a Merkle commitment.
///
/// Facts are sorted by their natural ordering (predicate, then terms).
/// Each fact is inserted into the Merkle tree as `H_leaf(fact.to_bytes())`.
#[derive(Clone, Debug)]
pub struct FactSet {
    /// The sorted set of facts.
    facts: BTreeSet<Fact>,
    /// The underlying Merkle tree.
    tree: MerkleTree,
}

/// A compact representation of a fact set for serialization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactSetSnapshot {
    /// All facts in sorted order.
    pub facts: Vec<Fact>,
    /// The Merkle root at the time of snapshot.
    pub root: [u8; 32],
}

impl FactSet {
    /// Create an empty fact set.
    pub fn new() -> Self {
        Self {
            facts: BTreeSet::new(),
            tree: MerkleTree::new(),
        }
    }

    /// Create a fact set from an iterator of facts.
    pub fn from_facts(facts: impl IntoIterator<Item = Fact>) -> Self {
        let mut fs = Self::new();
        for fact in facts {
            fs.insert(fact);
        }
        fs
    }

    /// Number of facts in the set.
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// Insert a fact into the set. Returns the new Merkle root.
    /// If the fact already exists, the root is unchanged.
    pub fn insert(&mut self, fact: Fact) -> [u8; 32] {
        if self.facts.insert(fact) {
            let leaf_data = fact.to_bytes();
            self.tree.insert(&leaf_data);
        }
        self.root()
    }

    /// Remove a fact from the set. Returns the new Merkle root.
    /// Returns None if the fact was not present.
    pub fn remove(&mut self, fact: &Fact) -> Option<[u8; 32]> {
        if self.facts.remove(fact) {
            let leaf_data = fact.to_bytes();
            self.tree.remove(&leaf_data);
            Some(self.root())
        } else {
            None
        }
    }

    /// Check if the set contains a fact.
    pub fn contains(&self, fact: &Fact) -> bool {
        self.facts.contains(fact)
    }

    /// Get the current Merkle root.
    pub fn root(&mut self) -> [u8; 32] {
        self.tree.root()
    }

    /// Generate a membership proof for a fact.
    /// Returns None if the fact is not in the set.
    pub fn membership_proof(&self, fact: &Fact) -> Option<MerkleProof> {
        if !self.facts.contains(fact) {
            return None;
        }
        let leaf_data = fact.to_bytes();
        self.tree.membership_proof(&leaf_data)
    }

    /// Generate a non-membership proof for a fact.
    /// Returns None if the fact IS in the set.
    pub fn non_membership_proof(&self, fact: &Fact) -> Option<NonMembershipProof> {
        if self.facts.contains(fact) {
            return None;
        }
        let leaf_data = fact.to_bytes();
        self.tree.non_membership_proof(&leaf_data)
    }

    /// Verify a membership proof against a root.
    pub fn verify_membership(root: &[u8; 32], fact: &Fact, proof: &MerkleProof) -> bool {
        let expected_leaf_hash = hash_leaf(&fact.to_bytes());
        if proof.leaf_hash != expected_leaf_hash {
            return false;
        }
        MerkleTree::verify_membership(root, proof)
    }

    /// Verify a non-membership proof against a root.
    pub fn verify_non_membership(
        root: &[u8; 32],
        _fact: &Fact,
        proof: &NonMembershipProof,
    ) -> bool {
        MerkleTree::verify_non_membership(root, proof)
    }

    /// Iterate over all facts in sorted order.
    pub fn iter(&self) -> impl Iterator<Item = &Fact> {
        self.facts.iter()
    }

    /// Get all facts as a vector.
    pub fn to_vec(&self) -> Vec<Fact> {
        self.facts.iter().copied().collect()
    }

    /// Create a snapshot of the current state.
    pub fn snapshot(&mut self) -> FactSetSnapshot {
        FactSetSnapshot {
            facts: self.to_vec(),
            root: self.root(),
        }
    }

    /// Restore from a snapshot. Verifies the root matches.
    pub fn from_snapshot(snapshot: &FactSetSnapshot) -> Option<Self> {
        let mut fs = Self::from_facts(snapshot.facts.iter().copied());
        let root = fs.root();
        if root == snapshot.root {
            Some(fs)
        } else {
            None
        }
    }

    /// Get facts matching a predicate.
    pub fn facts_with_predicate(
        &self,
        predicate: crate::field::FieldElement,
    ) -> Vec<&Fact> {
        self.facts
            .iter()
            .filter(|f| f.predicate == predicate)
            .collect()
    }

    /// Return the underlying Merkle tree (for advanced operations).
    pub fn tree(&self) -> &MerkleTree {
        &self.tree
    }

    /// Return a mutable reference to the underlying Merkle tree.
    pub fn tree_mut(&mut self) -> &mut MerkleTree {
        &mut self.tree
    }
}

impl Default for FactSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldElement;

    fn make_fact(name: &str, val: u64) -> Fact {
        Fact::unary(FieldElement::from_symbol(name), FieldElement::from_u64(val))
    }

    #[test]
    fn empty_factset() {
        let mut fs = FactSet::new();
        assert!(fs.is_empty());
        assert_eq!(fs.len(), 0);
        // Empty root should be consistent.
        let r1 = fs.root();
        let r2 = fs.root();
        assert_eq!(r1, r2);
    }

    #[test]
    fn insert_and_contains() {
        let mut fs = FactSet::new();
        let fact = make_fact("temperature", 72);
        fs.insert(fact);
        assert!(fs.contains(&fact));
        assert_eq!(fs.len(), 1);
    }

    #[test]
    fn insert_duplicate_idempotent() {
        let mut fs = FactSet::new();
        let fact = make_fact("x", 1);
        let root1 = fs.insert(fact);
        let root2 = fs.insert(fact);
        assert_eq!(root1, root2);
        assert_eq!(fs.len(), 1);
    }

    #[test]
    fn remove_fact() {
        let mut fs = FactSet::new();
        let fact = make_fact("y", 2);
        let empty_root = fs.root();
        fs.insert(fact);
        assert!(fs.contains(&fact));
        let new_root = fs.remove(&fact).unwrap();
        assert!(!fs.contains(&fact));
        assert_eq!(new_root, empty_root);
    }

    #[test]
    fn remove_absent_returns_none() {
        let mut fs = FactSet::new();
        let fact = make_fact("z", 3);
        assert!(fs.remove(&fact).is_none());
    }

    #[test]
    fn membership_proof_verifies() {
        let mut fs = FactSet::new();
        let f1 = make_fact("a", 1);
        let f2 = make_fact("b", 2);
        let f3 = make_fact("c", 3);
        fs.insert(f1);
        fs.insert(f2);
        fs.insert(f3);

        let root = fs.root();
        let proof = fs.membership_proof(&f2).unwrap();
        assert!(FactSet::verify_membership(&root, &f2, &proof));
    }

    #[test]
    fn membership_proof_wrong_fact_fails() {
        let mut fs = FactSet::new();
        let f1 = make_fact("a", 1);
        let f2 = make_fact("b", 2);
        fs.insert(f1);
        fs.insert(f2);

        let root = fs.root();
        let proof = fs.membership_proof(&f1).unwrap();
        // Verify with wrong fact should fail.
        assert!(!FactSet::verify_membership(&root, &f2, &proof));
    }

    #[test]
    fn non_membership_proof_verifies() {
        let mut fs = FactSet::new();
        let f1 = make_fact("a", 1);
        let f2 = make_fact("b", 2);
        let absent = make_fact("c", 3);
        fs.insert(f1);
        fs.insert(f2);

        let root = fs.root();
        let proof = fs.non_membership_proof(&absent).unwrap();
        assert!(FactSet::verify_non_membership(&root, &absent, &proof));
    }

    #[test]
    fn non_membership_present_fact_returns_none() {
        let mut fs = FactSet::new();
        let fact = make_fact("x", 1);
        fs.insert(fact);
        assert!(fs.non_membership_proof(&fact).is_none());
    }

    #[test]
    fn snapshot_roundtrip() {
        let mut fs = FactSet::new();
        fs.insert(make_fact("p", 1));
        fs.insert(make_fact("q", 2));
        fs.insert(make_fact("r", 3));

        let snap = fs.snapshot();
        let restored = FactSet::from_snapshot(&snap).unwrap();
        assert_eq!(restored.len(), 3);
        assert!(restored.contains(&make_fact("p", 1)));
        assert!(restored.contains(&make_fact("q", 2)));
        assert!(restored.contains(&make_fact("r", 3)));
    }

    #[test]
    fn facts_with_predicate_filter() {
        let mut fs = FactSet::new();
        let pred = FieldElement::from_symbol("owns");
        fs.insert(Fact::unary(pred, FieldElement::from_symbol("file1")));
        fs.insert(Fact::unary(pred, FieldElement::from_symbol("file2")));
        fs.insert(Fact::unary(
            FieldElement::from_symbol("other"),
            FieldElement::from_u64(99),
        ));

        let owns_facts = fs.facts_with_predicate(pred);
        assert_eq!(owns_facts.len(), 2);
    }

    #[test]
    fn from_facts_constructor() {
        let facts = vec![
            make_fact("a", 1),
            make_fact("b", 2),
            make_fact("c", 3),
        ];
        let mut fs = FactSet::from_facts(facts.clone());
        assert_eq!(fs.len(), 3);
        let root = fs.root();
        // Same facts inserted one by one should give same root.
        let mut fs2 = FactSet::new();
        for f in facts {
            fs2.insert(f);
        }
        assert_eq!(root, fs2.root());
    }

    #[test]
    fn iter_returns_sorted() {
        let mut fs = FactSet::new();
        let f1 = make_fact("z", 1);
        let f2 = make_fact("a", 2);
        let f3 = make_fact("m", 3);
        fs.insert(f1);
        fs.insert(f2);
        fs.insert(f3);

        let collected: Vec<Fact> = fs.iter().copied().collect();
        // Should be sorted.
        for i in 1..collected.len() {
            assert!(collected[i - 1] <= collected[i]);
        }
    }
}
