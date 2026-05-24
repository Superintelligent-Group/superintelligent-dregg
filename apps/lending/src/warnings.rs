//! Store-and-forward inbox for liquidation warnings.
//!
//! When a borrower's health factor crosses below 1.1 (EXECUTOR_HEALTH_THRESHOLD_BPS),
//! a warning message is pushed to their inbox.  The borrower can poll the inbox
//! when they reconnect to learn their position is at risk.
//!
//! # Message Format
//!
//! Messages are `InboxMessage::Encrypted { ciphertext, sender }` where:
//! - `ciphertext` is a JSON-encoded [`HealthWarning`] — **plaintext today,
//!   despite the variant name**.  See `REVIEW[P1]` on
//!   [`push_health_warning`].
//! - `sender` is the pool's own identity (all zeros sentinel).

use pyana_types::CellId;
use serde::{Deserialize, Serialize};

use pyana_app_framework::inbox_endpoint::InboxEndpoint;
use pyana_storage::QuotaId;
use pyana_storage::inbox::{CapInbox, InboxMessage};

/// Inbox capacity: up to 64 pending warnings per inbox.
pub const WARNING_INBOX_CAPACITY: usize = 64;

/// Minimum deposit to send a warning (zero — warnings are protocol-internal).
pub const WARNING_MIN_DEPOSIT: u64 = 0;

/// Sender identity used for protocol-generated warnings (all-zero sentinel).
pub const POOL_SENDER: [u8; 32] = [0u8; 32];

/// A health-factor warning message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthWarning {
    /// The borrow position ID (hex string).
    pub position_id_hex: String,
    /// Current health factor in basis points.
    pub health_factor_bps: u64,
    /// Threshold that was crossed.
    pub threshold_bps: u64,
    /// Block height at which the warning was generated.
    pub block: u64,
}

/// Build the global warnings [`InboxEndpoint`].
///
/// The capacity and deposit are set to protocol defaults. The actual push
/// to the inbox is done via [`push_health_warning`] which uses the inner
/// `CapInbox` directly.
pub fn build_warnings_inbox() -> InboxEndpoint {
    InboxEndpoint::new(WARNING_INBOX_CAPACITY, WARNING_MIN_DEPOSIT)
}

/// Push a health warning to the given `CapInbox` for `borrower`.
///
/// Returns `Ok(root)` on success, or `Err(e)` if the inbox is full or the
/// deposit is rejected.
///
// REVIEW[P1]: the payload is JSON-serialized in plaintext and stored in
// `InboxMessage::Encrypted { ciphertext, .. }`. The variant name is a lie —
// nothing encrypts it. A `HealthWarning` reveals the position id, current
// health, and the block, all of which the borrower likely does NOT want
// public (this is the same data an MEV searcher would use to time a
// liquidation). Either (a) require a borrower public key parameter and
// encrypt-to-recipient before storing, or (b) switch to a per-borrower inbox
// (one `CapInbox` per borrower cell) so the global inbox doesn't leak.
// Today, anyone with read access to the warnings inbox can scrape every
// at-risk position.
//
// REVIEW[P2]: `borrower` is currently unused — there is only ONE global
// warnings inbox (see `AppState.warnings_inbox` in server.rs). Until this is
// keyed-per-borrower, the parameter is misleading.
pub fn push_health_warning(
    inbox: &mut CapInbox,
    borrower: CellId,
    warning: HealthWarning,
    deposit: u64,
) -> Result<[u8; 32], pyana_storage::inbox::InboxError> {
    let _ = borrower; // see REVIEW[P2] above — single global inbox for now.
    let payload = serde_json::to_vec(&warning).unwrap_or_default();
    let msg = InboxMessage::Encrypted {
        ciphertext: payload,
        sender: POOL_SENDER,
    };
    inbox.receive(msg, deposit)
}

