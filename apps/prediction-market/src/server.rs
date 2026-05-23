//! HTTP API server for the prediction-market app.
//!
//! ## Route map
//!
//! | Method | Path                                  | What it does                                |
//! |--------|---------------------------------------|---------------------------------------------|
//! | POST   | `/market`                             | Create a market.                            |
//! | GET    | `/market/{id}`                        | Status + pool.                              |
//! | POST   | `/queue/bets/commit`                  | Place a bet (commits into blinded queue).   |
//! | POST   | `/queue/bets/reveal`                  | Reveal + consume from blinded queue.        |
//! | POST   | `/oracle/report`                      | Submit a signed oracle report.              |
//! | POST   | `/market/{id}/settle`                 | Compute payouts after resolution.           |
//! | GET    | `/market/{id}/payouts`                | List computed payouts.                      |
//! | GET    | `/oracle/proof/{position}`            | Get positional-sequence inclusion proof.    |
//! | GET    | `/admin/height`                       | (admin) advance the current block height.   |
//!
//! Note that `/queue/bets/commit` shares the underlying `BlindedQueue` with
//! the framework-supplied `/queue/blinded/{commit,status,...}` routes,
//! installed via `AppServer::with_blinded_endpoint`, so external observers
//! get a consistent view of root + size.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use pyana_app_framework::auth::{AdminAuth, AdminToken, HasAdminToken};
use pyana_app_framework::server::{ErrorResponse, api_error};
use pyana_storage::blinded::{BlindedQueue, ConsumeResult, crypto as blinded_crypto};

use crate::bets::{BetPayload, BettorId, PendingBet, create_bet_commitment};
use crate::market::{Market, MarketId, MarketStatus};
use crate::oracle::{Oracle, OracleReport};
use crate::settlement::{Payout, RevealedBet, settle, total_winning_stake};

// =============================================================================
// Application State
// =============================================================================

/// All app state. Wrapped in `Arc<RwLock<...>>` per field rather than the
/// whole struct so independent endpoints don't serialize against each other.
#[derive(Clone)]
pub struct AppState {
    pub markets: Arc<RwLock<HashMap<MarketId, Market>>>,
    /// The blinded queue, shared with the framework endpoint.
    pub blinded_queue: Arc<Mutex<BlindedQueue>>,
    /// Parallel commitment list (used to compute merkle siblings — the
    /// `BlindedQueue` keeps these private). Always kept in lock-step with the
    /// queue under `blinded_queue`'s mutex.
    pub commitments: Arc<Mutex<Vec<[u8; 32]>>>,
    /// Pending (committed-but-not-revealed) bets, keyed by commitment.
    pub pending_bets: Arc<RwLock<HashMap<[u8; 32], PendingBet>>>,
    /// Revealed bets per market (built up as consume succeeds).
    pub revealed: Arc<RwLock<HashMap<MarketId, Vec<RevealedBet>>>>,
    /// Computed payouts per market (filled by `/market/{id}/settle`).
    pub payouts: Arc<RwLock<HashMap<MarketId, Vec<Payout>>>>,
    /// In-app escrow balances. `escrow[bettor]` = total computrons currently
    /// locked across bets. Decreases on payout.
    ///
    /// REVIEW[P2]: this is in-app bookkeeping, not the real escrow primitive
    /// from `pyana_app_framework::escrow`. Wiring `EscrowManager` would mean
    /// running an actual `PyanaEngine`, which is heavier than this app
    /// currently sets up. Until that's wired, the "escrow" guarantee is
    /// app-local rather than turn-conserved.
    pub escrow: Arc<RwLock<HashMap<BettorId, u64>>>,
    pub oracle: Arc<RwLock<Oracle>>,
    pub current_height: Arc<RwLock<u64>>,
    pub admin_token: AdminToken,
}

impl HasAdminToken for AppState {
    fn admin_token(&self) -> &AdminToken {
        &self.admin_token
    }
}

