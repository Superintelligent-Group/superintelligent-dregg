//! Multi-asset treasury balance ledger.
//!
//! The treasury tracks balances per [`AssetId`]. Spending in asset X debits
//! asset X only; it does NOT fall back to another asset. A debit that would
//! drive a balance negative is rejected with [`TreasuryError::Insufficient`].
//!
//! This module is intentionally minimal — it is a typed key/value ledger,
//! not a clearing system. Higher-level invariants (proposal gating, batch
//! atomicity) live in [`crate::executor`] and [`crate::governance`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 32-byte asset identifier (matches `pyana_intent::exchange::AssetId`).
pub type AssetId = [u8; 32];

/// Multi-asset balance ledger.
///
/// All operations are O(1) hashmap lookups; the structure is small enough to
/// snapshot in tests without any persistence layer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Treasury {
    balances: HashMap<AssetId, u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TreasuryError {
    #[error("insufficient balance for asset (have {have}, want {want})")]
    Insufficient { have: u128, want: u128 },
    #[error("overflow on credit (current {current}, amount {amount})")]
    Overflow { current: u128, amount: u128 },
    #[error("asset not held: {asset:?}")]
    UnknownAsset { asset: AssetId },
}

impl Treasury {
    pub fn new() -> Self {
        Self::default()
    }

    /// Credit `amount` to `asset`. Returns the new balance.
    pub fn credit(&mut self, asset: AssetId, amount: u128) -> Result<u128, TreasuryError> {
        let entry = self.balances.entry(asset).or_insert(0);
        let new = entry
            .checked_add(amount)
            .ok_or(TreasuryError::Overflow { current: *entry, amount })?;
        *entry = new;
        Ok(new)
    }

    /// Debit `amount` from `asset`. Returns the new balance or
    /// [`TreasuryError::Insufficient`] if the balance is too low. Never
    /// drives a balance negative.
    pub fn debit(&mut self, asset: AssetId, amount: u128) -> Result<u128, TreasuryError> {
        let entry = self.balances.entry(asset).or_insert(0);
        if *entry < amount {
            return Err(TreasuryError::Insufficient { have: *entry, want: amount });
        }
        *entry -= amount;
        Ok(*entry)
    }

    /// Read the current balance for `asset` (0 if unknown).
    pub fn balance(&self, asset: &AssetId) -> u128 {
        self.balances.get(asset).copied().unwrap_or(0)
    }

    /// Iterate over (asset, balance) tuples.
    pub fn iter(&self) -> impl Iterator<Item = (&AssetId, &u128)> {
        self.balances.iter()
    }

    /// Snapshot of all balances (cloned).
    pub fn snapshot(&self) -> HashMap<AssetId, u128> {
        self.balances.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: AssetId = [0xAA; 32];
    const B: AssetId = [0xBB; 32];

    #[test]
    fn credit_then_debit_balances_track_independently() {
        let mut t = Treasury::new();
        t.credit(A, 1000).unwrap();
        t.credit(B, 5000).unwrap();
        assert_eq!(t.balance(&A), 1000);
        assert_eq!(t.balance(&B), 5000);

        t.debit(A, 600).unwrap();
        assert_eq!(t.balance(&A), 400);
        assert_eq!(t.balance(&B), 5000, "debiting A must not touch B");
    }

    #[test]
    fn debit_insufficient_rejected_and_balance_unchanged() {
        let mut t = Treasury::new();
        t.credit(A, 100).unwrap();
        let err = t.debit(A, 200).unwrap_err();
        assert_eq!(err, TreasuryError::Insufficient { have: 100, want: 200 });
        // Adversarial: a rejected debit must leave the balance intact.
        assert_eq!(t.balance(&A), 100);
    }

    #[test]
    fn debit_unknown_asset_is_insufficient_not_silently_ok() {
        let mut t = Treasury::new();
        let err = t.debit(A, 1).unwrap_err();
        // An empty asset slot must report Insufficient, NOT silently succeed.
        assert!(matches!(err, TreasuryError::Insufficient { have: 0, .. }));
    }

    #[test]
    fn credit_overflow_rejected() {
        let mut t = Treasury::new();
        t.credit(A, u128::MAX).unwrap();
        let err = t.credit(A, 1).unwrap_err();
        assert!(matches!(err, TreasuryError::Overflow { .. }));
    }
}
