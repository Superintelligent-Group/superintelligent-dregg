//! Integration tests for the audit trail system.
//!
//! These tests demonstrate the end-to-end flow of token usage auditing,
//! including budget enforcement and privacy-preserving proofs.

use std::time::Duration;

use crate::budget::{BudgetEnforcer, BudgetSpec};
use crate::event::UsageEvent;
use crate::log::AuditLog;
use crate::proofs::BudgetProof;

/// Helper: create a usage event with a given token, sequence, and timestamp.
fn make_event(token_id: [u8; 32], seq: u64, ts: i64) -> UsageEvent {
    let action = blake3::hash(format!("action-{seq}").as_bytes());
    let verifier = blake3::hash(b"test-verifier");
    UsageEvent::new(token_id, ts, *action.as_bytes(), *verifier.as_bytes(), seq)
}

/// Helper: create a usage event with a specific action.
fn make_event_with_action(token_id: [u8; 32], seq: u64, ts: i64, action: &str) -> UsageEvent {
    let action_hash = blake3::hash(action.as_bytes());
    let verifier = blake3::hash(b"test-verifier");
    UsageEvent::new(
        token_id,
        ts,
        *action_hash.as_bytes(),
        *verifier.as_bytes(),
        seq,
    )
}

/// End-to-end demo: Token used 3 times, auditor requests count proof,
/// then 4th use exceeds budget and enforcement kicks in.
#[test]
fn end_to_end_budget_enforcement() {
    let token = blake3::hash(b"my-api-token");
    let token_id = *token.as_bytes();

    // Create a budget enforcer: 3 uses total.
    let mut enforcer = BudgetEnforcer::new(token_id, BudgetSpec::total(3));

    // === Token used 3 times ===
    let receipt1 = enforcer
        .record_use(make_event_with_action(token_id, 0, 1000, "read /api/data"))
        .expect("first use should succeed");
    let receipt2 = enforcer
        .record_use(make_event_with_action(token_id, 1, 1010, "write /api/data"))
        .expect("second use should succeed");
    let receipt3 = enforcer
        .record_use(make_event_with_action(
            token_id,
            2,
            1020,
            "delete /api/data",
        ))
        .expect("third use should succeed");

    // All receipts should have valid inclusion proofs.
    assert!(receipt1.inclusion_proof.verify(&receipt1.log_root_after));
    assert!(receipt2.inclusion_proof.verify(&receipt2.log_root_after));
    assert!(receipt3.inclusion_proof.verify(&receipt3.log_root_after));

    // Receipts should have sequential indices.
    assert_eq!(receipt1.global_index, 0);
    assert_eq!(receipt2.global_index, 1);
    assert_eq!(receipt3.global_index, 2);

    // === Auditor requests count proof ===
    // The auditor gets "3 uses" without seeing what actions were taken.
    let count_proof = enforcer.log_mut().prove_count(&token_id);
    assert_eq!(count_proof.count, 3);
    assert!(count_proof.verify());

    // The auditor does NOT see the action hashes — they only know the count.
    // The event proofs contain leaf hashes, not the actual event contents.

    // === Token used a 4th time — exceeds budget ===
    let result = enforcer.record_use(make_event_with_action(
        token_id,
        3,
        1030,
        "read /api/secret",
    ));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.budget_limit, 3);
    assert_eq!(err.uses_consumed, 3);

    // === Budget proof shows status ===
    let budget_proof = enforcer.prove_budget_status(1030);
    assert_eq!(budget_proof.budget_limit, 3);
    assert_eq!(budget_proof.uses_consumed, 3);
    assert_eq!(budget_proof.remaining, 0);
    assert!(budget_proof.verify());
}