impl AppState {
    /// Construct with a fresh blinded queue + an oracle authority pubkey.
    pub fn new(blinded_capacity: usize, oracle_authority: [u8; 32]) -> Self {
        Self::new_with_queue(
            Arc::new(Mutex::new(BlindedQueue::new(blinded_capacity))),
            oracle_authority,
        )
    }

    /// Construct using an externally-provided `BlindedQueue` so the same Arc
    /// can be handed to `FairDistributionEndpoint` for /queue/blinded/*.
    pub fn new_with_queue(
        blinded_queue: Arc<Mutex<BlindedQueue>>,
        oracle_authority: [u8; 32],
    ) -> Self {
        Self {
            markets: Arc::new(RwLock::new(HashMap::new())),
            blinded_queue,
            commitments: Arc::new(Mutex::new(Vec::new())),
            pending_bets: Arc::new(RwLock::new(HashMap::new())),
            revealed: Arc::new(RwLock::new(HashMap::new())),
            payouts: Arc::new(RwLock::new(HashMap::new())),
            escrow: Arc::new(RwLock::new(HashMap::new())),
            oracle: Arc::new(RwLock::new(Oracle::new(oracle_authority))),
            current_height: Arc::new(RwLock::new(1)),
            admin_token: AdminToken::from_env(),
        }
    }

    /// Build a router that the binary wires into `AppServer`.
    pub fn router(self) -> Router {
        router().with_state(self)
    }
}

// =============================================================================
// Request/Response Types
// =============================================================================

#[derive(Deserialize)]
pub struct CreateMarketRequest {
    pub question: String,
    pub outcomes: Vec<String>,
    pub close_height: u64,
}

#[derive(Serialize)]
pub struct MarketResponse {
    pub id: String,
    pub question: String,
    pub outcomes: Vec<String>,
    pub status: String,
    pub total_pool: u64,
    pub close_height: u64,
    pub winning_outcome: Option<String>,
}

#[derive(Deserialize)]
pub struct PlaceBetRequest {
    pub market_id: String,
    /// Outcome label (e.g., "yes"). Server derives the OutcomeId.
    pub outcome: String,
    pub stake: u64,
    pub bettor_hex: String,
    /// 32-byte secret used to derive both the commitment and the nullifier.
    pub secret_hex: String,
}

#[derive(Serialize)]
pub struct PlaceBetResponse {
    pub commitment_hex: String,
    pub position: usize,
    pub queue_root_hex: String,
    pub escrowed: u64,
}

#[derive(Deserialize)]
pub struct RevealBetRequest {
    pub commitment_hex: String,
}

#[derive(Serialize)]
pub struct RevealBetResponse {
    pub result: String,
    pub nullifier_hex: Option<String>,
}

#[derive(Serialize)]
pub struct OracleReportResponse {
    pub accepted: bool,
    pub position: u64,
    pub root_hex: String,
}

#[derive(Serialize)]
pub struct SettleResponse {
    pub market_id: String,
    pub winning_outcome: String,
    pub winners: Vec<PayoutDto>,
    pub total_pool: u64,
    pub total_winning_stake: u64,
}

#[derive(Serialize)]
pub struct PayoutDto {
    pub bettor_hex: String,
    pub amount: u64,
}

// =============================================================================
// Router
// =============================================================================

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/market", post(create_market))
        .route("/market/{id}", get(get_market))
        .route("/market/{id}/settle", post(settle_market))
        .route("/market/{id}/payouts", get(get_payouts))
        .route("/queue/bets/commit", post(commit_bet))
        .route("/queue/bets/reveal", post(reveal_bet))
        .route("/oracle/report", post(oracle_report))
        .route("/oracle/proof/{position}", get(oracle_proof))
        .route("/admin/height", post(admin_advance_height))
}

// =============================================================================
// Hex helpers
// =============================================================================

fn hex_id(id: &[u8; 32]) -> String {
    pyana_app_framework::hex::bytes32_to_hex(id)
}

fn parse_hex_id(s: &str) -> Option<[u8; 32]> {
    pyana_app_framework::hex::hex_to_bytes32(s).ok()
}

