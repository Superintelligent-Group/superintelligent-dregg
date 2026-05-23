//! Authenticated outcome oracle backed by a positional sequence.
//!
//! ## Wire shape
//!
//! The oracle publishes a sequence of `OracleEntry { market_id, outcome_id,
//! timestamp }` at successive positions `0, 1, 2, ...`. Each entry is signed
//! by a pre-configured Ed25519 oracle key. The entire sequence is
//! Merkle-committed (binary blake3 tree); an `InclusionProof` is a sibling
//! path verifying that a given entry sits at a given position.
//!
//! ## Why not real KZG
//!
//! `pyana_storage::poly_queue` provides actual BN254-pairing KZG10 single-
//! point openings, but the module is gated behind the `kzg` cargo feature
//! and pulls a heavy arkworks chain. Until the feature flag is exposed
//! cleanly through `app-framework`, we ship a Merkle positional sequence
//! that preserves the same _positional_ semantics — entries live at named
//! positions, inclusion claims are O(log n) — and matches the same
//! `(market_id, outcome_id) <-> position` mapping the KZG fallback would.
//!
//! `// REVIEW[P3]:` switch to `KzgQueue` when the `kzg` feature is wired
//! through and the SRS-loading story is clear.
//!
//! ## Authentication
//!
//! Every `OracleReport` carries an Ed25519 signature by the pre-configured
//! oracle key. `Oracle::accept_report` rejects unsigned, malformed, or
//! wrong-key reports. The server's `/oracle/report` handler ONLY calls
//! `Oracle::accept_report`; there is no bypass.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::market::{MarketId, OutcomeId};

/// Serde helper for `[u8; 64]` (Ed25519 signatures).
mod serde_sig64 {
    use super::*;
    pub fn serialize<S: Serializer>(bytes: &[u8; 64], ser: S) -> Result<S::Ok, S::Error> {
        bytes.as_ref().serialize(ser)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<[u8; 64], D::Error> {
        let v: Vec<u8> = Deserialize::deserialize(de)?;
        v.try_into()
            .map_err(|_| serde::de::Error::custom("expected 64 bytes"))
    }
}

/// A single positional record from the oracle.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OracleEntry {
    pub market_id: MarketId,
    pub outcome_id: OutcomeId,
    pub timestamp: u64,
}

impl OracleEntry {
    /// The bytes that get signed AND that hash to the leaf in the positional
    /// merkle tree. Includes the position so signatures are bound to a slot.
    pub fn signing_bytes(&self, position: u64) -> Vec<u8> {
        let mut v = Vec::with_capacity(8 + 32 + 32 + 8);
        v.extend_from_slice(&position.to_le_bytes());
        v.extend_from_slice(&self.market_id);
        v.extend_from_slice(&self.outcome_id);
        v.extend_from_slice(&self.timestamp.to_le_bytes());
        v
    }

    /// Compute the leaf hash for the positional merkle tree.
    pub fn leaf_hash(&self, position: u64) -> [u8; 32] {
        let mut h = blake3::Hasher::new_derive_key("pyana-prediction-market-oracle-leaf-v1");
        h.update(&self.signing_bytes(position));
        *h.finalize().as_bytes()
    }
}

/// A signed report from the oracle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OracleReport {
    pub entry: OracleEntry,
    pub position: u64,
    pub oracle_pubkey: [u8; 32],
    #[serde(with = "serde_sig64")]
    pub signature: [u8; 64],
}

impl OracleReport {
    /// Verify the report's signature against the expected pubkey and the
    /// signed bytes `(position || market_id || outcome_id || timestamp)`.
    pub fn verify(&self) -> Result<(), OracleError> {
        if self.signature == [0u8; 64] {
            return Err(OracleError::UnsignedReport);
        }
        let vk = VerifyingKey::from_bytes(&self.oracle_pubkey)
            .map_err(|_| OracleError::MalformedKey)?;
        let sig = Signature::from_bytes(&self.signature);
        vk.verify(&self.entry.signing_bytes(self.position), &sig)
            .map_err(|_| OracleError::InvalidSignature)
    }
}

/// Inclusion proof for a positional sequence: a path of sibling hashes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InclusionProof {
    pub position: u64,
    pub leaf: [u8; 32],
    pub siblings: Vec<[u8; 32]>,
}

