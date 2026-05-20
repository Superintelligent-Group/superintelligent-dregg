//! Budget enforcement for token usage.
//!
//! The `BudgetEnforcer` integrates the audit log with a token's budget
//! specification to enforce usage limits. It tracks how many times a token
//! has been used and can prove the budget status to an auditor.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::event::{AuditReceipt, UsageEvent};
use crate::log::AuditLog;
use crate::proofs::BudgetProof;

/// Error returned when a token's budget is exhausted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetExhausted {
    /// The token that is exhausted.
    pub token_id: [u8; 32],
    /// The budget limit.
    pub budget_limit: u64,
    /// How many uses have been consumed.
    pub uses_consumed: u64,
    /// If windowed, the window that is full.
    pub window_info: Option<WindowInfo>,
}

/// Information about a budget window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Window duration in seconds.
    pub window_seconds: u64,
    /// Start of the current window (Unix timestamp).
    pub window_start: i64,
    /// End of the current window (Unix timestamp).
    pub window_end: i64,
    /// Uses within the current window.
    pub uses_in_window: u64,
}

/// Budget specification for a token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetSpec {
    /// Maximum number of uses allowed.
    pub limit: u64,
    /// Optional time window. If set, the budget resets at the end of each window.
    pub window: Option<Duration>,
}

impl BudgetSpec {
    /// Create a simple budget with a total limit and no window.
    pub fn total(limit: u64) -> Self {
        Self {
            limit,
            window: None,
        }
    }

    /// Create a windowed budget (limit per time window).
    pub fn windowed(limit: u64, window: Duration) -> Self {
        Self {
            limit,
            window: Some(window),
        }
    }
}

/// Enforces budget constraints on token usage, backed by an audit log.
///
/// The enforcer maintains an audit log and checks budget limits before
/// allowing new uses. It can generate proofs of budget status.
#[derive(Clone, Debug)]
pub struct BudgetEnforcer {
    /// The token this enforcer manages.
    pub token_id: [u8; 32],
    /// The budget specification.
    pub budget: BudgetSpec,
    /// The underlying audit log.
    pub log: AuditLog,
}

impl BudgetEnforcer {
    /// Create a new budget enforcer for a token.
    pub fn new(token_id: [u8; 32], budget: BudgetSpec) -> Self {
        Self {
            token_id,
            budget,
            log: AuditLog::new(),
        }
    }

    /// Create a budget enforcer with an existing audit log.
    pub fn with_log(token_id: [u8; 32], budget: BudgetSpec, log: AuditLog) -> Self {
        Self {
            token_id,
            budget,
            log,
        }
    }

    /// Check if the token can be used (budget not exhausted).
    ///
    /// For windowed budgets, only counts uses within the current window.
    /// The `now` parameter is the current timestamp for window calculation.
    pub fn can_use(&self, now: i64) -> bool {
        let used = self.uses_consumed(now);
        used < self.budget.limit
    }

    /// Get the number of remaining uses.
    pub fn remaining(&self, now: i64) -> u64 {
        let used = self.uses_consumed(now);
        self.budget.limit.saturating_sub(used)
    }

    /// Get the number of uses consumed (respecting window if applicable).
    pub fn uses_consumed(&self, now: i64) -> u64 {
        match self.budget.window {
            None => {
                // Total budget: count all uses ever.
                self.log.token_use_count(&self.token_id)
            }
            Some(window) => {
                // Windowed budget: count uses in current window.
                self.uses_in_window(now, window)
            }
        }
    }

    /// Record a use of the token, enforcing the budget.
    ///
    /// Returns an `AuditReceipt` on success, or `BudgetExhausted` if the
    /// budget would be exceeded.
    pub fn record_use(
        &mut self,
        event: UsageEvent,
    ) -> Result<AuditReceipt, BudgetExhausted> {
        // Verify the event is for our token.
        assert_eq!(
            event.token_id, self.token_id,
            "event token_id does not match enforcer token_id"
        );

        let now = event.timestamp;

        if !self.can_use(now) {
            let used = self.uses_consumed(now);
            let window_info = self.budget.window.map(|w| {
                let window_secs = w.as_secs();
                let window_start = self.window_start(now, w);
                WindowInfo {
                    window_seconds: window_secs,
                    window_start,
                    window_end: window_start + window_secs as i64,
                    uses_in_window: used,
                }
            });

            return Err(BudgetExhausted {
                token_id: self.token_id,
                budget_limit: self.budget.limit,
                uses_consumed: used,
                window_info,
            });
        }

        Ok(self.log.append(event))
    }

