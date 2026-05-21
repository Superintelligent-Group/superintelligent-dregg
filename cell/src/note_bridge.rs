//! Note bridge: cross-federation value transfer via proof-carrying notes.
//!
//! Notes are self-proving (the STARK proof carries all verification needed). A note
//! "burned" (nullifier published) in Federation A can be "minted" in Federation B by
//! presenting the spending proof. The proof IS the bridge — no light client needed.
//!
//! # Security Model
//!
//! The bridge relies on:
//! 1. **Nullifier uniqueness**: Since nullifiers are derived from note-intrinsic data
//!    (not tree position), the same note produces the same nullifier everywhere. A
//!    nullifier revealed in Fed A cannot be replayed in Fed B for a different note.
//! 2. **Trusted roots**: The destination federation maintains a set of trusted roots
//!    from source federations. Only proofs against these roots are accepted.
//! 3. **Bridged-nullifier tracking**: Each federation tracks which nullifiers have been
//!    bridged in, preventing double-bridge (same note minted twice).
//! 4. **STARK proof verification**: The spending proof proves knowledge of the spending
//!    key and Merkle membership without revealing the note contents.

use serde::{Deserialize, Serialize};

use crate::note::{NoteCommitment, Nullifier};
use pyana_types::AttestedRoot;

/// A portable note proof that can be presented to another federation.
///
/// This is the "bridge message" — the thing Alice creates in Federation A
/// and presents to Federation B to mint equivalent value.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PortableNoteProof {
    /// The nullifier (proves the note was spent in the source federation).
    pub nullifier: [u8; 32],
    /// The source federation's attested root at time of spend.
    pub source_root: AttestedRoot,
    /// The STARK proof of valid spending (NoteSpendingAir).
    /// Serialized via postcard from a StarkProof.
    pub spending_proof: Vec<u8>,
    /// The new note commitment for the destination (what gets minted).
    pub destination_commitment: NoteCommitment,
    /// Value being transferred.
    pub value: u64,
    /// Asset type.
    pub asset_type: u64,
}

/// Errors that can occur during bridge operations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeError {
    /// The source root is not in our trusted set.
    UntrustedRoot {
        /// Short hex of the untrusted root for diagnostics.
        root_hex: String,
    },
    /// The source root does not contain a note_tree_root (federation too old).
    MissingNoteTreeRoot,
    /// The STARK spending proof failed verification.
    InvalidSpendingProof { reason: String },
    /// The nullifier has already been bridged (double-bridge attempt).
    AlreadyBridged { nullifier: [u8; 32] },
    /// The nullifier in the proof does not match the public inputs.
    NullifierMismatch,
    /// Value or asset type inconsistency.
    ValueMismatch { expected: u64, got: u64 },
}

impl core::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BridgeError::UntrustedRoot { root_hex } => {
                write!(f, "source root {root_hex}... is not in the trusted set")
            }
            BridgeError::MissingNoteTreeRoot => {
                write!(
                    f,
                    "source root does not contain a note_tree_root attestation"
                )
            }
            BridgeError::InvalidSpendingProof { reason } => {
                write!(f, "STARK spending proof verification failed: {reason}")
            }
            BridgeError::AlreadyBridged { nullifier } => {
                write!(
                    f,
                    "nullifier {:02x}{:02x}{:02x}{:02x}... already bridged",
                    nullifier[0], nullifier[1], nullifier[2], nullifier[3]
                )
            }
            BridgeError::NullifierMismatch => {
                write!(f, "nullifier does not match proof public inputs")
            }
            BridgeError::ValueMismatch { expected, got } => {
                write!(f, "value mismatch: expected {expected}, got {got}")
            }
        }
    }
}

impl std::error::Error for BridgeError {}

/// A set of nullifiers that have been bridged into this federation from others.
///
/// Prevents the same portable note proof from being accepted twice (double-bridge).
/// Separate from the local NullifierSet which tracks locally-spent notes.
#[derive(Clone, Debug, Default)]
pub struct BridgedNullifierSet {
    /// Sorted set of bridged nullifiers for O(log n) lookup.
    nullifiers: Vec<[u8; 32]>,
}

impl BridgedNullifierSet {
    /// Create an empty bridged nullifier set.
    pub fn new() -> Self {
        Self {
            nullifiers: Vec::new(),
        }
    }

    /// Check if a nullifier has already been bridged.
    pub fn contains(&self, nullifier: &[u8; 32]) -> bool {
        self.nullifiers.binary_search(nullifier).is_ok()
    }