impl InclusionProof {
    /// Verify the proof against a claimed root.
    pub fn verify(&self, root: &[u8; 32]) -> bool {
        let mut current = self.leaf;
        let mut idx = self.position as usize;
        for sibling in &self.siblings {
            let mut h = blake3::Hasher::new();
            if idx % 2 == 0 {
                h.update(&current);
                h.update(sibling);
            } else {
                h.update(sibling);
                h.update(&current);
            }
            current = *h.finalize().as_bytes();
            idx /= 2;
        }
        current == *root
    }
}

/// The oracle authority + accepted-report log.
#[derive(Clone, Debug)]
pub struct Oracle {
    /// The single Ed25519 pubkey allowed to publish reports.
    /// (Multi-oracle median would be a v2 extension.)
    pub authority_pubkey: [u8; 32],
    /// All accepted entries, in the order accepted. Position `i` = `entries[i]`.
    entries: Vec<OracleEntry>,
}

impl Oracle {
    pub fn new(authority_pubkey: [u8; 32]) -> Self {
        Self {
            authority_pubkey,
            entries: Vec::new(),
        }
    }

    /// Append a signed report to the oracle log.
    ///
    /// Rejects if:
    /// - the signature is unsigned / malformed / does not verify
    /// - the report's pubkey does not match the configured authority
    /// - the report's `position` does not equal `self.entries.len()`
    pub fn accept_report(&mut self, report: &OracleReport) -> Result<(), OracleError> {
        if report.oracle_pubkey != self.authority_pubkey {
            return Err(OracleError::UntrustedKey);
        }
        report.verify()?;
        let expected_position = self.entries.len() as u64;
        if report.position != expected_position {
            return Err(OracleError::WrongPosition {
                expected: expected_position,
                got: report.position,
            });
        }
        self.entries.push(report.entry.clone());
        Ok(())
    }

    /// All known entries.
    pub fn entries(&self) -> &[OracleEntry] {
        &self.entries
    }

    /// The merkle root of the positional sequence.
    pub fn root(&self) -> [u8; 32] {
        let leaves = self.leaf_hashes();
        merkle_root(&leaves)
    }

    /// Compute leaf hashes for all entries.
    fn leaf_hashes(&self) -> Vec<[u8; 32]> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, e)| e.leaf_hash(i as u64))
            .collect()
    }

    /// Build an inclusion proof for a given position.
    pub fn inclusion_proof(&self, position: u64) -> Option<InclusionProof> {
        let leaves = self.leaf_hashes();
        if (position as usize) >= leaves.len() {
            return None;
        }
        let siblings = merkle_siblings(&leaves, position as usize);
        Some(InclusionProof {
            position,
            leaf: leaves[position as usize],
            siblings,
        })
    }

    /// The entry at `position`, if any.
    pub fn entry_at(&self, position: u64) -> Option<&OracleEntry> {
        self.entries.get(position as usize)
    }

    /// Find the most recent entry for `market_id` and return `(position, entry)`.
    pub fn latest_for_market(&self, market_id: &MarketId) -> Option<(u64, &OracleEntry)> {
        self.entries
            .iter()
            .enumerate()
            .rev()
            .find(|(_, e)| &e.market_id == market_id)
            .map(|(i, e)| (i as u64, e))
    }
}

/// Errors from oracle operations.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OracleError {
    #[error("report is not signed (signature is all zeros)")]
    UnsignedReport,
    #[error("report's signature did not verify")]
    InvalidSignature,
    #[error("oracle pubkey could not be parsed as a valid Ed25519 verifying key")]
    MalformedKey,
    #[error("report's pubkey is not the configured oracle authority")]
    UntrustedKey,
    #[error("report position {got} does not match expected next position {expected}")]
    WrongPosition { expected: u64, got: u64 },
}

// ============================================================================
// Helpers
// ============================================================================

/// Sign a report with the given signing key. Used by clients (tests, real
/// oracle daemons) to produce reports.
pub fn sign_report(
    signing_key_bytes: &[u8; 32],
    entry: OracleEntry,
    position: u64,
) -> OracleReport {
    let sk = SigningKey::from_bytes(signing_key_bytes);
    let pubkey = sk.verifying_key().to_bytes();
    let sig = sk.sign(&entry.signing_bytes(position));
    OracleReport {
        entry,
        position,
        oracle_pubkey: pubkey,
        signature: sig.to_bytes(),
    }
}