    /// Generate a proof of the current budget status.
    pub fn prove_budget_status(&mut self, now: i64) -> BudgetProof {
        let uses_consumed = self.uses_consumed(now);
        let remaining = self.budget.limit.saturating_sub(uses_consumed);

        let (windowed, window_start, window_end) = match self.budget.window {
            None => (false, None, None),
            Some(w) => {
                let ws = self.window_start(now, w);
                let we = ws + w.as_secs() as i64;
                (true, Some(ws), Some(we))
            }
        };

        // Generate count proof — for windowed budgets this is the count in the window.
        // We generate a count proof for all events of this token in the log.
        let count_proof = self.log.prove_count(&self.token_id);

        BudgetProof {
            token_id: self.token_id,
            budget_limit: self.budget.limit,
            uses_consumed,
            remaining,
            windowed,
            window_start,
            window_end,
            count_proof,
        }
    }

    /// Get the sequence number for the next event.
    pub fn next_sequence(&self) -> u64 {
        self.log.token_use_count(&self.token_id)
    }

    /// Get a reference to the underlying audit log.
    pub fn log(&self) -> &AuditLog {
        &self.log
    }

    /// Get a mutable reference to the underlying audit log.
    pub fn log_mut(&mut self) -> &mut AuditLog {
        &mut self.log
    }

    // ─── Internal helpers ───────────────────────────────────────────────

    /// Count uses within the current time window.
    fn uses_in_window(&self, now: i64, window: Duration) -> u64 {
        let window_start = self.window_start(now, window);
        let indices = self.log.token_event_indices(&self.token_id);
        let mut count = 0u64;

        for &idx in indices {
            if let Some(event) = self.log.get_event(idx) {
                if event.timestamp >= window_start {
                    count += 1;
                }
            }
        }

        count
    }

    /// Calculate the start of the current window.
    ///
    /// Windows are aligned to multiples of the window duration from epoch.
    fn window_start(&self, now: i64, window: Duration) -> i64 {
        let window_secs = window.as_secs() as i64;
        if window_secs == 0 {
            return now;
        }
        // Align to window boundaries.
        (now / window_secs) * window_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(token_id: [u8; 32], seq: u64, ts: i64) -> UsageEvent {
        let action = blake3::hash(format!("action-{seq}").as_bytes());
        UsageEvent::new(token_id, ts, *action.as_bytes(), [0xBB; 32], seq)
    }

    #[test]
    fn total_budget_basic() {
        let token = [1u8; 32];
        let mut enforcer = BudgetEnforcer::new(token, BudgetSpec::total(3));

        assert!(enforcer.can_use(1000));
        assert_eq!(enforcer.remaining(1000), 3);

        // Use once.
        let result = enforcer.record_use(make_event(token, 0, 1000));
        assert!(result.is_ok());
        assert_eq!(enforcer.remaining(1000), 2);

        // Use twice more.
        enforcer.record_use(make_event(token, 1, 1001)).unwrap();
        enforcer.record_use(make_event(token, 2, 1002)).unwrap();
        assert_eq!(enforcer.remaining(1003), 0);
        assert!(!enforcer.can_use(1003));

        // Fourth use should fail.
        let result = enforcer.record_use(make_event(token, 3, 1003));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.budget_limit, 3);
        assert_eq!(err.uses_consumed, 3);
    }