    /// Insert a bridged nullifier. Returns error if already present.
    pub fn insert(&mut self, nullifier: [u8; 32]) -> Result<(), BridgeError> {
        match self.nullifiers.binary_search(&nullifier) {
            Ok(_) => Err(BridgeError::AlreadyBridged { nullifier }),
            Err(idx) => {
                self.nullifiers.insert(idx, nullifier);
                Ok(())
            }
        }
    }

    /// Number of bridged nullifiers.
    pub fn len(&self) -> usize {
        self.nullifiers.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.nullifiers.is_empty()
    }
}

/// Verify a portable note proof from another federation.
///
/// This is the core verification that a destination federation performs before
/// minting a new note. It checks:
/// 1. The source_root is in our trusted set (we accept proofs from that federation).
/// 2. The source_root has a note_tree_root (the source federation attests note trees).
/// 3. The STARK spending proof verifies against the source_root's note_tree_root.
/// 4. The nullifier is consistent with the proof's public inputs.
///
/// On success, the caller should:
/// - Add the nullifier to the bridged-nullifier set (prevent double-bridge).
/// - Create a new note commitment in the local note tree.
///
/// # Arguments
///
/// * `proof` - The portable note proof to verify.
/// * `trusted_roots` - The set of attested roots we accept from other federations.
/// * `verify_stark` - A closure that verifies the STARK proof given (nullifier_bytes, merkle_root_bytes, proof_bytes).
///   Returns Ok(()) if valid.
pub fn verify_portable_note<F>(
    proof: &PortableNoteProof,
    trusted_roots: &[AttestedRoot],
    verify_stark: F,
) -> Result<(), BridgeError>
where
    F: FnOnce(&[u8; 32], &[u8; 32], &[u8]) -> Result<(), String>,
{
    // 1. Check source_root is in our trusted set.
    let is_trusted = trusted_roots.iter().any(|r| {
        r.merkle_root == proof.source_root.merkle_root
            && r.height == proof.source_root.height
            && r.note_tree_root == proof.source_root.note_tree_root
    });
    if !is_trusted {
        let root_hex = proof
            .source_root
            .merkle_root
            .iter()
            .take(4)
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        return Err(BridgeError::UntrustedRoot { root_hex });
    }

    // 2. Check the source root has a note_tree_root.
    let note_tree_root = proof
        .source_root
        .note_tree_root
        .ok_or(BridgeError::MissingNoteTreeRoot)?;

    // 3. Verify the STARK spending proof.
    verify_stark(&proof.nullifier, &note_tree_root, &proof.spending_proof)
        .map_err(|reason| BridgeError::InvalidSpendingProof { reason })?;

    // 4. Verification passed. The nullifier corresponds to a valid note in the
    //    source federation's note tree at the attested root.
    Ok(())
}

