//! LedgerJournal: undo log for efficient atomic rollback.
//!
//! Instead of cloning the entire ledger before executing a turn, the journal
//! records each mutation's previous value as it happens. On success, the journal
//! is simply dropped (zero cost). On failure, the journal is replayed in reverse
//! to restore the ledger to its exact pre-turn state.

use pyana_cell::{
    CapabilityRef, CellId, Ledger,
    state::FieldElement,
};

/// A single undo entry in the journal.
#[derive(Debug)]
pub(crate) enum JournalEntry {
    /// A state field was overwritten. Records the old value.
    SetField {
        cell: CellId,
        index: usize,
        old_value: FieldElement,
    },
    /// A cell's balance was changed (by transfer or fee deduction).
    /// Records the old balance.
    SetBalance {
        cell: CellId,
        old_balance: u64,
    },
    /// A cell's nonce was incremented. Records the old nonce.
    SetNonce {
        cell: CellId,
        old_nonce: u64,
    },
    /// A capability was granted to a cell. Records the slot that was assigned,
    /// so we can revoke it on rollback.
    GrantCapability {
        cell: CellId,
        slot: u32,
    },
    /// A capability was revoked from a cell. Records the full capability
    /// so we can re-grant it on rollback.
    RevokeCapability {
        cell: CellId,
        old_cap: CapabilityRef,
    },
    /// A new cell was created. Records the cell ID so we can remove it on rollback.
    CreateCell {
        cell: CellId,
    },
}

/// The undo journal for a turn's execution.
#[derive(Debug)]
pub(crate) struct LedgerJournal {
    entries: Vec<JournalEntry>,
}

impl LedgerJournal {
    /// Create a new empty journal.
    #[allow(dead_code)]
    pub fn new() -> Self {
        LedgerJournal {
            entries: Vec::new(),
        }
    }

    /// Create a new journal with pre-allocated capacity.
    pub fn with_capacity(cap: usize) -> Self {
        LedgerJournal {
            entries: Vec::with_capacity(cap),
        }
    }

    /// Get a reference to the journal entries for inspection.
    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    /// Record a field change.
    pub fn record_set_field(&mut self, cell: CellId, index: usize, old_value: FieldElement) {
        self.entries.push(JournalEntry::SetField { cell, index, old_value });
    }

    /// Record a balance change.
    pub fn record_set_balance(&mut self, cell: CellId, old_balance: u64) {
        self.entries.push(JournalEntry::SetBalance { cell, old_balance });
    }

    /// Record a nonce change.
    pub fn record_set_nonce(&mut self, cell: CellId, old_nonce: u64) {
        self.entries.push(JournalEntry::SetNonce { cell, old_nonce });
    }

    /// Record a capability grant (so it can be revoked on rollback).
    pub fn record_grant_capability(&mut self, cell: CellId, slot: u32) {
        self.entries.push(JournalEntry::GrantCapability { cell, slot });
    }

    /// Record a capability revocation (so it can be re-granted on rollback).
    pub fn record_revoke_capability(&mut self, cell: CellId, old_cap: CapabilityRef) {
        self.entries.push(JournalEntry::RevokeCapability { cell, old_cap });
    }

    /// Record a cell creation (so it can be removed on rollback).
    pub fn record_create_cell(&mut self, cell: CellId) {
        self.entries.push(JournalEntry::CreateCell { cell });
    }

    /// Roll back all recorded changes in reverse order.
    ///
    /// After this call, the ledger is restored to the state it was in before
    /// any journaled mutations were applied.
    pub fn rollback(self, ledger: &mut Ledger) {
        for entry in self.entries.into_iter().rev() {
            match entry {
                JournalEntry::SetField { cell, index, old_value } => {
                    if let Some(c) = ledger.get_mut(&cell) {
                        c.state.fields[index] = old_value;
                    }
                }
                JournalEntry::SetBalance { cell, old_balance } => {
                    if let Some(c) = ledger.get_mut(&cell) {
                        c.state.balance = old_balance;
                    }
                }
                JournalEntry::SetNonce { cell, old_nonce } => {
                    if let Some(c) = ledger.get_mut(&cell) {
                        c.state.nonce = old_nonce;
                    }
                }
                JournalEntry::GrantCapability { cell, slot } => {
                    if let Some(c) = ledger.get_mut(&cell) {
                        c.capabilities.revoke(slot);
                    }
                }
                JournalEntry::RevokeCapability { cell, old_cap } => {
                    if let Some(c) = ledger.get_mut(&cell) {
                        // Re-insert the capability. We use grant_with_breadstuff
                        // but that assigns a new slot. Instead we need to restore
                        // the exact old state. We'll use a direct re-insert.
                        c.capabilities.restore(old_cap);
                    }
                }
                JournalEntry::CreateCell { cell } => {
                    ledger.remove(&cell);
                }
            }
        }
    }
}