// =============================================================================
// Handlers — markets
// =============================================================================

async fn create_market(
    State(state): State<AppState>,
    Json(req): Json<CreateMarketRequest>,
) -> Result<(StatusCode, Json<MarketResponse>), (StatusCode, Json<ErrorResponse>)> {
    if req.outcomes.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "outcomes cannot be empty"));
    }
    let m = Market::new(req.question, req.outcomes, req.close_height);
    let resp = market_to_response(&m);
    state.markets.write().await.insert(m.id, m);
    Ok((StatusCode::CREATED, Json(resp)))
}

async fn get_market(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MarketResponse>, (StatusCode, Json<ErrorResponse>)> {
    let id_bytes = parse_hex_id(&id)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid market id"))?;
    let markets = state.markets.read().await;
    let m = markets
        .get(&id_bytes)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "market not found"))?;
    Ok(Json(market_to_response(m)))
}

fn market_to_response(m: &Market) -> MarketResponse {
    let (status_str, winner) = match &m.status {
        MarketStatus::Open => ("open".to_string(), None),
        MarketStatus::Resolving { winning_outcome, .. } => {
            ("resolving".to_string(), Some(hex_id(winning_outcome)))
        }
        MarketStatus::Resolved { winning_outcome, .. } => {
            ("resolved".to_string(), Some(hex_id(winning_outcome)))
        }
    };
    MarketResponse {
        id: hex_id(&m.id),
        question: m.question.clone(),
        outcomes: m.outcome_labels.clone(),
        status: status_str,
        total_pool: m.total_pool,
        close_height: m.close_height,
        winning_outcome: winner,
    }
}

// =============================================================================
// Handlers — bets (blinded queue)
// =============================================================================

async fn commit_bet(
    State(state): State<AppState>,
    Json(req): Json<PlaceBetRequest>,
) -> Result<Json<PlaceBetResponse>, (StatusCode, Json<ErrorResponse>)> {
    let market_id = parse_hex_id(&req.market_id)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid market_id"))?;
    let bettor = parse_hex_id(&req.bettor_hex)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid bettor_hex"))?;
    let secret = parse_hex_id(&req.secret_hex)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid secret_hex"))?;
    if req.stake == 0 {
        return Err(api_error(StatusCode::BAD_REQUEST, "stake must be non-zero"));
    }

    // 1) Look up the market and pre-validate. We hold a write lock here so
    //    that the outcome resolution + total_pool update are atomic with the
    //    commitment being added.
    let mut markets = state.markets.write().await;
    let market = markets
        .get_mut(&market_id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "market not found"))?;
    if !matches!(market.status, MarketStatus::Open) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "market is not open for new bets",
        ));
    }
    let height = *state.current_height.read().await;
    if height > market.close_height {
        return Err(api_error(
            StatusCode::CONFLICT,
            format!(
                "market closed: current height {height} > close {}",
                market.close_height
            ),
        ));
    }
    let outcome_id = market
        .outcome_for_label(&req.outcome)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "unknown outcome label"))?;

    // 2) Build the commitment.
    let payload = BetPayload {
        market_id,
        outcome_id,
        stake: req.stake,
        bettor,
    };
    let commitment = create_bet_commitment(&payload, &secret);

    // 3) Acquire queue + parallel-list locks together (always in the same
    //    order to avoid deadlocks). Commit into both.
    let mut queue = state.blinded_queue.lock().await;
    let mut commitments = state.commitments.lock().await;
    queue.commit(commitment).map_err(|e| {
        api_error(StatusCode::UNPROCESSABLE_ENTITY, format!("commit failed: {e:?}"))
    })?;
    let position = commitments.len();
    commitments.push(commitment);
    let root_hex = hex_id(&queue.commitment_root());
    drop(queue);
    drop(commitments);

    // 4) Move stake into escrow + bump pool.
    let mut escrow = state.escrow.write().await;
    *escrow.entry(bettor).or_insert(0) += req.stake;
    drop(escrow);
    market.total_pool += req.stake;

    // 5) Stash the pending bet.
    let pending = PendingBet {
        payload,
        commitment,
        secret,
        position,
    };
    state.pending_bets.write().await.insert(commitment, pending);

    Ok(Json(PlaceBetResponse {
        commitment_hex: hex_id(&commitment),
        position,
        queue_root_hex: root_hex,
        escrowed: req.stake,
    }))
}