/// End-to-end demo: Consistency proof shows log wasn't tampered.
#[test]
fn end_to_end_consistency_proof() {
    let token = blake3::hash(b"audit-token");
    let token_id = *token.as_bytes();

    let mut log = AuditLog::new();

    // Append 3 events and take a snapshot.
    for i in 0..3 {
        log.append(make_event(token_id, i, 1000 + i as i64));
    }
    let snapshot_at_3 = log.snapshot();

    // Append 4 more events.
    for i in 3..7 {
        log.append(make_event(token_id, i, 1000 + i as i64));
    }

    // Generate consistency proof.
    let consistency_proof = log.prove_consistency(&snapshot_at_3).unwrap();

    // Verify structural consistency.
    assert!(consistency_proof.verify_structure());
    assert_eq!(consistency_proof.old_root, snapshot_at_3.root);
    assert_eq!(consistency_proof.old_size, 3);
    assert_eq!(consistency_proof.new_size, 7);

    // The proof demonstrates that the old tree is a prefix of the new tree.
    // No events were removed or modified — only new events were appended.
}

/// End-to-end demo: Range proof shows all uses are within a time window.
#[test]
fn end_to_end_range_proof() {
    let token = blake3::hash(b"time-bounded-token");
    let token_id = *token.as_bytes();

    let mut log = AuditLog::new();

    // Use the token several times within a 1-hour window.
    let base_time = 1_700_000_000i64; // Some Unix timestamp.
    for i in 0..5 {
        let ts = base_time + (i * 600); // Every 10 minutes.
        log.append(make_event(token_id, i as u64, ts));
    }

    // Prove all uses are within [base_time, base_time + 3600].
    let range_proof = log.prove_range(&token_id, base_time, base_time + 3600);
    assert!(range_proof.verify());
    assert_eq!(range_proof.count(), 5);

    // A tighter range should fail if events are outside it.
    let tight_proof = log.prove_range(&token_id, base_time + 700, base_time + 3600);
    // Event at base_time + 0 and base_time + 600 are outside [base_time + 700, ...].
    assert!(!tight_proof.verify());
}

/// End-to-end demo: Multiple tokens with different budgets.
#[test]
fn end_to_end_multi_token() {
    let token_a_hash = blake3::hash(b"token-alice");
    let token_b_hash = blake3::hash(b"token-bob");
    let token_a = *token_a_hash.as_bytes();
    let token_b = *token_b_hash.as_bytes();

    let mut enforcer_a = BudgetEnforcer::new(token_a, BudgetSpec::total(5));
    let mut enforcer_b =
        BudgetEnforcer::new(token_b, BudgetSpec::windowed(2, Duration::from_secs(60)));

    // Alice uses her token 3 times.
    for i in 0..3 {
        enforcer_a
            .record_use(make_event(token_a, i, 1000 + i as i64))
            .unwrap();
    }

    // Bob uses his token 2 times in the first window.
    enforcer_b.record_use(make_event(token_b, 0, 10)).unwrap();
    enforcer_b.record_use(make_event(token_b, 1, 30)).unwrap();

    // Bob's budget is exhausted in this window.
    assert!(!enforcer_b.can_use(45));
    let err = enforcer_b
        .record_use(make_event(token_b, 2, 45))
        .unwrap_err();
    assert_eq!(err.budget_limit, 2);

    // But Bob can use it in the next window.
    assert!(enforcer_b.can_use(60));
    enforcer_b.record_use(make_event(token_b, 2, 60)).unwrap();

    // Alice still has budget.
    assert_eq!(enforcer_a.remaining(1003), 2);

    // Generate budget proofs for both.
    let proof_a = enforcer_a.prove_budget_status(1003);
    assert!(proof_a.verify());
    assert_eq!(proof_a.uses_consumed, 3);
    assert_eq!(proof_a.remaining, 2);

    // Bob's proof in the new window shows 1 use.
    let proof_b = enforcer_b.prove_budget_status(60);
    // Note: the count proof counts ALL events in the log, not just windowed ones.
    // The budget proof's uses_consumed reflects the windowed count.
    assert_eq!(proof_b.uses_consumed, 1);
    assert_eq!(proof_b.remaining, 1);
}

/// Test: receipts from different events are all independently verifiable.
#[test]
fn receipts_independently_verifiable() {
    let token = [0x42; 32];
    let mut log = AuditLog::new();

    let mut receipts = Vec::new();
    for i in 0..10 {
        let event = make_event(token, i, 1000 + i as i64);
        let receipt = log.append(event);
        receipts.push(receipt);
    }

    // Each receipt is verifiable against the root it was issued with.
    for receipt in &receipts {
        assert!(
            receipt.inclusion_proof.verify(&receipt.log_root_after),
            "Receipt at index {} failed verification",
            receipt.global_index
        );
    }

    // The current root is the same as the last receipt's root.
    let current_root = log.root();
    assert_eq!(current_root, receipts.last().unwrap().log_root_after);
}