/// Create a portable note proof for cross-federation transfer.
///
/// This is called by the note owner in the source federation after spending
/// their note there. It packages the spending proof along with the federation's
/// attested root into a portable format that can be presented elsewhere.
///
/// # Arguments
///
/// * `nullifier` - The nullifier revealed when spending in the source federation.
/// * `spending_proof` - The serialized STARK proof from `prove_note_spend`.
/// * `source_root` - The source federation's attested root at time of spend.
/// * `destination_commitment` - The new note commitment for the destination federation.
/// * `value` - The value being transferred.
/// * `asset_type` - The asset type being transferred.
pub fn create_portable_note(
    nullifier: Nullifier,
    spending_proof: Vec<u8>,
    source_root: AttestedRoot,
    destination_commitment: NoteCommitment,
    value: u64,
    asset_type: u64,
) -> PortableNoteProof {
    PortableNoteProof {
        nullifier: nullifier.0,
        source_root,
        spending_proof,
        destination_commitment,
        value,
        asset_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_attested_root(height: u64, note_root: Option<[u8; 32]>) -> AttestedRoot {
        AttestedRoot {
            merkle_root: [height as u8; 32],
            note_tree_root: note_root,
            nullifier_set_root: None,
            height,
            timestamp: 1000 + height as i64,
            quorum_signatures: vec![],
            threshold_qc: None,
            threshold: 0,
        }
    }

    fn make_proof(nullifier: [u8; 32], value: u64, asset_type: u64) -> PortableNoteProof {
        let source_root = make_attested_root(42, Some([0xAA; 32]));
        PortableNoteProof {
            nullifier,
            source_root,
            spending_proof: vec![1, 2, 3, 4], // dummy proof bytes
            destination_commitment: NoteCommitment([0xBB; 32]),
            value,
            asset_type,
        }
    }

    /// A dummy verifier that always succeeds.
    fn verify_ok(_nullifier: &[u8; 32], _root: &[u8; 32], _proof: &[u8]) -> Result<(), String> {
        Ok(())
    }

    /// A dummy verifier that always fails.
    fn verify_fail(
        _nullifier: &[u8; 32],
        _root: &[u8; 32],
        _proof: &[u8],
    ) -> Result<(), String> {
        Err("mock verification failure".to_string())
    }

    #[test]
    fn test_verify_portable_note_success() {
        let trusted = vec![make_attested_root(42, Some([0xAA; 32]))];
        let proof = make_proof([1u8; 32], 100, 1);
        let result = verify_portable_note(&proof, &trusted, verify_ok);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_portable_note_untrusted_root() {
        // Trusted set has height 99, but proof has height 42.
        let trusted = vec![make_attested_root(99, Some([0xCC; 32]))];
        let proof = make_proof([1u8; 32], 100, 1);
        let result = verify_portable_note(&proof, &trusted, verify_ok);
        assert!(matches!(result, Err(BridgeError::UntrustedRoot { .. })));
    }

    #[test]
    fn test_verify_portable_note_missing_note_tree_root() {
        // Trusted root has no note_tree_root.
        let trusted = vec![make_attested_root(42, None)];
        let mut proof = make_proof([1u8; 32], 100, 1);
        proof.source_root.note_tree_root = None;
        let result = verify_portable_note(&proof, &trusted, verify_ok);
        assert!(matches!(result, Err(BridgeError::MissingNoteTreeRoot)));
    }

    #[test]
    fn test_verify_portable_note_invalid_proof() {
        let trusted = vec![make_attested_root(42, Some([0xAA; 32]))];
        let proof = make_proof([1u8; 32], 100, 1);
        let result = verify_portable_note(&proof, &trusted, verify_fail);
        assert!(matches!(
            result,
            Err(BridgeError::InvalidSpendingProof { .. })
        ));
    }

    #[test]
    fn test_bridged_nullifier_set_insert_and_contains() {
        let mut set = BridgedNullifierSet::new();
        let n = [42u8; 32];

        assert!(!set.contains(&n));
        set.insert(n).unwrap();
        assert!(set.contains(&n));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_bridged_nullifier_set_double_bridge_rejected() {
        let mut set = BridgedNullifierSet::new();
        let n = [42u8; 32];

        set.insert(n).unwrap();
        let result = set.insert(n);
        assert!(matches!(result, Err(BridgeError::AlreadyBridged { .. })));
    }

    #[test]
    fn test_bridged_nullifier_set_multiple() {
        let mut set = BridgedNullifierSet::new();
        for i in 0..10u8 {
            let mut n = [0u8; 32];
            n[0] = i;
            set.insert(n).unwrap();
        }
        assert_eq!(set.len(), 10);

        for i in 0..10u8 {
            let mut n = [0u8; 32];
            n[0] = i;
            assert!(set.contains(&n));
        }
    }

    #[test]
    fn test_create_portable_note() {
        let nullifier = Nullifier([0x11; 32]);
        let source_root = make_attested_root(10, Some([0xAA; 32]));
        let dest_commitment = NoteCommitment([0xBB; 32]);

        let portable = create_portable_note(
            nullifier,
            vec![5, 6, 7, 8],
            source_root.clone(),
            dest_commitment,
            500,
            2,
        );

        assert_eq!(portable.nullifier, [0x11; 32]);
        assert_eq!(portable.value, 500);
        assert_eq!(portable.asset_type, 2);
        assert_eq!(portable.destination_commitment, dest_commitment);
        assert_eq!(portable.source_root.height, 10);
    }

    #[test]
    fn test_verify_then_bridge_flow() {
        // Simulate the full flow: verify then track in bridged set.
        let trusted = vec![make_attested_root(42, Some([0xAA; 32]))];
        let proof = make_proof([0x99; 32], 100, 1);
        let mut bridged_set = BridgedNullifierSet::new();

        // First bridge succeeds.
        verify_portable_note(&proof, &trusted, verify_ok).unwrap();
        bridged_set.insert(proof.nullifier).unwrap();

        // Second bridge with same nullifier fails.
        let result = bridged_set.insert(proof.nullifier);
        assert!(matches!(result, Err(BridgeError::AlreadyBridged { .. })));
    }
}