async fn reveal_bet(
    State(state): State<AppState>,
    Json(req): Json<RevealBetRequest>,
) -> Result<Json<RevealBetResponse>, (StatusCode, Json<ErrorResponse>)> {
    let commitment = parse_hex_id(&req.commitment_hex)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid commitment_hex"))?;

    // Locate the pending bet WITHOUT removing it, so a failed consume
    // doesn't lose state.
    let pending = {
        let pending_bets = state.pending_bets.read().await;
        pending_bets
            .get(&commitment)
            .cloned()
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "no pending bet for that commitment"))?
    };

    // Compute merkle siblings against the parallel commitment list.
    let merkle_proof = {
        let commitments = state.commitments.lock().await;
        local_merkle_proof(&commitments, pending.position)
    };

    let proof = blinded_crypto::build_consumption_proof(
        pending.commitment,
        pending.secret,
        pending.position,
        merkle_proof,
    );

    // Consume against the live queue.
    let nullifier_bytes = proof.nullifier;
    let result = {
        let mut queue = state.blinded_queue.lock().await;
        queue.consume(&proof)
    };

    match result {
        ConsumeResult::Consumed { nullifier } => {
            // Record the revealed bet against its market.
            let revealed = RevealedBet {
                payload: pending.payload.clone(),
                nullifier,
            };
            state
                .revealed
                .write()
                .await
                .entry(pending.payload.market_id)
                .or_default()
                .push(revealed);
            // Remove pending entry to make a second reveal attempt a NotFound
            // at THIS layer (queue layer also rejects via AlreadyConsumed).
            state.pending_bets.write().await.remove(&commitment);
            Ok(Json(RevealBetResponse {
                result: "revealed".into(),
                nullifier_hex: Some(hex_id(&nullifier)),
            }))
        }
        ConsumeResult::AlreadyConsumed => Ok(Json(RevealBetResponse {
            result: "already_consumed".into(),
            nullifier_hex: Some(hex_id(&nullifier_bytes)),
        })),
        ConsumeResult::InvalidProof => Ok(Json(RevealBetResponse {
            result: "invalid_proof".into(),
            nullifier_hex: None,
        })),
    }
}

// =============================================================================
// Handlers — oracle
// =============================================================================

async fn oracle_report(
    State(state): State<AppState>,
    Json(report): Json<OracleReport>,
) -> Result<Json<OracleReportResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut oracle = state.oracle.write().await;
    oracle
        .accept_report(&report)
        .map_err(|e| api_error(StatusCode::UNAUTHORIZED, e.to_string()))?;

    // Advance the matching market to Resolving if it's still Open and the
    // outcome is one of its declared outcomes.
    let mut markets = state.markets.write().await;
    if let Some(market) = markets.get_mut(&report.entry.market_id) {
        if matches!(market.status, MarketStatus::Open) {
            // Verify the reported outcome is actually one of the market's
            // declared outcomes (oracle could report an arbitrary 32-byte id;
            // we reject if the market doesn't know it).
            if !market.has_outcome(&report.entry.outcome_id) {
                return Err(api_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "oracle reported an outcome that the market does not declare",
                ));
            }
            // Use current_height + 100 as the claim deadline. (Adjustable.)
            let height = *state.current_height.read().await;
            market
                .begin_resolution(report.entry.outcome_id, height + 100)
                .map_err(|e| api_error(StatusCode::CONFLICT, e.to_string()))?;
        }
    }

    let root_hex = hex_id(&oracle.root());
    Ok(Json(OracleReportResponse {
        accepted: true,
        position: report.position,
        root_hex,
    }))
}

