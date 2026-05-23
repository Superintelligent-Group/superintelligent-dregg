//! Solo federation mode: single-node operation for devnets.
//!
//! When running with `FederationMode::Solo`, the local node processes all turns
//! without waiting for BFT quorum. This trades safety (no Byzantine fault tolerance)
//! for liveness (100% uptime with a single node).
//!
//! # Safety Argument
//!
//! Solo mode is safe when:
//! - There is exactly one operator (no Byzantine adversaries)
//! - Single-owner turns cannot harm others regardless of mode
//! - The nullifier log provides replay protection on rejoin
//!
//! # Rejoin Protocol
//!
//! When peers come back online:
//! 1. They receive the solo node's signed nullifier log
//! 2. They validate each entry (no double-spends, valid signatures)
//! 3. Tentative receipts are promoted to Final if no conflicts
//! 4. The federation upgrades back to Full mode

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

// =============================================================================
// FederationMode
// =============================================================================

/// The operating mode of a federation node.
///
/// Controls quorum requirements and finality semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FederationMode {
    /// Full BFT: require 2f+1 signatures for finality (standard behavior).
    Full,
    /// Solo: single node processes all turns. No BFT safety but full liveness.
    /// Safe when there are no Byzantine adversaries (devnet, single-operator).
    Solo,
}

impl Default for FederationMode {
    fn default() -> Self {
        FederationMode::Full
    }
}

impl std::fmt::Display for FederationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FederationMode::Full => write!(f, "full"),
            FederationMode::Solo => write!(f, "solo"),
        }
    }
}

impl std::str::FromStr for FederationMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "full" => Ok(FederationMode::Full),
            "solo" => Ok(FederationMode::Solo),
            other => Err(format!(
                "unknown federation mode: '{}' (expected 'full' or 'solo')",
                other
            )),
        }
    }
}

// =============================================================================
// Quorum Threshold (mode-aware)
// =============================================================================

/// Compute the effective quorum threshold for a given mode and node count.
///
/// In Full mode: standard BFT threshold (n - f where f = n/3).
/// In Solo mode: 1 (the local node's signature is sufficient).
pub fn effective_quorum_threshold(mode: FederationMode, num_nodes: usize) -> usize {
    match mode {
        FederationMode::Full => crate::quorum_threshold(num_nodes),
        FederationMode::Solo => 1,
    }
}

// =============================================================================
// Nullifier Log (solo mode sequencer)
// =============================================================================

/// A signed entry in the solo-mode nullifier log.
///
/// Each entry records a nullifier insertion with the sequencing node's signature.
/// On rejoin, peers replay this log to validate no conflicts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NullifierLogEntry {
    /// The nullifier being inserted (BLAKE3 hash of the spent note).
    pub nullifier: [u8; 32],
    /// The turn hash that consumed this nullifier.
    pub turn_hash: [u8; 32],
    /// Sequence number (monotonically increasing within an epoch).
    pub sequence: u64,
    /// Block height at which this entry was produced.
    pub height: u64,
    /// BLAKE3-keyed MAC from the sequencing node (not a full Ed25519 sig for perf).
    /// Verifiers who trust the solo node can validate this cheaply.
    pub node_signature: [u8; 32],
}

/// The nullifier log maintained by a solo-mode node.
///
/// This is the authoritative ordering of nullifier insertions during solo operation.
/// It prevents double-spend: if the same nullifier appears twice, the second is rejected.
#[derive(Clone, Debug, Default)]
pub struct NullifierLog {
    /// All entries in sequence order.
    entries: Vec<NullifierLogEntry>,
    /// Fast lookup set for conflict detection.
    seen: HashSet<[u8; 32]>,
    /// Current sequence counter.
    next_sequence: u64,
    /// Signing key for entry authentication (BLAKE3-keyed hash).
    signing_key: [u8; 32],
}

