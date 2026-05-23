//! HTTP API for the DAO treasury.
//!
//! Routes (all wired in the binary via [`router()`] + the framework's
//! `AppServer::with_queue_endpoint`):
//!
//! - `POST /proposals/submit`         — submit a new spending proposal
//! - `POST /proposals/{id}/vote`      — cast an approve/reject vote
//! - `GET  /proposals/{id}`           — read a proposal's state
//! - `POST /proposals/{id}/enqueue`   — application-layer quorum gate, then
//!   forwards to the programmable queue at `/queue/proposals`
//! - `POST /executor/run`             — collect+execute one batch
//! - `GET  /treasury/balances`        — read balance ledger
//! - `POST /admin/credit`             — admin: top up a treasury asset
//!
//! The programmable queue itself is mounted by the binary under
//! `/queue/proposals/*` and exposes `enqueue`/`dequeue`/`status` (see
//! [`pyana_app_framework::queue_endpoint`]).

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
use pyana_app_framework::hex::{bytes32_to_hex, hex_to_bytes32};
use pyana_app_framework::server::{ErrorResponse, api_error};
use pyana_storage::programmable::{ProgrammableQueue, ValidationContext};
use pyana_storage::queue::QueueEntry;
use pyana_types::CellId;

use crate::executor::TreasuryBatchExecutor;
use crate::governance::{GovernanceError, GovernanceState, QuorumGate, Voter};
use crate::proposal::{Proposal, ProposalStatus, SpendOrder};
use crate::treasury::{AssetId, Treasury};

// =============================================================================
// AppState
// =============================================================================

#[derive(Clone)]
pub struct AppState {
    pub governance: GovernanceState,
    pub treasury: Arc<RwLock<Treasury>>,
    pub queue: Arc<Mutex<ProgrammableQueue>>,
    pub executor: Arc<Mutex<TreasuryBatchExecutor>>,
    pub admin_token: AdminToken,
}

impl HasAdminToken for AppState {
    fn admin_token(&self) -> &AdminToken {
        &self.admin_token
    }
}

impl AppState {
    /// Build a fresh app state with the supplied initial voter set.
    pub fn new(voters: Vec<Voter>) -> Self {
        let governance = GovernanceState::new(voters);
        let treasury = Arc::new(RwLock::new(Treasury::new()));
        let queue = QuorumGate::make_queue("dao-treasury-proposals", [0u8; 32], 1024);
        let executor = TreasuryBatchExecutor::new(
            governance.clone(),
            treasury.clone(),
            CellId([0xEE; 32]),
        );
        Self {
            governance,
            treasury,
            queue: Arc::new(Mutex::new(queue)),
            executor: Arc::new(Mutex::new(executor)),
            admin_token: AdminToken::from_env(),
        }
    }
}

// =============================================================================
// Request / response types
// =============================================================================

#[derive(Deserialize)]
pub struct SubmitProposalRequest {
    /// Proposer id (hex32). Must be a registered voter.
    pub proposer: String,
    pub orders: Vec<SpendOrderJson>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct SpendOrderJson {
    pub asset: String,
    pub amount: u128,
    pub recipient: String,
}

#[derive(Deserialize)]
pub struct VoteRequest {
    /// Voter id (hex32). Must be a registered voter.
    pub voter: String,
    pub approve: bool,
}

#[derive(Deserialize)]
pub struct EnqueueProposalRequest {
    /// Sender id (hex32). Must equal a registered voter (gates noise).
    pub sender: String,
    /// Deposit in computrons (queue MinDeposit constraint is 0; passed through).
    pub deposit: u64,
}

#[derive(Deserialize)]
pub struct AdminCreditRequest {
    pub asset: String,
    pub amount: u128,
}

#[derive(Serialize)]
pub struct ProposalResponse {
    pub id: String,
    pub proposer: String,
    pub orders: Vec<SpendOrderJson>,
    pub approve_weight: u32,
    pub reject_weight: u32,
    pub status: String,
}

#[derive(Serialize)]
pub struct BalancesResponse {
    pub balances: Vec<(String, u128)>,
}

#[derive(Serialize)]
pub struct ExecutorRunResponse {
    pub batch_id: String,
    pub proposals: Vec<String>,
    pub turn_count: usize,
}

// =============================================================================
// Router
// =============================================================================

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/proposals/submit", post(submit_proposal))
        .route("/proposals/{id}", get(get_proposal))
        .route("/proposals/{id}/vote", post(vote_proposal))
        .route("/proposals/{id}/enqueue", post(enqueue_proposal))
        .route("/executor/run", post(executor_run))
        .route("/treasury/balances", get(treasury_balances))
        .route("/admin/credit", post(admin_credit))
}

// =============================================================================
// Handlers
// =============================================================================

fn parse_id(s: &str) -> Result<[u8; 32], (StatusCode, Json<ErrorResponse>)> {
    hex_to_bytes32(s).map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))
}

fn proposal_to_response(p: &Proposal) -> ProposalResponse {
    ProposalResponse {
        id: bytes32_to_hex(&p.id),
        proposer: bytes32_to_hex(&p.proposer),
        orders: p
            .orders
            .iter()
            .map(|o| SpendOrderJson {
                asset: bytes32_to_hex(&o.asset),
                amount: o.amount,
                recipient: bytes32_to_hex(&o.recipient),
            })
            .collect(),
        approve_weight: p.approve_weight,
        reject_weight: p.reject_weight,
        status: match p.status {
            ProposalStatus::Submitted => "submitted",
            ProposalStatus::Approved => "approved",
            ProposalStatus::Executed => "executed",
            ProposalStatus::Rejected => "rejected",
        }
        .to_string(),
    }
}