/// Test: log maintains consistent historical roots.
#[test]
fn historical_roots_consistent() {
    let token = [0x99; 32];
    let mut log = AuditLog::new();

    let mut expected_roots = Vec::new();
    for i in 0..8 {
        let receipt = log.append(make_event(token, i, 1000 + i as i64));
        expected_roots.push(receipt.log_root_after);
    }

    // Historical roots should match receipt roots.
    for (i, expected) in expected_roots.iter().enumerate() {
        let historical = log.historical_root(i + 1).unwrap();
        assert_eq!(
            historical,
            *expected,
            "Historical root mismatch at size {}",
            i + 1
        );
    }
}

/// Test: count proof is invalid if you try to claim fewer uses.
#[test]
fn count_proof_tamper_detection() {
    let token = [0xAA; 32];
    let mut log = AuditLog::new();

    for i in 0..5 {
        log.append(make_event(token, i, 1000 + i as i64));
    }

    let mut proof = log.prove_count(&token);
    assert!(proof.verify());

    // Tamper: claim only 2 uses by removing proofs.
    proof.event_proofs.truncate(2);
    proof.count = 2;
    // This should fail because the index commitment won't match.
    assert!(!proof.verify());
}

/// Test: consistency proof across multiple snapshots.
#[test]
fn multi_snapshot_consistency() {
    let token = [0xCC; 32];
    let mut log = AuditLog::new();

    // Take snapshots at different sizes.
    let mut snapshots = Vec::new();

    for i in 0..12 {
        log.append(make_event(token, i, 1000 + i as i64));
        if i % 3 == 2 {
            snapshots.push(log.snapshot());
        }
    }

    // Each earlier snapshot should be consistent with the final state.
    for snapshot in &snapshots {
        let proof = log.prove_consistency(snapshot).unwrap();
        assert!(
            proof.verify_structure(),
            "Consistency proof failed for snapshot at size {}",
            snapshot.size
        );
    }
}

/// Test: last use proof correctness.
#[test]
fn last_use_proof_tracks_most_recent() {
    let token = [0xDD; 32];
    let mut log = AuditLog::new();

    for i in 0..5 {
        log.append(make_event(token, i, 1000 + i as i64 * 100));
    }

    let proof = log.prove_last_use(&token).unwrap();
    assert_eq!(proof.last_sequence, 4);
    assert_eq!(proof.last_timestamp, 1400);
    assert!(proof.verify());
}

/// Test: interleaved tokens don't interfere.
#[test]
fn interleaved_tokens() {
    let token_a = [0x11; 32];
    let token_b = [0x22; 32];
    let token_c = [0x33; 32];
    let mut log = AuditLog::new();

    // Interleave events from different tokens.
    log.append(make_event(token_a, 0, 100));
    log.append(make_event(token_b, 0, 101));
    log.append(make_event(token_a, 1, 102));
    log.append(make_event(token_c, 0, 103));
    log.append(make_event(token_b, 1, 104));
    log.append(make_event(token_a, 2, 105));
    log.append(make_event(token_c, 1, 106));

    // Count proofs should be correct for each token.
    let count_a = log.prove_count(&token_a);
    let count_b = log.prove_count(&token_b);
    let count_c = log.prove_count(&token_c);

    assert_eq!(count_a.count, 3);
    assert_eq!(count_b.count, 2);
    assert_eq!(count_c.count, 2);

    assert!(count_a.verify());
    assert!(count_b.verify());
    assert!(count_c.verify());
}