    #[test]
    fn windowed_budget() {
        let token = [1u8; 32];
        let window = Duration::from_secs(3600); // 1 hour.
        let mut enforcer = BudgetEnforcer::new(token, BudgetSpec::windowed(2, window));

        // Window starts at 0 (epoch-aligned).
        // Timestamps 0..3599 are in window 0.
        assert!(enforcer.can_use(100));
        enforcer.record_use(make_event(token, 0, 100)).unwrap();
        enforcer.record_use(make_event(token, 1, 200)).unwrap();

        // Budget exhausted in this window.
        assert!(!enforcer.can_use(300));
        let err = enforcer.record_use(make_event(token, 2, 300)).unwrap_err();
        assert_eq!(err.uses_consumed, 2);
        assert!(err.window_info.is_some());

        // Next window (timestamp 3600+) — budget resets.
        assert!(enforcer.can_use(3600));
        assert_eq!(enforcer.remaining(3600), 2);
        enforcer.record_use(make_event(token, 2, 3600)).unwrap();
        assert_eq!(enforcer.remaining(3600), 1);
    }

    #[test]
    fn budget_proof_generation() {
        let token = [1u8; 32];
        let mut enforcer = BudgetEnforcer::new(token, BudgetSpec::total(5));

        for i in 0..3 {
            enforcer.record_use(make_event(token, i, 1000 + i as i64)).unwrap();
        }

        let proof = enforcer.prove_budget_status(1003);
        assert_eq!(proof.budget_limit, 5);
        assert_eq!(proof.uses_consumed, 3);
        assert_eq!(proof.remaining, 2);
        assert!(!proof.windowed);
        assert!(proof.verify());
    }

    #[test]
    fn budget_enforcer_next_sequence() {
        let token = [1u8; 32];
        let mut enforcer = BudgetEnforcer::new(token, BudgetSpec::total(10));

        assert_eq!(enforcer.next_sequence(), 0);
        enforcer.record_use(make_event(token, 0, 1000)).unwrap();
        assert_eq!(enforcer.next_sequence(), 1);
        enforcer.record_use(make_event(token, 1, 1001)).unwrap();
        assert_eq!(enforcer.next_sequence(), 2);
    }

    #[test]
    fn multiple_tokens_independent_budgets() {
        let token_a = [1u8; 32];
        let token_b = [2u8; 32];

        let mut enforcer_a = BudgetEnforcer::new(token_a, BudgetSpec::total(2));
        let mut enforcer_b = BudgetEnforcer::new(token_b, BudgetSpec::total(3));

        // Use token A twice.
        enforcer_a.record_use(make_event(token_a, 0, 1000)).unwrap();
        enforcer_a.record_use(make_event(token_a, 1, 1001)).unwrap();
        assert!(!enforcer_a.can_use(1002));

        // Token B should still be usable.
        assert!(enforcer_b.can_use(1002));
        enforcer_b.record_use(make_event(token_b, 0, 1002)).unwrap();
        assert_eq!(enforcer_b.remaining(1002), 2);
    }

    #[test]
    fn windowed_budget_aligned_boundaries() {
        let token = [1u8; 32];
        let window = Duration::from_secs(100);
        let mut enforcer = BudgetEnforcer::new(token, BudgetSpec::windowed(1, window));

        // Window [0, 100): use at t=50.
        enforcer.record_use(make_event(token, 0, 50)).unwrap();
        assert!(!enforcer.can_use(60));

        // Window [100, 200): budget resets.
        assert!(enforcer.can_use(100));
        enforcer.record_use(make_event(token, 1, 150)).unwrap();
        assert!(!enforcer.can_use(160));

        // Window [200, 300): budget resets again.
        assert!(enforcer.can_use(200));
    }

    #[test]
    fn budget_exhausted_error_details() {
        let token = [1u8; 32];
        let window = Duration::from_secs(60);
        let mut enforcer = BudgetEnforcer::new(token, BudgetSpec::windowed(1, window));

        enforcer.record_use(make_event(token, 0, 30)).unwrap();
        let err = enforcer.record_use(make_event(token, 1, 45)).unwrap_err();

        assert_eq!(err.token_id, token);
        assert_eq!(err.budget_limit, 1);
        assert_eq!(err.uses_consumed, 1);

        let wi = err.window_info.unwrap();
        assert_eq!(wi.window_seconds, 60);
        assert_eq!(wi.window_start, 0);
        assert_eq!(wi.window_end, 60);
        assert_eq!(wi.uses_in_window, 1);
    }
}
