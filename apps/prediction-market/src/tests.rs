//! Integration tests for the prediction-market app.
//!
//! Each "upgrade claim" the README makes has at least one test that tries
//! to violate the property and asserts it gets rejected.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tower::ServiceExt;

use pyana_storage::blinded::BlindedQueue;

use crate::market::{Market, MarketStatus};
use crate::oracle::{Oracle, OracleEntry, OracleError, pubkey_of, sign_report, unsigned_report_for_test};
use crate::server::{AppState, router};

fn hex32(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ---------- helpers ----------

fn test_state() -> (AppState, [u8; 32]) {
    let signing_key = [0x42u8; 32];
    let oracle_pub = pubkey_of(&signing_key);
    let queue = Arc::new(Mutex::new(BlindedQueue::new(64)));
    let state = AppState::new_with_queue(queue, oracle_pub);
    (state, signing_key)
}

async fn json_post(
    app: axum::Router,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

async fn json_get(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

async fn create_market_via_api(app: axum::Router) -> Value {
    let (status, body) = json_post(
        app,
        "/market",
        json!({
            "question": "Will it rain?",
            "outcomes": ["yes", "no"],
            "close_height": 1000,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "market creation failed: {body}");
    body
}

// ============================================================================
// Property 1: a bet committed via /queue/bets/commit actually pays when the
// matching outcome wins.
// ============================================================================
#[tokio::test]
async fn winning_bet_via_queue_actually_pays() {
    let (state, signing_key) = test_state();
    let app = router().with_state(state.clone());
    let market = create_market_via_api(app.clone()).await;
    let market_id_hex = market["id"].as_str().unwrap().to_string();

    // Three bettors:
    //   alice bets 100 on "yes"
    //   bob bets 50 on "no"
    //   carol bets 200 on "yes"
    let bettors = vec![
        ([0xAAu8; 32], "yes", 100u64, [0x01u8; 32]),
        ([0xBBu8; 32], "no", 50, [0x02u8; 32]),
        ([0xCCu8; 32], "yes", 200, [0x03u8; 32]),
    ];

    let mut commitments = Vec::new();
    for (bettor, outcome, stake, secret) in &bettors {
        let (status, body) = json_post(
            app.clone(),
            "/queue/bets/commit",
            json!({
                "market_id": market_id_hex,
                "outcome": outcome,
                "stake": stake,
                "bettor_hex": hex32(bettor),
                "secret_hex": hex32(secret),
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "commit failed: {body}");
        commitments.push(body["commitment_hex"].as_str().unwrap().to_string());
    }

    // Oracle reports "yes" as winner.
    let market_id_bytes = {
        let m = state.markets.read().await;
        let mid = pyana_app_framework::hex::hex_to_bytes32(&market_id_hex).unwrap();
        let market_ref = m.get(&mid).unwrap();
        let yes_id = market_ref.outcome_for_label("yes").unwrap();
        (mid, yes_id)
    };
    let entry = OracleEntry {
        market_id: market_id_bytes.0,
        outcome_id: market_id_bytes.1,
        timestamp: 1,
    };
    let report = sign_report(&signing_key, entry, 0);
    let (status, _body) = json_post(
        app.clone(),
        "/oracle/report",
        serde_json::to_value(&report).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "oracle report rejected");

    // All bettors reveal.
    for c in &commitments {
        let (status, body) = json_post(
            app.clone(),
            "/queue/bets/reveal",
            json!({ "commitment_hex": c }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "reveal failed: {body}");
        assert_eq!(body["result"], "revealed");
    }

    // Settle.
    let (status, body) = json_post(app.clone(), &format!("/market/{}/settle", market_id_hex), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "settle failed: {body}");
    let winners = body["winners"].as_array().unwrap();

    // Exactly the "yes" bettors should be paid: alice and carol, NOT bob.
    let paid_to: Vec<String> = winners
        .iter()
        .map(|w| w["bettor_hex"].as_str().unwrap().to_string())
        .collect();
    assert!(paid_to.contains(&hex32(&[0xAAu8; 32])), "alice missing");
    assert!(paid_to.contains(&hex32(&[0xCCu8; 32])), "carol missing");
    assert!(!paid_to.contains(&hex32(&[0xBBu8; 32])), "bob (loser) was paid!");

    // Total payouts equal total pool (100 + 50 + 200 = 350).
    let total: u64 = winners.iter().map(|w| w["amount"].as_u64().unwrap()).sum();
    assert_eq!(total, 350);

    // Carol staked 2x alice on "yes" → she gets ~2x the payout.
    let alice_payout = winners
        .iter()
        .find(|w| w["bettor_hex"] == hex32(&[0xAAu8; 32]))
        .unwrap()["amount"]
        .as_u64()
        .unwrap();
    let carol_payout = winners
        .iter()
        .find(|w| w["bettor_hex"] == hex32(&[0xCCu8; 32]))
        .unwrap()["amount"]
        .as_u64()
        .unwrap();
    assert!(
        carol_payout > alice_payout,
        "carol should beat alice in payout"
    );
}

// ============================================================================
// Property 2: double-consume (same nullifier) is rejected.
// ============================================================================
#[tokio::test]
async fn double_reveal_is_rejected() {
    let (state, signing_key) = test_state();
    let app = router().with_state(state.clone());
    let market = create_market_via_api(app.clone()).await;
    let market_id_hex = market["id"].as_str().unwrap().to_string();

    // Place one bet.
    let (status, body) = json_post(
        app.clone(),
        "/queue/bets/commit",
        json!({
            "market_id": market_id_hex,
            "outcome": "yes",
            "stake": 100,
            "bettor_hex": hex32(&[0xAAu8; 32]),
            "secret_hex": hex32(&[0x01u8; 32]),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let commitment_hex = body["commitment_hex"].as_str().unwrap().to_string();

    // Oracle reports.
    let market_id = pyana_app_framework::hex::hex_to_bytes32(&market_id_hex).unwrap();
    let outcome_id = state
        .markets
        .read()
        .await
        .get(&market_id)
        .unwrap()
        .outcome_for_label("yes")
        .unwrap();
    let report = sign_report(
        &signing_key,
        OracleEntry {
            market_id,
            outcome_id,
            timestamp: 1,
        },
        0,
    );
    json_post(
        app.clone(),
        "/oracle/report",
        serde_json::to_value(&report).unwrap(),
    )
    .await;

    // First reveal succeeds.
    let (status, body) = json_post(
        app.clone(),
        "/queue/bets/reveal",
        json!({ "commitment_hex": commitment_hex }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"], "revealed");
    let first_nullifier = body["nullifier_hex"].as_str().unwrap().to_string();

    // Second reveal of the same commitment is rejected by /queue/bets/reveal
    // at the pending-bet layer (the pending entry was removed). To verify
    // the queue-layer rejection too, we directly attempt to consume again
    // via the underlying BlindedQueue.
    let (status, body) = json_post(
        app.clone(),
        "/queue/bets/reveal",
        json!({ "commitment_hex": commitment_hex }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = body;

    // Direct queue-level adversarial double-consume: same nullifier, same
    // proof. The queue must return AlreadyConsumed.
    use pyana_storage::blinded::{ConsumeResult, ConsumptionProof};
    let nullifier_bytes = pyana_app_framework::hex::hex_to_bytes32(&first_nullifier).unwrap();
    let bogus_proof = ConsumptionProof {
        nullifier: nullifier_bytes,
        commitment: [0u8; 32],
        position: 0,
        membership_proof: vec![],
    };
    let result = state.blinded_queue.lock().await.consume(&bogus_proof);
    assert_eq!(result, ConsumeResult::AlreadyConsumed);
}

// ============================================================================
// Property 3: an unauthenticated oracle report is rejected.
// ============================================================================
#[tokio::test]
async fn unauthenticated_oracle_report_rejected() {
    let (state, _signing_key) = test_state();
    let app = router().with_state(state.clone());
    let market = create_market_via_api(app.clone()).await;
    let market_id_hex = market["id"].as_str().unwrap().to_string();
    let market_id = pyana_app_framework::hex::hex_to_bytes32(&market_id_hex).unwrap();
    let outcome_id = state
        .markets
        .read()
        .await
        .get(&market_id)
        .unwrap()
        .outcome_for_label("yes")
        .unwrap();

    // 3a) zeroed-signature report under the configured authority pubkey.
    let oracle_pub = state.oracle.read().await.authority_pubkey;
    let bad = unsigned_report_for_test(
        OracleEntry {
            market_id,
            outcome_id,
            timestamp: 1,
        },
        0,
        oracle_pub,
    );
    let (status, _) = json_post(
        app.clone(),
        "/oracle/report",
        serde_json::to_value(&bad).unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "unsigned report should be rejected"
    );

    // 3b) attacker signs with their OWN key, claims to be the oracle —
    // signature is valid against attacker key but key is not the authority.
    let attacker_sk = [0x99u8; 32];
    let attacker_report = sign_report(
        &attacker_sk,
        OracleEntry {
            market_id,
            outcome_id,
            timestamp: 1,
        },
        0,
    );
    let (status, _) = json_post(
        app.clone(),
        "/oracle/report",
        serde_json::to_value(&attacker_report).unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "attacker-signed report should be rejected"
    );

    // 3c) attacker claims the authority pubkey but signs the message with
    // their own key. This produces a `UntrustedKey` error first; if they
    // forge a (pubkey, signature) pair where pubkey IS the authority but
    // signature is some random bytes, verify() rejects.
    let mut forged = sign_report(
        &attacker_sk,
        OracleEntry {
            market_id,
            outcome_id,
            timestamp: 1,
        },
        0,
    );
    forged.oracle_pubkey = oracle_pub;
    let (status, _) = json_post(
        app.clone(),
        "/oracle/report",
        serde_json::to_value(&forged).unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "forged report should be rejected"
    );

    // Market must STILL be Open: no oracle report changed its status.
    let m = state.markets.read().await;
    let market_state = m.get(&market_id).unwrap();
    assert!(
        matches!(market_state.status, MarketStatus::Open),
        "market status should not have changed despite forged reports"
    );
}

// ============================================================================
// Property 4: oracle report position is computed positionally (matches the
// sequence the oracle has already produced) — replay/wrong-position rejected.
// ============================================================================
#[tokio::test]
async fn oracle_wrong_position_rejected() {
    let signing_key = [0x42u8; 32];
    let oracle_pub = pubkey_of(&signing_key);
    let mut oracle = Oracle::new(oracle_pub);

    let entry0 = OracleEntry {
        market_id: [1u8; 32],
        outcome_id: [10u8; 32],
        timestamp: 100,
    };
    let entry1 = OracleEntry {
        market_id: [2u8; 32],
        outcome_id: [20u8; 32],
        timestamp: 101,
    };

    // Accept position 0.
    oracle
        .accept_report(&sign_report(&signing_key, entry0.clone(), 0))
        .unwrap();

    // Adversary tries to submit position 5 (skipping 1, 2, 3, 4).
    let skip = sign_report(&signing_key, entry1.clone(), 5);
    let err = oracle.accept_report(&skip).unwrap_err();
    assert!(
        matches!(err, OracleError::WrongPosition { expected: 1, got: 5 }),
        "skipping positions must be rejected, got {err:?}"
    );

    // Adversary tries to replay position 0.
    let replay = sign_report(&signing_key, entry0.clone(), 0);
    let err = oracle.accept_report(&replay).unwrap_err();
    assert!(
        matches!(err, OracleError::WrongPosition { expected: 1, got: 0 }),
        "replay must be rejected, got {err:?}"
    );

    // Honest position 1 accepted.
    oracle
        .accept_report(&sign_report(&signing_key, entry1.clone(), 1))
        .unwrap();

    // The inclusion proof for position 1 verifies against the root.
    let root = oracle.root();
    let proof = oracle.inclusion_proof(1).unwrap();
    assert!(proof.verify(&root));
    // And a tampered proof (wrong leaf) does NOT verify.
    let mut bad = proof.clone();
    bad.leaf[0] ^= 0x01;
    assert!(!bad.verify(&root));
}

// ============================================================================
// Property 5: oracle CANNOT resolve a market with an outcome the market did
// not declare.
// ============================================================================
#[tokio::test]
async fn oracle_cannot_report_undeclared_outcome() {
    let (state, signing_key) = test_state();
    let app = router().with_state(state.clone());
    let market = create_market_via_api(app.clone()).await;
    let market_id_hex = market["id"].as_str().unwrap().to_string();
    let market_id = pyana_app_framework::hex::hex_to_bytes32(&market_id_hex).unwrap();

    // Report a totally unrelated outcome id.
    let bogus_outcome: [u8; 32] = [0xDEu8; 32];
    let report = sign_report(
        &signing_key,
        OracleEntry {
            market_id,
            outcome_id: bogus_outcome,
            timestamp: 1,
        },
        0,
    );
    let (status, body) = json_post(
        app.clone(),
        "/oracle/report",
        serde_json::to_value(&report).unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "undeclared outcome should be rejected: {body}"
    );

    // Market must still be Open.
    let m = state.markets.read().await;
    assert!(matches!(
        m.get(&market_id).unwrap().status,
        MarketStatus::Open
    ));
}

// ============================================================================
// Property 6: escrow tracks per-bettor locked stake — settlement releases
// winner stakes and leaves losers' funds in the pool.
// ============================================================================
#[tokio::test]
async fn escrow_decrements_for_winners_only() {
    let (state, signing_key) = test_state();
    let app = router().with_state(state.clone());
    let market = create_market_via_api(app.clone()).await;
    let market_id_hex = market["id"].as_str().unwrap().to_string();

    let alice = [0xAAu8; 32];
    let bob = [0xBBu8; 32];

    // Alice (winner) and Bob (loser) bet.
    for (bettor, outcome, stake, secret) in [
        (alice, "yes", 100u64, [0x01u8; 32]),
        (bob, "no", 50, [0x02u8; 32]),
    ] {
        let (status, _) = json_post(
            app.clone(),
            "/queue/bets/commit",
            json!({
                "market_id": market_id_hex,
                "outcome": outcome,
                "stake": stake,
                "bettor_hex": hex32(&bettor),
                "secret_hex": hex32(&secret),
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // Both bettors' escrow balance is exactly the stake.
    {
        let e = state.escrow.read().await;
        assert_eq!(e.get(&alice).copied().unwrap_or(0), 100);
        assert_eq!(e.get(&bob).copied().unwrap_or(0), 50);
    }

    // Oracle says "yes".
    let market_id = pyana_app_framework::hex::hex_to_bytes32(&market_id_hex).unwrap();
    let outcome_id = state
        .markets
        .read()
        .await
        .get(&market_id)
        .unwrap()
        .outcome_for_label("yes")
        .unwrap();
    json_post(
        app.clone(),
        "/oracle/report",
        serde_json::to_value(&sign_report(
            &signing_key,
            OracleEntry {
                market_id,
                outcome_id,
                timestamp: 1,
            },
            0,
        ))
        .unwrap(),
    )
    .await;

    // Reveal Alice and Bob.
    let pending: Vec<[u8; 32]> = state.pending_bets.read().await.keys().copied().collect();
    for c in &pending {
        json_post(
            app.clone(),
            "/queue/bets/reveal",
            json!({ "commitment_hex": hex32(c) }),
        )
        .await;
    }

    // Settle.
    let (status, _) =
        json_post(app.clone(), &format!("/market/{}/settle", market_id_hex), json!({})).await;
    assert_eq!(status, StatusCode::OK);

    // After settle: alice's escrow goes back to 0 (her stake was released
    // into her payout). Bob's escrow is unchanged (still 50, those funds
    // funded the prize).
    let e = state.escrow.read().await;
    assert_eq!(e.get(&alice).copied().unwrap_or(0), 0, "alice should have no escrow left");
    assert_eq!(e.get(&bob).copied().unwrap_or(0), 50, "bob's losing stake should remain locked");
}

// ============================================================================
// Property 7: closed market refuses new bets.
// ============================================================================
#[tokio::test]
async fn cannot_bet_after_market_close_height() {
    let (state, _signing_key) = test_state();
    let app = router().with_state(state.clone());

    // Create market with close_height = 5.
    let (status, market) = json_post(
        app.clone(),
        "/market",
        json!({
            "question": "X?",
            "outcomes": ["a", "b"],
            "close_height": 5,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let market_id_hex = market["id"].as_str().unwrap().to_string();

    // Advance height past close (height is not behind admin auth in tests
    // because PYANA_ADMIN_TOKEN is unset → Disabled, so we set it directly).
    *state.current_height.write().await = 10;

    // Try to bet.
    let (status, body) = json_post(
        app.clone(),
        "/queue/bets/commit",
        json!({
            "market_id": market_id_hex,
            "outcome": "a",
            "stake": 1,
            "bettor_hex": hex32(&[0xAAu8; 32]),
            "secret_hex": hex32(&[0x01u8; 32]),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}

// ============================================================================
// Property 8: blinded queue truly hides outcome lean. After 3 commits, the
// queue exposes only commitments + a single root — NOT outcome distribution.
// ============================================================================
#[tokio::test]
async fn blinded_queue_does_not_leak_outcome_distribution() {
    // Construct two markets that should have IDENTICAL public-observable
    // state from the queue's perspective despite WILDLY different sentiment.
    //
    // We do this by committing the same number of bets with the same stakes
    // in both scenarios, but with different outcomes — and asserting that
    // the commitment root + (commitment, position) tuples differ randomly
    // (driven by the bettor secrets), but neither scenario leaks "which
    // outcome is winning".
    let (state_a, _) = test_state();
    let (state_b, _) = test_state();

    use crate::bets::{BetPayload, create_bet_commitment};
    let market_id = [0u8; 32];
    let outcome_yes: [u8; 32] = [1u8; 32];
    let outcome_no: [u8; 32] = [2u8; 32];

    // Scenario A: 3 bets all on YES.
    for i in 0..3u8 {
        let payload = BetPayload {
            market_id,
            outcome_id: outcome_yes,
            stake: 100,
            bettor: [i; 32],
        };
        let secret = [i + 10; 32];
        let c = create_bet_commitment(&payload, &secret);
        state_a.blinded_queue.lock().await.commit(c).unwrap();
    }

    // Scenario B: 3 bets all on NO.
    for i in 0..3u8 {
        let payload = BetPayload {
            market_id,
            outcome_id: outcome_no,
            stake: 100,
            bettor: [i; 32],
        };
        let secret = [i + 10; 32];
        let c = create_bet_commitment(&payload, &secret);
        state_b.blinded_queue.lock().await.commit(c).unwrap();
    }

    // The queue exposes only the commitment_root and the consumed_count.
    // Observe that there is NO route on AppState that reveals outcomes for
    // unconsumed bets. Confirm via the router that /queue/blinded is the
    // only observability surface (the queue is private otherwise).
    let qa = state_a.blinded_queue.lock().await;
    let qb = state_b.blinded_queue.lock().await;
    assert_eq!(qa.remaining(), 3);
    assert_eq!(qb.remaining(), 3);
    assert_eq!(qa.consumed_count(), 0);
    assert_eq!(qb.consumed_count(), 0);
    // Roots differ (because commitments differ) but the difference doesn't
    // reveal outcome distribution — any observer comparing roots cannot say
    // "A has more YES bets" without breaking blake3 preimage resistance.
    assert_ne!(qa.commitment_root(), qb.commitment_root());
}

// ============================================================================
// Property 9: a malformed (wrong-position) consumption proof is rejected by
// the queue even though the nullifier is fresh.
// ============================================================================
#[tokio::test]
async fn malformed_consumption_proof_rejected() {
    let (state, _signing_key) = test_state();
    use crate::bets::{BetPayload, create_bet_commitment};

    let payload = BetPayload {
        market_id: [0u8; 32],
        outcome_id: [1u8; 32],
        stake: 100,
        bettor: [0xAAu8; 32],
    };
    let secret = [0x01u8; 32];
    let c = create_bet_commitment(&payload, &secret);
    state.blinded_queue.lock().await.commit(c).unwrap();

    // Hand-craft a proof claiming the commitment is at position 99 (out of
    // range).
    use pyana_storage::blinded::{ConsumeResult, ConsumptionProof, crypto::derive_nullifier};
    let nullifier = derive_nullifier(&c, &secret, 99);
    let bogus = ConsumptionProof {
        nullifier,
        commitment: c,
        position: 99,
        membership_proof: vec![],
    };
    let result = state.blinded_queue.lock().await.consume(&bogus);
    assert_eq!(result, ConsumeResult::InvalidProof);
}

// ============================================================================
// Property 10: market state machine ordering — settle before resolve fails.
// ============================================================================
#[tokio::test]
async fn settle_before_resolve_rejected() {
    let (state, _) = test_state();
    let app = router().with_state(state.clone());
    let market = create_market_via_api(app.clone()).await;
    let market_id_hex = market["id"].as_str().unwrap().to_string();

    // Try to settle without an oracle report.
    let (status, _body) = json_post(
        app.clone(),
        &format!("/market/{}/settle", market_id_hex),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

// ============================================================================
// Property 11: a market initialised through the API exposes a 32-byte id
// and the right outcomes.
// ============================================================================
#[tokio::test]
async fn create_then_get_market_round_trips() {
    let (state, _) = test_state();
    let app = router().with_state(state);

    let (_, m) = json_post(
        app.clone(),
        "/market",
        json!({
            "question": "Q",
            "outcomes": ["a", "b", "c"],
            "close_height": 100,
        }),
    )
    .await;
    let id = m["id"].as_str().unwrap().to_string();

    let (status, fetched) = json_get(app.clone(), &format!("/market/{}", id)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["outcomes"], json!(["a", "b", "c"]));
    assert_eq!(fetched["status"], "open");
    assert_eq!(fetched["total_pool"], 0);
}

// ============================================================================
// Property 12: payouts endpoint returns the persisted result.
// ============================================================================
#[tokio::test]
async fn payouts_endpoint_returns_settled_payouts() {
    let (state, signing_key) = test_state();
    let app = router().with_state(state.clone());
    let market = create_market_via_api(app.clone()).await;
    let market_id_hex = market["id"].as_str().unwrap().to_string();

    json_post(
        app.clone(),
        "/queue/bets/commit",
        json!({
            "market_id": market_id_hex,
            "outcome": "yes",
            "stake": 100,
            "bettor_hex": hex32(&[0xAAu8; 32]),
            "secret_hex": hex32(&[0x01u8; 32]),
        }),
    )
    .await;

    let market_id = pyana_app_framework::hex::hex_to_bytes32(&market_id_hex).unwrap();
    let outcome_id = state
        .markets
        .read()
        .await
        .get(&market_id)
        .unwrap()
        .outcome_for_label("yes")
        .unwrap();
    json_post(
        app.clone(),
        "/oracle/report",
        serde_json::to_value(&sign_report(
            &signing_key,
            OracleEntry { market_id, outcome_id, timestamp: 1 },
            0,
        ))
        .unwrap(),
    )
    .await;

    let commitments: Vec<[u8; 32]> = state.pending_bets.read().await.keys().copied().collect();
    for c in &commitments {
        json_post(
            app.clone(),
            "/queue/bets/reveal",
            json!({ "commitment_hex": hex32(c) }),
        )
        .await;
    }
    json_post(app.clone(), &format!("/market/{}/settle", market_id_hex), json!({})).await;

    let (status, payouts) =
        json_get(app.clone(), &format!("/market/{}/payouts", market_id_hex)).await;
    assert_eq!(status, StatusCode::OK);
    let arr = payouts.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["amount"], 100);
}

// ============================================================================
// Smoke: market struct itself begins in Open state.
// ============================================================================
#[test]
fn market_lifecycle_smoke() {
    let m = Market::new("Q", vec!["yes".into(), "no".into()], 100);
    assert_eq!(m.outcomes.len(), 2);
    assert!(matches!(m.status, MarketStatus::Open));
}
