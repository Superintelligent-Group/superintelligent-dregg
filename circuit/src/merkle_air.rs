//! Merkle membership proof AIR.
//!
//! Types, witnesses, and helpers live in [`crate::merkle_types`]. This module
//! contains only the `Air` implementation for `MerkleAir`.
//!
//! Proves: "this fact exists in the committed 4-ary Merkle tree at this root."
//!
//! Trace layout (one row per tree level, from leaf to root):
//!
//! | Column       | Description                                           |
//! |-------------|-------------------------------------------------------|
//! | 0: current  | Hash at this level (starts as leaf hash)              |
//! | 1: sib0     | First sibling hash                                    |
//! | 2: sib1     | Second sibling hash                                   |
//! | 3: sib2     | Third sibling hash                                    |
//! | 4: position | Position index at this level (0..3)                   |
//! | 5: parent   | Computed parent hash (hash_node of children in order) |
//!
//! Constraints:
//! 1. Position is valid: position * (position - 1) * (position - 2) * (position - 3) = 0
//! 2. Parent is correctly computed from children ordered by position
//! 3. Chain continuity: parent[row] = current[row + 1]
//! 4. Last row's parent = expected root (public input)
//! 5. First row's current = leaf hash (public input)

// Re-export everything from merkle_types for backward compatibility.
pub use crate::merkle_types::*;

use crate::constraint_prover::{Air, Constraint};
use crate::field::BabyBear;

impl Air for MerkleAir {
    fn trace_width(&self) -> usize {
        MERKLE_AIR_WIDTH
    }

    fn num_public_inputs(&self) -> usize {
        2 // leaf_hash, expected_root
    }

    fn constraints(&self) -> Vec<Constraint> {
        vec![
            // Constraint 1: Position validity.
            // position * (position - 1) * (position - 2) * (position - 3) = 0
            Constraint {
                name: "position_valid".to_string(),
                eval: Box::new(|row, _, _| {
                    let pos = row[col::POSITION];
                    pos * (pos - BabyBear::ONE)
                        * (pos - BabyBear::new(2))
                        * (pos - BabyBear::new(3))
                }),
            },
            // Constraint 2: Parent hash is correctly computed.
            // We verify this by checking that `parent` equals the hash of children
            // arranged according to `position`.
            //
            // In a real STARK AIR, we'd expand this into Poseidon2 round constraints.
            // For the mock prover, we directly check the hash.
            Constraint {
                name: "parent_hash_correct".to_string(),
                eval: Box::new(|row, _, _| {
                    let current = row[col::CURRENT];
                    let siblings = [row[col::SIB0], row[col::SIB1], row[col::SIB2]];
                    let position = row[col::POSITION].as_u32() as u8;

                    // Skip if position is out of range (caught by position_valid)
                    if position > 3 {
                        return BabyBear::ZERO;
                    }

                    let expected_parent = MerkleAir::compute_parent(current, position, &siblings);
                    let claimed_parent = row[col::PARENT];

                    expected_parent - claimed_parent
                }),
            },
            // Constraint 3: Chain continuity — parent[row] = current[row+1].
            // Only applies to non-last rows.
            Constraint {
                name: "chain_continuity".to_string(),
                eval: Box::new(|row, next_row, _| {
                    if let Some(next) = next_row {
                        row[col::PARENT] - next[col::CURRENT]
                    } else {
                        BabyBear::ZERO // No constraint on last row
                    }
                }),
            },
        ]
    }

    fn first_row_constraints(&self) -> Vec<Constraint> {
        vec![
            // First row's current must equal the leaf hash (public input 0).
            Constraint {
                name: "leaf_hash_match".to_string(),
                eval: Box::new(|row, _, public_inputs| row[col::CURRENT] - public_inputs[0]),
            },
        ]
    }

    fn last_row_constraints(&self) -> Vec<Constraint> {
        vec![
            // Last row's parent must equal the expected root (public input 1).
            Constraint {
                name: "root_match".to_string(),
                eval: Box::new(|row, _, public_inputs| row[col::PARENT] - public_inputs[1]),
            },
        ]
    }

    fn generate_trace(&self) -> (Vec<Vec<BabyBear>>, Vec<BabyBear>) {
        let mut trace = Vec::with_capacity(self.witness.levels.len());
        let mut current = self.witness.leaf_hash;

        for level in &self.witness.levels {
            let parent = MerkleAir::compute_parent(current, level.position, &level.siblings);

            let row = vec![
                current,
                level.siblings[0],
                level.siblings[1],
                level.siblings[2],
                BabyBear::new(level.position as u32),
                parent,
            ];
            trace.push(row);
            current = parent;
        }

        let public_inputs = vec![self.witness.leaf_hash, self.witness.expected_root];
        (trace, public_inputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint_prover::ConstraintProver;

    #[test]
    fn merkle_air_valid_proof() {
        let leaf = BabyBear::new(12345);
        let witness = create_test_witness(leaf, TREE_DEPTH);
        let air = MerkleAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(
            result.is_valid(),
            "Merkle AIR should verify: {:?}",
            result.violations()
        );
    }

    #[test]
    fn merkle_air_wrong_root_fails() {
        let leaf = BabyBear::new(12345);
        let mut witness = create_test_witness(leaf, TREE_DEPTH);
        // Tamper with expected root
        witness.expected_root = BabyBear::new(99999);
        let air = MerkleAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(!result.is_valid());
    }

    #[test]
    fn merkle_air_wrong_leaf_fails() {
        let leaf = BabyBear::new(12345);
        let mut witness = create_test_witness(leaf, TREE_DEPTH);
        // Tamper with leaf hash (but keep levels the same)
        witness.leaf_hash = BabyBear::new(99999);
        let air = MerkleAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(!result.is_valid());
    }

    #[test]
    fn merkle_air_tampered_sibling_fails() {
        let leaf = BabyBear::new(12345);
        let mut witness = create_test_witness(leaf, TREE_DEPTH);
        // Tamper with a sibling at level 5
        witness.levels[5].siblings[0] = BabyBear::new(999999);
        let air = MerkleAir::new(witness);
        let result = ConstraintProver::verify(&air);
        // This should fail because the parent hashes won't chain correctly
        assert!(!result.is_valid());
    }

    #[test]
    fn merkle_air_invalid_position_fails() {
        let leaf = BabyBear::new(42);
        let mut witness = create_test_witness(leaf, 4);
        // Set an invalid position
        witness.levels[2].position = 5; // only 0-3 are valid
        let air = MerkleAir::new(witness);
        let result = ConstraintProver::verify(&air);
        // Should fail: position_valid constraint catches out-of-range position,
        // and chain_continuity fails because compute_parent returns ZERO for invalid pos.
        assert!(!result.is_valid());
        let has_position_violation = result
            .violations()
            .iter()
            .any(|v| v.constraint_name.contains("position_valid"));
        assert!(has_position_violation);
    }

    #[test]
    fn merkle_air_short_depth() {
        // Test with a small tree (depth 4)
        let leaf = BabyBear::new(7777);
        let witness = create_test_witness(leaf, 4);
        let air = MerkleAir::new(witness);
        let result = ConstraintProver::verify(&air);
        assert!(result.is_valid());
    }
}
