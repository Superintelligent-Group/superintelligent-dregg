//! Batched proposal execution.
//!
//! [`TreasuryBatchExecutor`] implements [`BatchExecutor`] from the framework.
//! Trustees delegate execution by leaving approved proposals on the queue; the
//! executor drains them in priority order and applies the spend orders to the
//! treasury **atomically per batch**: if any proposal cannot be satisfied
//! (insufficient balance, missing from governance, etc.), the whole batch is
//! rolled back and no balances change.
//!
//! The batch id is `blake3(domain || proposal_id1 || ... || proposal_idN)` so
//! it content-addresses the set of proposals executed.

use std::sync::Arc;

use tokio::sync::RwLock;

use pyana_app_framework::batch_executor::{BatchExecution, BatchExecutor, ClientTurnRequest};
use pyana_types::CellId;

use crate::governance::GovernanceState;
use crate::proposal::{Proposal, ProposalStatus};
use crate::treasury::{Treasury, TreasuryError};

/// Summary returned alongside the framework's [`BatchExecution`].
#[derive(Clone, Debug)]
pub struct BatchSummary {
    pub batch_id: [u8; 32],
    pub proposals: Vec<[u8; 32]>,
}

#[derive(Debug, thiserror::Error)]
pub enum TreasuryBatchExecutorError {
    #[error("proposal not approved at execution time: {0:?}")]
    NotApproved([u8; 32]),
    #[error("proposal disappeared between collect and execute: {0:?}")]
    Missing([u8; 32]),
    #[error("treasury rejected debit: {0}")]
    Treasury(#[from] TreasuryError),
    #[error("governance state error: {0}")]
    Governance(#[from] crate::governance::GovernanceError),
}

/// The executor: drains approved proposals from governance and settles them
/// against the treasury in atomic batches.
///
/// The executor needs *exclusive* mutable access to apply the batch atomically
/// — the framework's `BatchExecutor` trait takes `&mut self`. Treasury and
/// governance are held in shared async cells so the rest of the app can also
/// read them while the executor runs.
pub struct TreasuryBatchExecutor {
    governance: GovernanceState,
    treasury: Arc<RwLock<Treasury>>,
    /// Cell identity of the delegated executor (for `ClientTurnRequest.client`).
    executor_cell: CellId,
    /// Most-recently-collected batch (populated by `collect_batch`).
    pending: Vec<Proposal>,
}

impl TreasuryBatchExecutor {
    pub fn new(
        governance: GovernanceState,
        treasury: Arc<RwLock<Treasury>>,
        executor_cell: CellId,
    ) -> Self {
        Self {
            governance,
            treasury,
            executor_cell,
            pending: Vec::new(),
        }
    }

    /// Drain up to `max_size` approved proposals into the pending buffer and
    /// return them as `ClientTurnRequest`s for the framework's plumbing.
    ///
    /// Synchronous wrapper around the async governance read for trait
    /// conformance — uses `block_in_place` so it's safe inside a multi-thread
    /// runtime.
    pub async fn collect_batch_async(&mut self, max_size: usize) -> Vec<ClientTurnRequest> {
        let approved = self.governance.approved().await;
        let take = approved.into_iter().take(max_size).collect::<Vec<_>>();
        self.pending = take.clone();
        take.into_iter()
            .map(|p| ClientTurnRequest {
                client: self.executor_cell,
                turn_bytes: encode_turn(&p),
                deadline_height: None,
            })
            .collect()
    }

    /// Apply the pending batch atomically: either every proposal succeeds and
    /// is marked executed, or NO change is committed to the treasury.
    pub async fn execute_batch_async(
        &mut self,
        batch: Vec<ClientTurnRequest>,
    ) -> Result<(BatchExecution, BatchSummary), TreasuryBatchExecutorError> {
        // Re-fetch the proposals; the entries in `batch` are opaque blobs from
        // the framework's POV. We use the pending buffer that was filled by
        // `collect_batch_async`.
        let proposals = std::mem::take(&mut self.pending);
        if proposals.len() != batch.len() {
            // The framework called execute with a different batch than the one
            // we collected (or no collect happened). This is a programming
            // bug; fail loudly.
            return Err(TreasuryBatchExecutorError::Missing([0u8; 32]));
        }

        // === Stage 1: re-verify approval (TOCTOU defense) ===
        // Between `collect_batch_async` and `execute_batch_async`, governance
        // could have changed. Re-read current statuses.
        for p in &proposals {
            let live = self
                .governance
                .get(&p.id)
                .await
                .ok_or(TreasuryBatchExecutorError::Missing(p.id))?;
            if live.status != ProposalStatus::Approved {
                return Err(TreasuryBatchExecutorError::NotApproved(p.id));
            }
        }

        // === Stage 2: atomic debit ===
        // Snapshot the treasury, attempt all debits, commit only if every
        // proposal succeeds. We use a clone + replace so that a mid-batch
        // failure leaves the original balances untouched.
        let mut tre = self.treasury.write().await;
        let snapshot = tre.clone();
        let mut apply_result = Ok(());
        for p in &proposals {
            for order in &p.orders {
                if let Err(e) = tre.debit(order.asset, order.amount) {
                    apply_result = Err(TreasuryBatchExecutorError::Treasury(e));
                    break;
                }
            }
            if apply_result.is_err() {
                break;
            }
        }
        if let Err(e) = apply_result {
            // Roll back: restore the snapshot.
            *tre = snapshot;
            return Err(e);
        }
        drop(tre);

        // === Stage 3: mark executed ===
        // Only after the atomic debit lands do we flip statuses.
        for p in &proposals {
            self.governance.mark_executed(&p.id).await?;
        }

        // === Stage 4: produce batch id ===
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"pyana-dao-treasury-batch-v1");
        for p in &proposals {
            hasher.update(&p.id);
        }
        let batch_id = *hasher.finalize().as_bytes();

        Ok((
            BatchExecution {
                batch_id,
                turn_count: proposals.len(),
                proof: None, // optimistic mode — proof generation is future work
            },
            BatchSummary {
                batch_id,
                proposals: proposals.iter().map(|p| p.id).collect(),
            },
        ))
    }
}

/// Synchronous `BatchExecutor` impl uses `tokio::runtime::Handle::current()`
/// to bridge to the async governance/treasury APIs. This is only safe inside
/// a multi-thread tokio runtime; tests and the binary both use that.
impl BatchExecutor for TreasuryBatchExecutor {
    type Error = TreasuryBatchExecutorError;

    fn collect_batch(&mut self, max_size: usize) -> Vec<ClientTurnRequest> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.collect_batch_async(max_size))
        })
    }

    fn execute_batch(
        &mut self,
        batch: Vec<ClientTurnRequest>,
    ) -> Result<BatchExecution, Self::Error> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.execute_batch_async(batch))
        })
        .map(|(exec, _)| exec)
    }
}

/// Canonical turn bytes for a proposal (used as the framework's
/// `ClientTurnRequest.turn_bytes`).
fn encode_turn(p: &Proposal) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"dao-treasury-turn-v1");
    out.extend_from_slice(&p.id);
    for o in &p.orders {
        out.extend_from_slice(&o.asset);
        out.extend_from_slice(&o.amount.to_le_bytes());
        out.extend_from_slice(&o.recipient);
    }
    out
}