/// Test: budget proof with windowed budget across multiple windows.
#[test]
fn windowed_budget_proof_across_windows() {
    let token = [0xEE; 32];
    let window = Duration::from_secs(100);
    let mut enforcer = BudgetEnforcer::new(token, BudgetSpec::windowed(3, window));

    // Window [0, 100): use 2 times.
    enforcer.record_use(make_event(token, 0, 10)).unwrap();
    enforcer.record_use(make_event(token, 1, 50)).unwrap();

    let proof_w0 = enforcer.prove_budget_status(90);
    assert_eq!(proof_w0.uses_consumed, 2);
    assert_eq!(proof_w0.remaining, 1);
    assert!(proof_w0.windowed);

    // Window [100, 200): use 3 times (full budget).
    enforcer.record_use(make_event(token, 2, 110)).unwrap();
    enforcer.record_use(make_event(token, 3, 130)).unwrap();
    enforcer.record_use(make_event(token, 4, 150)).unwrap();

    let proof_w1 = enforcer.prove_budget_status(170);
    assert_eq!(proof_w1.uses_consumed, 3);
    assert_eq!(proof_w1.remaining, 0);
    assert!(!enforcer.can_use(180));

    // Window [200, 300): budget resets.
    assert!(enforcer.can_use(200));
    assert_eq!(enforcer.remaining(200), 3);
}

/// Test: empty log produces valid proofs.
#[test]
fn empty_log_proofs() {
    let token = [0xFF; 32];
    let mut log = AuditLog::new();

    let count_proof = log.prove_count(&token);
    assert_eq!(count_proof.count, 0);
    assert!(count_proof.verify());

    let range_proof = log.prove_range(&token, 0, i64::MAX);
    assert!(range_proof.verify());
    assert_eq!(range_proof.count(), 0);

    assert!(log.prove_last_use(&token).is_none());
}

/// Stress test: many events, proofs still work.
#[test]
fn stress_many_events() {
    let token = [0x77; 32];
    let mut log = AuditLog::new();

    for i in 0..100 {
        log.append(make_event(token, i, 1000 + i as i64));
    }

    // Root should be stable.
    let root = log.root();
    assert_eq!(log.root(), root);

    // Inclusion proofs for sampled events.
    for i in [0u64, 25, 50, 75, 99] {
        let proof = log.prove_inclusion(i).unwrap();
        assert!(proof.verify(&root), "Failed at index {i}");
    }

    // Count proof.
    let count_proof = log.prove_count(&token);
    assert_eq!(count_proof.count, 100);
    assert!(count_proof.verify());
}

/// Test: the audit system provides non-repudiation.
/// Once a receipt is issued, the event cannot be denied.
#[test]
fn non_repudiation() {
    let token = [0x88; 32];
    let mut log = AuditLog::new();

    // Issue a receipt.
    let event = make_event(token, 0, 1000);
    let receipt = log.append(event.clone());

    // The receipt is a commitment: it ties the event hash to a specific log state.
    assert_eq!(receipt.event_hash, event.hash());

    // Even if more events are added, the receipt remains valid at its recorded root.
    for i in 1..10 {
        log.append(make_event(token, i, 1000 + i as i64));
    }

    // Receipt is still valid (against its own root, not current root).
    assert!(receipt.inclusion_proof.verify(&receipt.log_root_after));

    // And we can prove consistency between the receipt's root and the current state.
    let old_snapshot = crate::log::LogSnapshot {
        root: receipt.log_root_after,
        size: 1,
    };
    let consistency = log.prove_consistency(&old_snapshot).unwrap();
    assert!(consistency.verify_structure());
}

/// Test: BudgetProof with zero budget.
#[test]
fn zero_budget() {
    let token = [0x44; 32];
    let mut enforcer = BudgetEnforcer::new(token, BudgetSpec::total(0));

    // Can't use at all.
    assert!(!enforcer.can_use(1000));
    assert_eq!(enforcer.remaining(1000), 0);

    let err = enforcer.record_use(make_event(token, 0, 1000)).unwrap_err();
    assert_eq!(err.budget_limit, 0);
    assert_eq!(err.uses_consumed, 0);
}

/// Test: Large budget is not exhausted prematurely.
#[test]
fn large_budget() {
    let token = [0x55; 32];
    let mut enforcer = BudgetEnforcer::new(token, BudgetSpec::total(u64::MAX));

    for i in 0..50 {
        enforcer
            .record_use(make_event(token, i, 1000 + i as i64))
            .unwrap();
    }

    assert!(enforcer.can_use(2000));
    assert_eq!(enforcer.remaining(2000), u64::MAX - 50);
}