/// Derive the Ed25519 pubkey from a 32-byte signing key (for tests/config).
pub fn pubkey_of(signing_key_bytes: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(signing_key_bytes)
        .verifying_key()
        .to_bytes()
}

/// Build a report that intentionally has an empty signature (for adversarial
/// tests that the server rejects it).
pub fn unsigned_report_for_test(entry: OracleEntry, position: u64, claimed_pubkey: [u8; 32]) -> OracleReport {
    OracleReport {
        entry,
        position,
        oracle_pubkey: claimed_pubkey,
        signature: [0u8; 64],
    }
}

// ----- Internal binary-merkle helpers (mirrors blinded.rs style) -----

fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return *blake3::hash(b"empty_oracle_positional_sequence").as_bytes();
    }
    if leaves.len() == 1 {
        return leaves[0];
    }
    let mut layer: Vec<[u8; 32]> = leaves.to_vec();
    let next_pow2 = layer.len().next_power_of_two();
    layer.resize(next_pow2, [0u8; 32]);
    while layer.len() > 1 {
        let mut next = Vec::with_capacity(layer.len() / 2);
        for pair in layer.chunks(2) {
            let mut h = blake3::Hasher::new();
            h.update(&pair[0]);
            h.update(&pair[1]);
            next.push(*h.finalize().as_bytes());
        }
        layer = next;
    }
    layer[0]
}

fn merkle_siblings(leaves: &[[u8; 32]], position: usize) -> Vec<[u8; 32]> {
    if leaves.len() <= 1 {
        return Vec::new();
    }
    let mut layer: Vec<[u8; 32]> = leaves.to_vec();
    let next_pow2 = layer.len().next_power_of_two();
    layer.resize(next_pow2, [0u8; 32]);

    let mut proof = Vec::new();
    let mut idx = position;
    while layer.len() > 1 {
        let sib_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
        proof.push(layer[sib_idx]);
        let mut next = Vec::with_capacity(layer.len() / 2);
        for pair in layer.chunks(2) {
            let mut h = blake3::Hasher::new();
            h.update(&pair[0]);
            h.update(&pair[1]);
            next.push(*h.finalize().as_bytes());
        }
        layer = next;
        idx /= 2;
    }
    proof
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_signed_report_then_reject_unsigned() {
        let sk = [7u8; 32];
        let pubkey = pubkey_of(&sk);
        let mut oracle = Oracle::new(pubkey);

        let entry = OracleEntry {
            market_id: [1u8; 32],
            outcome_id: [2u8; 32],
            timestamp: 100,
        };
        let report = sign_report(&sk, entry.clone(), 0);
        assert!(oracle.accept_report(&report).is_ok());

        let unsigned = unsigned_report_for_test(entry, 1, pubkey);
        assert_eq!(
            oracle.accept_report(&unsigned),
            Err(OracleError::UnsignedReport)
        );
    }

    #[test]
    fn untrusted_key_rejected_even_when_signature_valid() {
        let attacker_sk = [9u8; 32];
        let authority_sk = [7u8; 32];
        let authority_pubkey = pubkey_of(&authority_sk);
        let mut oracle = Oracle::new(authority_pubkey);

        let entry = OracleEntry {
            market_id: [1u8; 32],
            outcome_id: [2u8; 32],
            timestamp: 100,
        };
        let bad = sign_report(&attacker_sk, entry, 0);
        assert_eq!(oracle.accept_report(&bad), Err(OracleError::UntrustedKey));
    }

    #[test]
    fn inclusion_proof_round_trips() {
        let sk = [7u8; 32];
        let pubkey = pubkey_of(&sk);
        let mut oracle = Oracle::new(pubkey);
        for i in 0..5u64 {
            let entry = OracleEntry {
                market_id: [i as u8; 32],
                outcome_id: [(i + 100) as u8; 32],
                timestamp: i,
            };
            oracle.accept_report(&sign_report(&sk, entry, i)).unwrap();
        }
        let root = oracle.root();
        for i in 0..5 {
            let proof = oracle.inclusion_proof(i).unwrap();
            assert!(proof.verify(&root));
        }
    }
}
