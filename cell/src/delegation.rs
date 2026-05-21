//! Snapshot+refresh delegation model for capability inheritance.
//!
//! In E-style delegation, a child cell inherits its parent's capabilities as a
//! SNAPSHOT. The child can act offline using the snapshot, and periodically
//! refreshes to pick up new capabilities. Revocation is eventual, bounded by
//! `max_staleness` — acceptors may reject stale snapshots at verification time.

use serde::{Deserialize, Serialize};

use crate::capability::CapabilityRef;
use crate::id::CellId;

/// A delegated capability snapshot from a parent cell.
///
/// This represents the E-style delegation model: the child receives a point-in-time
/// copy of the parent's c-list. The child can act using this snapshot without
/// contacting the parent. Freshness is checked by acceptors (remote verifiers),
/// not by the executor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedRef {
    /// The parent cell this delegation comes from.
    pub source: CellId,
    /// Snapshot of capabilities inherited from parent.
    pub snapshot: Vec<CapabilityRef>,
    /// Parent's delegation epoch when this snapshot was taken.
    pub delegation_epoch: u64,
    /// Timestamp when this snapshot was last refreshed.
    pub refreshed_at: u64,
    /// Maximum acceptable staleness (seconds). Acceptors may reject
    /// if `now - refreshed_at > max_staleness`. Zero means "always refresh."
    pub max_staleness: u64,
}

impl DelegatedRef {
    /// Create a new delegated reference.
    pub fn new(
        source: CellId,
        snapshot: Vec<CapabilityRef>,
        delegation_epoch: u64,
        refreshed_at: u64,
        max_staleness: u64,
    ) -> Self {
        DelegatedRef {
            source,
            snapshot,
            delegation_epoch,
            refreshed_at,
            max_staleness,
        }
    }

    /// Check if this delegation is stale relative to the given timestamp.
    ///
    /// A staleness of zero means "always stale" (always refresh before use).
    /// Otherwise, the delegation is stale if `now - refreshed_at > max_staleness`.
    pub fn is_stale(&self, now: u64) -> bool {
        if self.max_staleness == 0 {
            return true; // always stale = always refresh
        }
        now.saturating_sub(self.refreshed_at) > self.max_staleness
    }

    /// Check if a specific capability is available in the snapshot.
    pub fn has_capability(&self, target: &CellId) -> bool {
        self.snapshot.iter().any(|cap| &cap.target == target)
    }

    /// Get capabilities for a specific target from the snapshot.
    pub fn capabilities_for(&self, target: &CellId) -> Vec<&CapabilityRef> {
        self.snapshot
            .iter()
            .filter(|cap| &cap.target == target)
            .collect()
    }

    /// Number of capabilities in this snapshot.
    pub fn len(&self) -> usize {
        self.snapshot.len()
    }

    /// Whether the snapshot is empty.
    pub fn is_empty(&self) -> bool {
        self.snapshot.is_empty()
    }
}