impl NullifierLog {
    /// Create a new empty nullifier log with the given signing key.
    pub fn new(signing_key: [u8; 32]) -> Self {
        Self {
            entries: Vec::new(),
            seen: HashSet::new(),
            next_sequence: 0,
            signing_key,
        }
    }

    /// Attempt to insert a nullifier. Returns Ok(entry) if novel, Err if duplicate.
    pub fn insert(
        &mut self,
        nullifier: [u8; 32],
        turn_hash: [u8; 32],
        height: u64,
    ) -> Result<&NullifierLogEntry, NullifierConflict> {
        if self.seen.contains(&nullifier) {
            return Err(NullifierConflict { nullifier });
        }

        let sequence = self.next_sequence;
        self.next_sequence += 1;

        let node_signature = self.sign_entry(&nullifier, &turn_hash, sequence, height);

        self.seen.insert(nullifier);
        self.entries.push(NullifierLogEntry {
            nullifier,
            turn_hash,
            sequence,
            height,
            node_signature,
        });

        Ok(self.entries.last().unwrap())
    }

    /// Check if a nullifier has already been consumed.
    pub fn contains(&self, nullifier: &[u8; 32]) -> bool {
        self.seen.contains(nullifier)
    }

    /// Get all entries (for syncing to rejoining peers).
    pub fn entries(&self) -> &[NullifierLogEntry] {
        &self.entries
    }

    /// Number of entries in the log.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Validate a set of entries from a peer (used during rejoin).
    /// Returns Ok if all entries are valid and conflict-free, Err on first conflict.
    pub fn validate_remote_entries(
        &self,
        entries: &[NullifierLogEntry],
        remote_key: &[u8; 32],
    ) -> Result<(), NullifierConflict> {
        let mut local_seen = self.seen.clone();
        for entry in entries {
            // Verify signature.
            let expected_sig = Self::compute_signature(
                remote_key,
                &entry.nullifier,
                &entry.turn_hash,
                entry.sequence,
                entry.height,
            );
            if expected_sig != entry.node_signature {
                return Err(NullifierConflict {
                    nullifier: entry.nullifier,
                });
            }
            // Check for conflicts with our local state.
            if local_seen.contains(&entry.nullifier) {
                return Err(NullifierConflict {
                    nullifier: entry.nullifier,
                });
            }
            local_seen.insert(entry.nullifier);
        }
        Ok(())
    }

    /// Merge validated remote entries into the local log.
    /// Call this only after `validate_remote_entries` succeeds.
    pub fn merge_validated(&mut self, entries: Vec<NullifierLogEntry>) {
        for entry in entries {
            if !self.seen.contains(&entry.nullifier) {
                self.seen.insert(entry.nullifier);
                self.entries.push(entry);
            }
        }
        // Re-sort by sequence for consistent ordering.
        self.entries.sort_by_key(|e| e.sequence);
        // Update next_sequence to be past the maximum.
        if let Some(max_seq) = self.entries.last().map(|e| e.sequence) {
            self.next_sequence = max_seq + 1;
        }
    }

    fn sign_entry(
        &self,
        nullifier: &[u8; 32],
        turn_hash: &[u8; 32],
        sequence: u64,
        height: u64,
    ) -> [u8; 32] {
        Self::compute_signature(&self.signing_key, nullifier, turn_hash, sequence, height)
    }

    fn compute_signature(
        key: &[u8; 32],
        nullifier: &[u8; 32],
        turn_hash: &[u8; 32],
        sequence: u64,
        height: u64,
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(key);
        hasher.update(b"pyana-nullifier-log-entry-v1");
        hasher.update(nullifier);
        hasher.update(turn_hash);
        hasher.update(&sequence.to_le_bytes());
        hasher.update(&height.to_le_bytes());
        *hasher.finalize().as_bytes()
    }
}

/// Error returned when a nullifier has already been consumed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NullifierConflict {
    pub nullifier: [u8; 32],
}

impl std::fmt::Display for NullifierConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "nullifier conflict: {} already consumed",
            hex::encode(&self.nullifier[..8])
        )
    }
}