async fn oracle_proof(
    State(state): State<AppState>,
    Path(position): Path<u64>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let oracle = state.oracle.read().await;
    let proof = oracle
        .inclusion_proof(position)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "no entry at that position"))?;
    let entry = oracle
        .entry_at(position)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "no entry at that position"))?;
    Ok(Json(serde_json::json!({
        "position": position,
        "entry": entry,
        "leaf_hex": hex_id(&proof.leaf),
        "siblings_hex": proof.siblings.iter().map(hex_id).collect::<Vec<_>>(),
        "root_hex": hex_id(&oracle.root()),
    })))
}

// =============================================================================
// Handlers — settlement
// =============================================================================

async fn settle_market(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SettleResponse>, (StatusCode, Json<ErrorResponse>)> {
    let market_id = parse_hex_id(&id)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid market id"))?;
    let mut markets = state.markets.write().await;
    let market = markets
        .get_mut(&market_id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "market not found"))?;
    let winning = match &market.status {
        MarketStatus::Resolving { winning_outcome, .. } => *winning_outcome,
        MarketStatus::Resolved { .. } => {
            return Err(api_error(StatusCode::CONFLICT, "market already resolved"));
        }
        MarketStatus::Open => {
            return Err(api_error(StatusCode::CONFLICT, "market is not in resolution"));
        }
    };

    let revealed = {
        let map = state.revealed.read().await;
        map.get(&market_id).cloned().unwrap_or_default()
    };

    let payouts = settle(&revealed, &winning, market.total_pool)
        .map_err(|e| api_error(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;

    // Decrement escrow by each winner's stake (their stake is "released"
    // back as a payout) and credit the winner side.
    let mut escrow = state.escrow.write().await;
    for p in &payouts {
        // Find the original stake to subtract.
        let original: u64 = revealed
            .iter()
            .filter(|r| {
                r.payload.outcome_id == winning && r.payload.bettor == p.bettor
            })
            .map(|r| r.payload.stake)
            .sum();
        if let Some(v) = escrow.get_mut(&p.bettor) {
            *v = v.saturating_sub(original);
        }
    }
    drop(escrow);

    let twstake = total_winning_stake(&revealed, &winning);
    market.finalize(twstake).ok();

    let resp = SettleResponse {
        market_id: hex_id(&market_id),
        winning_outcome: hex_id(&winning),
        winners: payouts
            .iter()
            .map(|p| PayoutDto {
                bettor_hex: hex_id(&p.bettor),
                amount: p.amount,
            })
            .collect(),
        total_pool: market.total_pool,
        total_winning_stake: twstake,
    };
    state.payouts.write().await.insert(market_id, payouts);
    Ok(Json(resp))
}

async fn get_payouts(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<PayoutDto>>, (StatusCode, Json<ErrorResponse>)> {
    let market_id = parse_hex_id(&id)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid market id"))?;
    let payouts = state.payouts.read().await;
    let list = payouts
        .get(&market_id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|p| PayoutDto {
            bettor_hex: hex_id(&p.bettor),
            amount: p.amount,
        })
        .collect();
    Ok(Json(list))
}

// =============================================================================
// Admin
// =============================================================================

#[derive(Deserialize)]
pub struct AdvanceHeightRequest {
    pub delta: Option<u64>,
}

async fn admin_advance_height(
    _auth: AdminAuth,
    State(state): State<AppState>,
    Json(req): Json<AdvanceHeightRequest>,
) -> Json<serde_json::Value> {
    let delta = req.delta.unwrap_or(1);
    let mut h = state.current_height.write().await;
    *h += delta;
    Json(serde_json::json!({"height": *h}))
}

// =============================================================================
// Local merkle (mirrors storage::blinded's private helper exactly so the
// proofs we build verify against the queue's root).
// =============================================================================

fn local_merkle_proof(leaves: &[[u8; 32]], position: usize) -> Vec<[u8; 32]> {
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