fn map_gov_err(e: GovernanceError) -> (StatusCode, Json<ErrorResponse>) {
    let code = match &e {
        GovernanceError::NotVoter => StatusCode::FORBIDDEN,
        GovernanceError::ProposalNotFound => StatusCode::NOT_FOUND,
        GovernanceError::ProposalDuplicate => StatusCode::CONFLICT,
        GovernanceError::AlreadyVoted => StatusCode::CONFLICT,
        GovernanceError::NotSubmitted => StatusCode::CONFLICT,
        GovernanceError::QuorumNotMet { .. } => StatusCode::UNPROCESSABLE_ENTITY,
    };
    api_error(code, e.to_string())
}

async fn submit_proposal(
    State(state): State<AppState>,
    Json(req): Json<SubmitProposalRequest>,
) -> Result<(StatusCode, Json<ProposalResponse>), (StatusCode, Json<ErrorResponse>)> {
    // REVIEW[P1]: this endpoint accepts the `proposer` id as a plain JSON
    // field rather than verifying a signed presentation proof. The framework
    // provides `pyana_app_framework::middleware` extractors for proof-bound
    // identities; wiring those here would make the proposer field
    // unforgeable. Today, the only safeguard is that `proposer` must be a
    // registered voter — a non-voter cannot use a stolen voter id without
    // also passing the AdminAuth on `/admin/*`, but a voter can still
    // submit a proposal in another voter's name. That is the auth gap.
    let proposer = parse_id(&req.proposer)?;
    let mut orders = Vec::with_capacity(req.orders.len());
    for o in req.orders {
        orders.push(SpendOrder {
            asset: parse_id(&o.asset)?,
            amount: o.amount,
            recipient: parse_id(&o.recipient)?,
        });
    }
    let proposal = Proposal::new(proposer, orders);
    let id = state
        .governance
        .submit(proposal.clone())
        .await
        .map_err(map_gov_err)?;
    let p = state
        .governance
        .get(&id)
        .await
        .ok_or_else(|| api_error(StatusCode::INTERNAL_SERVER_ERROR, "lost proposal"))?;
    Ok((StatusCode::CREATED, Json(proposal_to_response(&p))))
}

async fn get_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProposalResponse>, (StatusCode, Json<ErrorResponse>)> {
    let id_bytes = parse_id(&id)?;
    let p = state
        .governance
        .get(&id_bytes)
        .await
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "proposal not found"))?;
    Ok(Json(proposal_to_response(&p)))
}

async fn vote_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<VoteRequest>,
) -> Result<Json<ProposalResponse>, (StatusCode, Json<ErrorResponse>)> {
    // REVIEW[P1]: same authentication gap as `submit_proposal` — the voter id
    // is taken at face value. A future iteration should pull the voter from a
    // verified signature header rather than the body.
    let id_bytes = parse_id(&id)?;
    let voter = parse_id(&req.voter)?;
    state
        .governance
        .vote(&id_bytes, voter, req.approve)
        .await
        .map_err(map_gov_err)?;
    let p = state.governance.get(&id_bytes).await.unwrap();
    Ok(Json(proposal_to_response(&p)))
}

async fn enqueue_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<EnqueueProposalRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let id_bytes = parse_id(&id)?;
    let sender = parse_id(&req.sender)?;

    // === Application-layer quorum gate ===
    // The queue's `QueueConstraint::Custom { expr: "quorum-met" }` does not
    // currently enforce anything at the storage layer (see REVIEW[P1] in
    // `governance::QuorumGate::queue_program`). Enforce here.
    state.governance.check_quorum(&id_bytes).await.map_err(map_gov_err)?;

    // Forward to the programmable queue.
    let entry = QueueEntry {
        content_hash: id_bytes,
        sender,
        deposit: req.deposit,
        enqueued_at: 0,
        size: 32,
    };
    let ctx = ValidationContext {
        sender,
        current_height: 0,
        current_epoch: 0,
        sender_epoch_count: 0,
        preimage: None,
        sequence: None,
    };
    let mut q = state.queue.lock().await;
    let root = q.enqueue_validated(entry, &ctx).map_err(|e| {
        api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("queue rejected: {e:?}"),
        )
    })?;

    Ok(Json(serde_json::json!({
        "queued": true,
        "proposal_id": id,
        "queue_root": bytes32_to_hex(&root),
        "queue_len": q.len(),
    })))
}

async fn executor_run(
    State(state): State<AppState>,
) -> Result<Json<ExecutorRunResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut exec = state.executor.lock().await;
    let batch = exec.collect_batch_async(32).await;
    if batch.is_empty() {
        return Err(api_error(
            StatusCode::NO_CONTENT,
            "no approved proposals to execute",
        ));
    }
    let (execution, summary) = exec.execute_batch_async(batch).await.map_err(|e| {
        api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("execution failed: {e}"),
        )
    })?;
    Ok(Json(ExecutorRunResponse {
        batch_id: bytes32_to_hex(&execution.batch_id),
        proposals: summary
            .proposals
            .iter()
            .map(|p| bytes32_to_hex(p))
            .collect(),
        turn_count: execution.turn_count,
    }))
}

async fn treasury_balances(State(state): State<AppState>) -> Json<BalancesResponse> {
    let t = state.treasury.read().await;
    let balances = t
        .iter()
        .map(|(a, b)| (bytes32_to_hex(a), *b))
        .collect();
    Json(BalancesResponse { balances })
}

async fn admin_credit(
    _auth: AdminAuth,
    State(state): State<AppState>,
    Json(req): Json<AdminCreditRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let asset: AssetId = parse_id(&req.asset)?;
    let mut t = state.treasury.write().await;
    let new = t
        .credit(asset, req.amount)
        .map_err(|e| api_error(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    Ok(Json(serde_json::json!({
        "asset": req.asset,
        "balance": new,
    })))
}