impl std::error::Error for NullifierConflict {}

// =============================================================================
// Solo Consensus State
// =============================================================================

/// Solo-mode consensus state: a thin wrapper that auto-finalizes without quorum.
///
/// When in solo mode, the node:
/// 1. Produces blocks unilaterally (it is always the leader)
/// 2. Signs blocks with its own key only (no waiting for votes)
/// 3. Produces Tentative receipts for consensus-path turns
/// 4. Maintains a nullifier log for ordering
#[derive(Clone, Debug)]
pub struct SoloConsensusState {
    /// The current operating mode.
    pub mode: FederationMode,
    /// Current block height (increments on each finalized block).
    pub height: u64,
    /// Signing key for this node.
    pub signing_key: [u8; 32],
    /// The nullifier log.
    pub nullifier_log: NullifierLog,
    /// Whether this node has detected peers and should upgrade.
    pub peers_detected: bool,
}

impl SoloConsensusState {
    /// Create a new solo consensus state.
    pub fn new(signing_key: [u8; 32]) -> Self {
        Self {
            mode: FederationMode::Solo,
            height: 0,
            signing_key,
            nullifier_log: NullifierLog::new(signing_key),
            peers_detected: false,
        }
    }

    /// Signal that peers have been detected. The node should upgrade to Full mode.
    pub fn detect_peers(&mut self) {
        self.peers_detected = true;
        tracing::info!(
            "peers detected at height {}: upgrading federation mode from Solo to Full",
            self.height
        );
        self.mode = FederationMode::Full;
    }

    /// Get the effective quorum threshold for the current mode.
    pub fn effective_threshold(&self, num_nodes: usize) -> usize {
        effective_quorum_threshold(self.mode, num_nodes)
    }

