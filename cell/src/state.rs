use serde::{Deserialize, Serialize};

/// A generic 32-byte field element.
/// Could represent a BabyBear element, a BLAKE3 hash, a scalar, etc.
pub type FieldElement = [u8; 32];

/// The zero field element.
pub const FIELD_ZERO: FieldElement = [0u8; 32];

/// Number of user-defined state slots per cell.
pub const STATE_SLOTS: usize = 8;

/// The mutable state of an agent cell.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellState {
    /// 8 user-defined state fields (like Mina's app_state).
    pub fields: [FieldElement; STATE_SLOTS],
    /// Monotonically increasing action counter.
    pub nonce: u64,
    /// Computron balance (execution budget).
    pub balance: u64,
}

impl CellState {
    /// Create a fresh cell state with zero fields and the given balance.
    pub fn new(balance: u64) -> Self {
        CellState {
            fields: [FIELD_ZERO; STATE_SLOTS],
            nonce: 0,
            balance,
        }
    }

    /// Get a state field by index.
    pub fn get_field(&self, index: usize) -> Option<&FieldElement> {
        self.fields.get(index)
    }

    /// Set a state field by index.
    pub fn set_field(&mut self, index: usize, value: FieldElement) -> bool {
        if index < STATE_SLOTS {
            self.fields[index] = value;
            true
        } else {
            false
        }
    }

    /// Increment the nonce by 1.
    pub fn increment_nonce(&mut self) {
        self.nonce = self.nonce.wrapping_add(1);
    }

    /// Apply a balance change (positive or negative). Returns false on underflow.
    pub fn apply_balance_change(&mut self, delta: i64) -> bool {
        if delta >= 0 {
            match self.balance.checked_add(delta as u64) {
                Some(new_bal) => {
                    self.balance = new_bal;
                    true
                }
                None => false,
            }
        } else {
            let abs = delta.unsigned_abs();
            if self.balance >= abs {
                self.balance -= abs;
                true
            } else {
                false
            }
        }
    }
}

impl Default for CellState {
    fn default() -> Self {
        Self::new(0)
    }
}