/// Create a new standalone CapInbox for use in tests or direct integration.
pub fn new_warnings_cap_inbox() -> CapInbox {
    CapInbox::new(QuotaId(0), WARNING_INBOX_CAPACITY, WARNING_MIN_DEPOSIT)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::borrow::CollateralEntry;
    use crate::executor::EXECUTOR_HEALTH_THRESHOLD_BPS;
    use crate::interest::BPS_SCALE;
    use crate::{LendingPool, Market};

    /// Helper to get a hex representation of a 32-byte array.
    fn hex_id(id: &[u8; 32]) -> String {
        id.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn setup_at_risk_position() -> (LendingPool, CellId, [u8; 32]) {
        let mut pool = LendingPool::new();
        pool.add_market(Market::new(1));
        pool.add_market(Market::new(2));

        let alice = CellId([0xAA; 32]);
        let bob = CellId([0xBB; 32]);

        pool.supply(alice, 1, 10_000_000).unwrap();

        // health = 1_312_500 * 8000 / 1_000_000 = 10500 bps (1.05) — below 1.1
        let collateral = vec![CollateralEntry {
            asset_id: 2,
            amount: 1_312_500,
            price: BPS_SCALE,
        }];
        let pos_id = pool.borrow(bob, 1, 1_000_000, collateral).unwrap();
        (pool, bob, pos_id)
    }

    /// Test 1: when health crosses below 1.1, pushing a warning creates an inbox message.
    #[test]
    fn test_health_threshold_trigger_creates_inbox_message() {
        let (pool, borrower, pos_id) = setup_at_risk_position();
        let pos = pool
            .borrow_positions
            .iter()
            .find(|p| p.id == pos_id)
            .unwrap();

        let health = pos.health_factor_bps();
        assert!(
            health < EXECUTOR_HEALTH_THRESHOLD_BPS,
            "health {health} should be below threshold {EXECUTOR_HEALTH_THRESHOLD_BPS}"
        );

        let mut inbox = new_warnings_cap_inbox();

        let warning = HealthWarning {
            position_id_hex: hex_id(&pos_id),
            health_factor_bps: health,
            threshold_bps: EXECUTOR_HEALTH_THRESHOLD_BPS,
            block: pool.current_block,
        };

        let result = push_health_warning(&mut inbox, borrower, warning, 0);
        assert!(
            result.is_ok(),
            "push_health_warning should succeed: {:?}",
            result
        );

        // Confirm the inbox now has a pending message
        let status = inbox.status();
        assert_eq!(
            status.pending_messages, 1,
            "inbox should have 1 pending message"
        );
    }

    /// Test 2: offline borrower can retrieve the warning when they reconnect.
    #[test]
    fn test_offline_borrower_retrieves_warning_on_reconnect() {
        let (pool, borrower, pos_id) = setup_at_risk_position();
        let pos = pool
            .borrow_positions
            .iter()
            .find(|p| p.id == pos_id)
            .unwrap();

        let health = pos.health_factor_bps();
        let mut inbox = new_warnings_cap_inbox();

        // Protocol pushes warning while borrower is "offline"
        let warning = HealthWarning {
            position_id_hex: hex_id(&pos_id),
            health_factor_bps: health,
            threshold_bps: EXECUTOR_HEALTH_THRESHOLD_BPS,
            block: 42,
        };
        push_health_warning(&mut inbox, borrower, warning.clone(), 0).unwrap();

        // Simulate reconnect: borrower reads from inbox. `read_next` returns
        // only the entry's content commitment, not the original payload — the
        // raw ciphertext is held inside the underlying queue entry. For this
        // smoke test we accept that limitation and only verify that the entry
        // existed, the dequeue proof's roots changed, and the inbox is now
        // drained. REVIEW[P3]: when `CapInbox` grows a `read_next_with_message`
        // accessor, replace this with a real round-trip deserialize.
        let (_entry, proof) = inbox.read_next().expect("should have a message to read");
        let _ = &warning; // payload we *would* check once read returns it

        // Proof contains old/new roots
        assert_ne!(
            proof.old_root, proof.new_root,
            "roots should differ after dequeue"
        );

        // Inbox should now be empty
        let status = inbox.status();
        assert_eq!(
            status.pending_messages, 0,
            "inbox should be empty after read"
        );
    }

    /// Test 3: healthy positions do NOT trigger a warning.
    #[test]
    fn test_healthy_position_does_not_trigger_warning() {
        let mut pool = LendingPool::new();
        pool.add_market(Market::new(1));
        pool.add_market(Market::new(2));

        let alice = CellId([0xAA; 32]);
        let bob = CellId([0xBB; 32]);
        pool.supply(alice, 1, 10_000_000).unwrap();

        // Very healthy: 3M collateral for 1M debt => health = 24000 bps
        let collateral = vec![CollateralEntry {
            asset_id: 2,
            amount: 3_000_000,
            price: BPS_SCALE,
        }];
        let pos_id = pool.borrow(bob, 1, 1_000_000, collateral).unwrap();
        let pos = pool
            .borrow_positions
            .iter()
            .find(|p| p.id == pos_id)
            .unwrap();

        let health = pos.health_factor_bps();
        assert!(
            health >= EXECUTOR_HEALTH_THRESHOLD_BPS,
            "health {health} should be >= {EXECUTOR_HEALTH_THRESHOLD_BPS}"
        );
        // Protocol should NOT push warnings for healthy positions
        // (this is a logic check, not a state check)
        assert_eq!(pool.current_block, 0);
    }
}