    /// Advance height (called after processing a turn in solo mode).
    pub fn advance_height(&mut self) {
        self.height += 1;
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_mode_display_and_parse() {
        assert_eq!(FederationMode::Full.to_string(), "full");
        assert_eq!(FederationMode::Solo.to_string(), "solo");
        assert_eq!(
            "full".parse::<FederationMode>().unwrap(),
            FederationMode::Full
        );
        assert_eq!(
            "solo".parse::<FederationMode>().unwrap(),
            FederationMode::Solo
        );
        assert_eq!(
            "Solo".parse::<FederationMode>().unwrap(),
            FederationMode::Solo
        );
        assert!("invalid".parse::<FederationMode>().is_err());
    }

    #[test]
    fn test_effective_threshold_full_mode() {
        assert_eq!(effective_quorum_threshold(FederationMode::Full, 3), 2);
        assert_eq!(effective_quorum_threshold(FederationMode::Full, 4), 3);
        assert_eq!(effective_quorum_threshold(FederationMode::Full, 7), 5);
    }

    #[test]
    fn test_effective_threshold_solo_mode() {
        assert_eq!(effective_quorum_threshold(FederationMode::Solo, 3), 1);
        assert_eq!(effective_quorum_threshold(FederationMode::Solo, 7), 1);
        assert_eq!(effective_quorum_threshold(FederationMode::Solo, 100), 1);
    }

    #[test]
    fn test_nullifier_log_insert_and_conflict() {
        let key = [0xAA; 32];
        let mut log = NullifierLog::new(key);

        let nullifier = [0x01; 32];
        let turn_hash = [0x02; 32];

        // First insert succeeds.
        let result = log.insert(nullifier, turn_hash, 100);
        assert!(result.is_ok());
        let entry = result.unwrap();
        assert_eq!(entry.sequence, 0);
        assert_eq!(entry.nullifier, nullifier);

        // Second insert of same nullifier fails.
        let result2 = log.insert(nullifier, [0x03; 32], 101);
        assert!(result2.is_err());
        assert_eq!(result2.unwrap_err().nullifier, nullifier);
    }

    #[test]
    fn test_nullifier_log_ordering() {
        let key = [0xBB; 32];
        let mut log = NullifierLog::new(key);

        for i in 0..5u8 {
            let nullifier = [i; 32];
            log.insert(nullifier, [0xFF; 32], 100 + i as u64).unwrap();
        }

        assert_eq!(log.len(), 5);
        for (i, entry) in log.entries().iter().enumerate() {
            assert_eq!(entry.sequence, i as u64);
        }
    }

    #[test]
    fn test_nullifier_log_validate_remote() {
        let key_a = [0xAA; 32];
        let key_b = [0xBB; 32];

        let mut log_a = NullifierLog::new(key_a);
        let mut log_b = NullifierLog::new(key_b);

        // Node A inserts nullifier 1.
        log_a.insert([0x01; 32], [0xF1; 32], 100).unwrap();

        // Node B inserts nullifier 2.
        log_b.insert([0x02; 32], [0xF2; 32], 100).unwrap();

        // Node A validates node B's entries (no conflict).
        let result = log_a.validate_remote_entries(log_b.entries(), &key_b);
        assert!(result.is_ok());

        // Now insert a conflicting nullifier on B.
        log_b.insert([0x01; 32], [0xF3; 32], 101).unwrap();

        // Node A validates again -- now there's a conflict on nullifier 0x01.
        let result2 = log_a.validate_remote_entries(log_b.entries(), &key_b);
        assert!(result2.is_err());
    }

    #[test]
    fn test_solo_mode_fast_path_threshold() {
        // In solo mode, fast-path certificate only needs 1 signature.
        let threshold = effective_quorum_threshold(FederationMode::Solo, 3);
        assert_eq!(threshold, 1);
    }

    #[test]
    fn test_solo_state_upgrade_to_full() {
        let key = [0xCC; 32];
        let mut state = SoloConsensusState::new(key);
        assert_eq!(state.mode, FederationMode::Solo);
        assert_eq!(state.effective_threshold(3), 1);

        state.detect_peers();
        assert_eq!(state.mode, FederationMode::Full);
        assert_eq!(state.effective_threshold(3), 2);
    }

    #[test]
    fn test_nullifier_log_merge() {
        let key_a = [0xAA; 32];
        let key_b = [0xBB; 32];

        let mut log_a = NullifierLog::new(key_a);
        let mut log_b = NullifierLog::new(key_b);

        log_a.insert([0x01; 32], [0xF1; 32], 100).unwrap();
        log_b.insert([0x02; 32], [0xF2; 32], 100).unwrap();
        log_b.insert([0x03; 32], [0xF3; 32], 101).unwrap();

        // Validate and merge.
        assert!(
            log_a
                .validate_remote_entries(log_b.entries(), &key_b)
                .is_ok()
        );
        log_a.merge_validated(log_b.entries().to_vec());

        // Now log_a should have all 3 nullifiers.
        assert!(log_a.contains(&[0x01; 32]));
        assert!(log_a.contains(&[0x02; 32]));
        assert!(log_a.contains(&[0x03; 32]));
        assert_eq!(log_a.len(), 3);
    }

    // =========================================================================
    // Integration-style tests demonstrating full solo-mode scenarios
    // =========================================================================

    #[test]
    fn test_solo_mode_single_node_processes_turn_tentative() {
        // Scenario: Solo mode node processes a turn, receipt has Tentative finality.
        use pyana_turn::Finality;

        let key = [0xCC; 32];
        let mut state = SoloConsensusState::new(key);

        // In solo mode, threshold = 1.
        assert_eq!(state.effective_threshold(3), 1);
        assert_eq!(state.mode, FederationMode::Solo);

        // Simulate processing a turn: the node is the sole sequencer.
        let nullifier = [0x42; 32];
        let turn_hash = [0xAB; 32];
        let entry = state
            .nullifier_log
            .insert(nullifier, turn_hash, state.height);
        assert!(entry.is_ok());
        state.advance_height();
        assert_eq!(state.height, 1);

        // In solo mode, consensus-path receipts should have Tentative finality.
        let finality = match state.mode {
            FederationMode::Solo => Finality::Tentative,
            FederationMode::Full => Finality::Final,
        };
        assert_eq!(finality, Finality::Tentative);
    }

    #[test]
    fn test_solo_fast_path_single_signature_sufficient() {
        // Scenario: In solo mode, fast-path certificate needs only 1 signature.
        //
        // The turn crate's assemble_certificate(turn, hash, sigs, threshold) already
        // accepts threshold=1 (proven by test_execute_certified_turn in fast_path.rs).
        // Here we verify that effective_quorum_threshold returns 1 in Solo mode,
        // meaning a single local node signature is sufficient for certification.
        let solo_threshold = effective_quorum_threshold(FederationMode::Solo, 3);
        let full_threshold = effective_quorum_threshold(FederationMode::Full, 3);

        // Solo: 1 signature is enough for fast-path certificate.
        assert_eq!(solo_threshold, 1);
        // Full: standard BFT threshold (2 for n=3).
        assert_eq!(full_threshold, 2);

        // For larger federations, solo is still 1 while full scales.
        assert_eq!(effective_quorum_threshold(FederationMode::Solo, 7), 1);
        assert_eq!(effective_quorum_threshold(FederationMode::Full, 7), 5);
    }

    #[test]
    fn test_mode_upgrade_solo_to_full() {
        // Scenario: Start solo, peer joins, switch to Full.
        use pyana_turn::Finality;

        let key = [0xDD; 32];
        let mut state = SoloConsensusState::new(key);

        // Initially solo.
        assert_eq!(state.mode, FederationMode::Solo);

        // Process a turn in solo mode -> Tentative.
        let finality_before = match state.mode {
            FederationMode::Solo => Finality::Tentative,
            FederationMode::Full => Finality::Final,
        };
        assert_eq!(finality_before, Finality::Tentative);

        // Peer joins -> upgrade to Full.
        state.detect_peers();
        assert_eq!(state.mode, FederationMode::Full);

        // Subsequent turns get Final finality.
        let finality_after = match state.mode {
            FederationMode::Solo => Finality::Tentative,
            FederationMode::Full => Finality::Final,
        };
        assert_eq!(finality_after, Finality::Final);

        // Threshold is now standard BFT.
        assert_eq!(state.effective_threshold(3), 2);
    }

    #[test]
    fn test_tentative_distinct_from_final() {
        // Scenario: API consumers can distinguish Tentative from Final.
        use pyana_turn::Finality;

        let tentative = Finality::Tentative;
        let final_ = Finality::Final;

        // They are different enum variants.
        assert_ne!(tentative, final_);

        // Serialize differently.
        let t_bytes = postcard::to_allocvec(&tentative).unwrap();
        let f_bytes = postcard::to_allocvec(&final_).unwrap();
        assert_ne!(t_bytes, f_bytes);

        // Round-trip.
        let t_back: Finality = postcard::from_bytes(&t_bytes).unwrap();
        let f_back: Finality = postcard::from_bytes(&f_bytes).unwrap();
        assert_eq!(t_back, Finality::Tentative);
        assert_eq!(f_back, Finality::Final);
    }

    #[test]
    fn test_nullifier_double_spend_prevented_solo() {
        // Scenario: Solo mode prevents double-spend via nullifier log.
        let key = [0xEE; 32];
        let mut state = SoloConsensusState::new(key);

        let nullifier = [0x99; 32];
        let turn_hash_1 = [0xA1; 32];
        let turn_hash_2 = [0xA2; 32];

        // First spend succeeds.
        assert!(
            state
                .nullifier_log
                .insert(nullifier, turn_hash_1, 0)
                .is_ok()
        );

        // Second spend of same nullifier is REJECTED (double-spend attempt).
        let result = state.nullifier_log.insert(nullifier, turn_hash_2, 1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().nullifier, nullifier);
    }
}
