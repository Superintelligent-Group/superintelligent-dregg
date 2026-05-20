//! TurnExecutor: applies a turn to a ledger with full atomicity.
//!
//! The executor walks the call forest depth-first, checking preconditions,
//! verifying authorization, applying effects, and metering computrons at each step.
//! If any action fails, ALL effects are rolled back via journal replay (atomicity guarantee).

use ed25519_dalek::{Signature, VerifyingKey};
use pyana_cell::{
    AuthRequired, Cell, CellId, CellStateDelta, Ledger, LedgerDelta,
    Preconditions,
    preconditions::EvalContext,
    state::STATE_SLOTS,
};
use serde::{Deserialize, Serialize};

use crate::action::{Action, Authorization, DelegationMode, Effect};
use crate::error::TurnError;
use crate::forest::CallTree;
use crate::journal::{JournalEntry, LedgerJournal};
use crate::turn::{Turn, TurnReceipt, TurnResult};

/// Trait for verifying ZK proofs. Implementations provide circuit-specific verification.
///
/// The executor is fail-closed: if no ProofVerifier is configured and a cell requires
/// proof authorization, the action is rejected.
pub trait ProofVerifier: Send + Sync {
    /// Verify a proof against public inputs and a verification key.
    ///
    /// Returns true if the proof is valid for the given public inputs and verification key.
    fn verify(&self, proof: &[u8], public_inputs: &[u8], vk: &[u8]) -> bool;
}

/// Cost configuration for computron metering.
///
/// Each operation has a base cost in computrons. The total cost of a turn
/// is the sum of all operation costs. If the agent's fee doesn't cover the
/// total, the turn is rejected.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComputronCosts {
    /// Base cost per action in the forest.
    pub action_base: u64,
    /// Base cost per effect applied.
    pub effect_base: u64,
    /// Cost per computron transfer.
    pub transfer: u64,
    /// Cost for creating a new cell.
    pub create_cell: u64,
    /// Cost for verifying a ZK proof.
    pub proof_verify: u64,
    /// Cost for verifying a signature.
    pub signature_verify: u64,
    /// Cost per byte of data processed.
    pub per_byte: u64,
}

impl ComputronCosts {
    /// Default cost configuration (reasonable for testing).
    pub fn default_costs() -> Self {
        ComputronCosts {
            action_base: 100,
            effect_base: 50,
            transfer: 75,
            create_cell: 500,
            proof_verify: 1000,
            signature_verify: 200,
            per_byte: 1,
        }
    }

    /// Zero costs (for testing without metering).
    pub fn zero() -> Self {
        ComputronCosts {
            action_base: 0,
            effect_base: 0,
            transfer: 0,
            create_cell: 0,
            proof_verify: 0,
            signature_verify: 0,
            per_byte: 0,
        }
    }
}

impl Default for ComputronCosts {
    fn default() -> Self {
        Self::default_costs()
    }
}

/// The turn executor: applies turns to a ledger atomically.
pub struct TurnExecutor {
    /// Cost configuration for computron metering.
    pub costs: ComputronCosts,
    /// Current timestamp for precondition evaluation.
    pub current_timestamp: i64,
    /// Current block height for precondition evaluation.
    pub block_height: u64,
    /// Optional ZK proof verifier. If None and a cell requires proof auth, the action is rejected.
    pub proof_verifier: Option<Box<dyn ProofVerifier>>,
}

impl TurnExecutor {
    /// Create a new executor with the given cost configuration.
    pub fn new(costs: ComputronCosts) -> Self {
        TurnExecutor {
            costs,
            current_timestamp: 0,
            block_height: 0,
            proof_verifier: None,
        }
    }

    /// Create a new executor with a proof verifier.
    pub fn with_proof_verifier(costs: ComputronCosts, verifier: Box<dyn ProofVerifier>) -> Self {
        TurnExecutor {
            costs,
            current_timestamp: 0,
            block_height: 0,
            proof_verifier: Some(verifier),
        }
    }

    /// Set the proof verifier.
    pub fn set_proof_verifier(&mut self, verifier: Box<dyn ProofVerifier>) {
        self.proof_verifier = Some(verifier);
    }

    /// Set the current timestamp (used for expiration and precondition checks).
    pub fn set_timestamp(&mut self, ts: i64) {
        self.current_timestamp = ts;
    }

    /// Set the current block height (used for network preconditions).
    pub fn set_block_height(&mut self, height: u64) {
        self.block_height = height;
    }

    /// Execute a turn against a ledger, returning the result.
    ///
    /// This is the main entry point. The executor:
    /// 1. Validates turn-level conditions (expiration, nonce, fee).
    /// 2. Creates a journal for efficient rollback (no full ledger clone).
    /// 3. Walks the call forest depth-first.
    /// 4. For each action: checks preconditions, verifies authorization, applies effects.
    /// 5. Meters computrons at each step.
    /// 6. If any action fails: replays journal in reverse to roll back ALL effects.
    /// 7. If successful: produces a TurnReceipt with Merkle hashes.
    pub fn execute(&self, turn: &Turn, ledger: &mut Ledger) -> TurnResult {
        // Phase 0: basic validation.
        if turn.call_forest.is_empty() {
            return TurnResult::Rejected {
                reason: TurnError::EmptyForest,
                at_action: vec![],
            };
        }

        // Check expiration.
        if let Some(valid_until) = turn.valid_until {
            if self.current_timestamp > valid_until {
                return TurnResult::Rejected {
                    reason: TurnError::Expired {
                        valid_until,
                        now: self.current_timestamp,
                    },
                    at_action: vec![],
                };
            }
        }

        // Check agent cell exists.
        let agent_cell = match ledger.get(&turn.agent) {
            Some(cell) => cell,
            None => {
                return TurnResult::Rejected {
                    reason: TurnError::CellNotFound { id: turn.agent },
                    at_action: vec![],
                };
            }
        };

        // Check nonce.
        if agent_cell.state.nonce != turn.nonce {
            return TurnResult::Rejected {
                reason: TurnError::NonceReplay {
                    expected: agent_cell.state.nonce,
                    got: turn.nonce,
                },
                at_action: vec![],
            };
        }

        // Check fee coverage (agent must have enough balance for the fee).
        if agent_cell.state.balance < turn.fee {
            return TurnResult::Rejected {
                reason: TurnError::InsufficientBalance {
                    cell: turn.agent,
                    required: turn.fee,
                    available: agent_cell.state.balance,
                },
                at_action: vec![],
            };
        }

        // Phase 1: Compute pre-state hash and create journal for rollback.
        let pre_state_hash = ledger.root();
        let mut journal = LedgerJournal::with_capacity(16);

        // Phase 2: Deduct fee from agent (recording undo entries in journal).
        {
            let agent = ledger.get_mut(&turn.agent).unwrap();
            journal.record_set_balance(turn.agent, agent.state.balance);
            agent.state.balance -= turn.fee;
            journal.record_set_nonce(turn.agent, agent.state.nonce);
            agent.state.increment_nonce();
        }

        // Phase 3: Walk the call forest depth-first.
        let mut computrons_used: u64 = 0;
        let mut all_effects_hashes: Vec<[u8; 32]> = Vec::new();

        for (root_idx, root_tree) in turn.call_forest.roots.iter().enumerate() {
            let result = self.execute_tree(
                root_tree,
                ledger,
                &turn.agent,
                DelegationMode::ParentsOwn, // top-level: agent owns all its capabilities
                &mut computrons_used,
                turn.fee,
                &mut all_effects_hashes,
                vec![root_idx],
                &mut journal,
            );

            if let Err((error, path)) = result {
                // Rollback: replay journal in reverse to restore ledger.
                journal.rollback(ledger);
                return TurnResult::Rejected {
                    reason: error,
                    at_action: path,
                };
            }
        }

        // Check total cost against fee.
        if computrons_used > turn.fee {
            journal.rollback(ledger);
            return TurnResult::Rejected {
                reason: TurnError::BudgetExceeded {
                    limit: turn.fee,
                    used: computrons_used,
                },
                at_action: vec![],
            };
        }

        // Phase 4: Compute receipt.
        let post_state_hash = ledger.root();
        let effects_hash = self.compute_effects_hash(&all_effects_hashes);

        // Compute turn hash (we need a mutable clone for hashing).
        let mut turn_clone = turn.clone();
        let turn_hash = turn_clone.hash();
        let forest_hash = turn_clone.call_forest.forest_hash;

        // Build ledger delta from the journal (no snapshot needed).
        let delta = Self::compute_delta_from_journal(&journal, ledger);

        let receipt = TurnReceipt {
            turn_hash,
            forest_hash,
            pre_state_hash,
            post_state_hash,
            timestamp: self.current_timestamp,
            effects_hash,
            computrons_used,
            action_count: turn.call_forest.action_count(),
        };

        TurnResult::Committed {
            ledger_delta: delta,
            receipt,
            computrons_used,
        }
    }

    /// Estimate the computron cost of a turn without applying it.
    pub fn estimate_cost(&self, turn: &Turn) -> u64 {
        let mut total: u64 = 0;
        for root in &turn.call_forest.roots {
            total = total.saturating_add(self.estimate_tree_cost(root));
        }
        total
    }

    /// Validate a turn without applying it. Returns Ok(()) if it would succeed,
    /// or the first error that would be encountered.
    pub fn validate_without_apply(&self, turn: &Turn, ledger: &Ledger) -> Result<(), TurnError> {
        if turn.call_forest.is_empty() {
            return Err(TurnError::EmptyForest);
        }

        if let Some(valid_until) = turn.valid_until {
            if self.current_timestamp > valid_until {
                return Err(TurnError::Expired {
                    valid_until,
                    now: self.current_timestamp,
                });
            }
        }

        let agent_cell = ledger.get(&turn.agent).ok_or(TurnError::CellNotFound { id: turn.agent })?;

        if agent_cell.state.nonce != turn.nonce {
            return Err(TurnError::NonceReplay {
                expected: agent_cell.state.nonce,
                got: turn.nonce,
            });
        }

        if agent_cell.state.balance < turn.fee {
            return Err(TurnError::InsufficientBalance {
                cell: turn.agent,
                required: turn.fee,
                available: agent_cell.state.balance,
            });
        }

        // Estimate cost.
        let estimated = self.estimate_cost(turn);
        if estimated > turn.fee {
            return Err(TurnError::BudgetExceeded {
                limit: turn.fee,
                used: estimated,
            });
        }

        Ok(())
    }

    /// Execute a single tree node and its children recursively.
    ///
    /// Returns Ok(()) on success or Err((TurnError, path)) on failure.
    fn execute_tree(
        &self,
        tree: &CallTree,
        ledger: &mut Ledger,
        parent_cell: &CellId,
        parent_delegation: DelegationMode,
        computrons_used: &mut u64,
        budget: u64,
        effects_hashes: &mut Vec<[u8; 32]>,
        path: Vec<usize>,
        journal: &mut LedgerJournal,
    ) -> Result<(), (TurnError, Vec<usize>)> {
        let action = &tree.action;

        // Meter the action base cost.
        *computrons_used = computrons_used.saturating_add(self.costs.action_base);
        if *computrons_used > budget {
            return Err((
                TurnError::BudgetExceeded { limit: budget, used: *computrons_used },
                path,
            ));
        }

        // Check target cell exists.
        let target_cell = ledger.get(&action.target).ok_or_else(|| {
            (TurnError::CellNotFound { id: action.target }, path.clone())
        })?;

        // Check capability: does the parent have access to the target?
        // The agent (top-level parent) implicitly has access to itself.
        // For other cells, the parent must hold a capability.
        if &action.target != parent_cell {
            let parent = ledger.get(parent_cell).ok_or_else(|| {
                (TurnError::CellNotFound { id: *parent_cell }, path.clone())
            })?;

            let has_capability = parent.capabilities.has_access(&action.target);

            // Check delegation mode: if parent_delegation is None, child actions cannot
            // use the parent's capabilities to reach non-parent cells.
            if !has_capability {
                // Check if delegation allows reaching this target.
                match parent_delegation {
                    DelegationMode::None => {
                        return Err((
                            TurnError::CapabilityNotHeld {
                                actor: *parent_cell,
                                target: action.target,
                            },
                            path,
                        ));
                    }
                    DelegationMode::ParentsOwn | DelegationMode::Inherit => {
                        // Still need the capability to be held by someone in the chain.
                        return Err((
                            TurnError::CapabilityNotHeld {
                                actor: *parent_cell,
                                target: action.target,
                            },
                            path,
                        ));
                    }
                }
            }
        }

        // Check preconditions.
        self.check_preconditions(&action.preconditions, target_cell, &path)?;

        // Verify authorization (including signature/proof verification).
        self.verify_authorization(action, target_cell, ledger, &path)?;

        // Meter authorization cost.
        let auth_cost = match &action.authorization {
            Authorization::Signature(_, _) => self.costs.signature_verify,
            Authorization::Proof(_) => self.costs.proof_verify,
            Authorization::Breadstuff(_) => self.costs.signature_verify / 2, // cheaper
            Authorization::None => 0,
        };
        *computrons_used = computrons_used.saturating_add(auth_cost);
        if *computrons_used > budget {
            return Err((
                TurnError::BudgetExceeded { limit: budget, used: *computrons_used },
                path,
            ));
        }

        // Apply effects.
        for effect in &action.effects {
            let effect_cost = self.compute_effect_cost(effect);
            *computrons_used = computrons_used.saturating_add(effect_cost);
            if *computrons_used > budget {
                return Err((
                    TurnError::BudgetExceeded { limit: budget, used: *computrons_used },
                    path.clone(),
                ));
            }

            self.apply_effect(effect, ledger, &path, &action.target, parent_cell, journal)?;
            effects_hashes.push(effect.hash());
        }

        // Recurse into children.
        let child_delegation = match action.may_delegate {
            DelegationMode::None => DelegationMode::None,
            DelegationMode::ParentsOwn => DelegationMode::ParentsOwn,
            DelegationMode::Inherit => parent_delegation,
        };

        for (child_idx, child) in tree.children.iter().enumerate() {
            // Check delegation permission.
            if child_delegation == DelegationMode::None && child.action.target != action.target {
                return Err((
                    TurnError::DelegationDenied {
                        parent: action.target,
                        child_target: child.action.target,
                    },
                    {
                        let mut p = path.clone();
                        p.push(child_idx);
                        p
                    },
                ));
            }

            let mut child_path = path.clone();
            child_path.push(child_idx);

            self.execute_tree(
                child,
                ledger,
                &action.target, // current action's target becomes the parent for children
                child_delegation,
                computrons_used,
                budget,
                effects_hashes,
                child_path,
                journal,
            )?;
        }

        Ok(())
    }

    /// Check preconditions against the target cell's state.
    fn check_preconditions(
        &self,
        preconditions: &Preconditions,
        target_cell: &Cell,
        path: &[usize],
    ) -> Result<(), (TurnError, Vec<usize>)> {
        let ctx = EvalContext {
            block_height: self.block_height,
            timestamp: self.current_timestamp,
        };

        preconditions.evaluate(&target_cell.state, &ctx).map_err(|e| {
            (
                TurnError::PreconditionFailed {
                    description: format!("{e:?}"),
                },
                path.to_vec(),
            )
        })
    }

    /// Verify that the action's authorization satisfies the target cell's permission requirements.
    ///
    /// This checks ALL required permissions for ALL effects in the action (not just the first).
    /// For signature auth: verifies the Ed25519 signature against the cell's public key.
    /// For proof auth: delegates to the configured ProofVerifier (fail-closed if none set).
    fn verify_authorization(
        &self,
        action: &Action,
        target_cell: &Cell,
        ledger: &Ledger,
        path: &[usize],
    ) -> Result<(), (TurnError, Vec<usize>)> {
        // Determine ALL required permissions for this action's effects.
        let required_actions = self.determine_required_permissions(action);

        // Find the most restrictive auth requirement across all permissions.
        let mut most_restrictive = &AuthRequired::None;
        let mut most_restrictive_action_name = "Access";

        for (perm_action, action_name) in &required_actions {
            let auth_req = target_cell.permissions.for_action(*perm_action);
            if auth_req.is_narrower_or_equal(most_restrictive) {
                most_restrictive = auth_req;
                most_restrictive_action_name = action_name;
            }
        }

        // If no effects produced any specific permission, check general access.
        if required_actions.is_empty() {
            most_restrictive = target_cell.permissions.for_action(pyana_cell::permissions::Action::Access);
            most_restrictive_action_name = "Access";
        }

        // Now verify the authorization against the most restrictive requirement.
        self.check_single_auth_requirement(
            action,
            target_cell,
            most_restrictive,
            most_restrictive_action_name,
            path,
        )?;

        // Additionally, check Receive permission on transfer destinations.
        for effect in &action.effects {
            if let Effect::Transfer { to, .. } = effect {
                if let Some(dest_cell) = ledger.get(to) {
                    let receive_req = dest_cell.permissions.for_action(pyana_cell::permissions::Action::Receive);
                    if matches!(receive_req, AuthRequired::Impossible) {
                        return Err((
                            TurnError::PermissionDenied {
                                cell: *to,
                                action: "Receive".to_string(),
                                required: AuthRequired::Impossible,
                            },
                            path.to_vec(),
                        ));
                    }
                    if !matches!(receive_req, AuthRequired::None) {
                        return Err((
                            TurnError::PermissionDenied {
                                cell: *to,
                                action: "Receive".to_string(),
                                required: receive_req.clone(),
                            },
                            path.to_vec(),
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Check a single auth requirement against an action's authorization.
    fn check_single_auth_requirement(
        &self,
        action: &Action,
        target_cell: &Cell,
        auth_required: &AuthRequired,
        action_name: &str,
        path: &[usize],
    ) -> Result<(), (TurnError, Vec<usize>)> {
        match auth_required {
            AuthRequired::None => Ok(()),
            AuthRequired::Impossible => Err((
                TurnError::PermissionDenied {
                    cell: action.target,
                    action: action_name.to_string(),
                    required: AuthRequired::Impossible,
                },
                path.to_vec(),
            )),
            AuthRequired::Signature => match &action.authorization {
                Authorization::Signature(r, s) => {
                    self.verify_ed25519_signature(action, target_cell, r, s, path)
                }
                Authorization::Breadstuff(token) => {
                    self.check_breadstuff(target_cell, token, action_name, auth_required, path, action.target)
                }
                _ => Err((
                    TurnError::PermissionDenied {
                        cell: action.target,
                        action: action_name.to_string(),
                        required: AuthRequired::Signature,
                    },
                    path.to_vec(),
                )),
            },
            AuthRequired::Proof => match &action.authorization {
                Authorization::Proof(proof_bytes) => {
                    self.verify_zk_proof(action, target_cell, proof_bytes, path)
                }
                _ => Err((
                    TurnError::PermissionDenied {
                        cell: action.target,
                        action: action_name.to_string(),
                        required: AuthRequired::Proof,
                    },
                    path.to_vec(),
                )),
            },
            AuthRequired::Either => match &action.authorization {
                Authorization::Signature(r, s) => {
                    self.verify_ed25519_signature(action, target_cell, r, s, path)
                }
                Authorization::Proof(proof_bytes) => {
                    self.verify_zk_proof(action, target_cell, proof_bytes, path)
                }
                Authorization::Breadstuff(token) => {
                    self.check_breadstuff(target_cell, token, action_name, auth_required, path, action.target)
                }
                Authorization::None => Err((
                    TurnError::PermissionDenied {
                        cell: action.target,
                        action: action_name.to_string(),
                        required: AuthRequired::Either,
                    },
                    path.to_vec(),
                )),
            },
        }
    }

    /// Verify an Ed25519 signature against the target cell's public key.
    fn verify_ed25519_signature(
        &self,
        action: &Action,
        target_cell: &Cell,
        r: &[u8; 32],
        s: &[u8; 32],
        path: &[usize],
    ) -> Result<(), (TurnError, Vec<usize>)> {
        let message = Self::compute_signing_message(action);

        let mut sig_bytes = [0u8; 64];
        sig_bytes[..32].copy_from_slice(r);
        sig_bytes[32..].copy_from_slice(s);

        let signature = Signature::from_bytes(&sig_bytes);

        let verifying_key = VerifyingKey::from_bytes(&target_cell.public_key).map_err(|_| {
            (
                TurnError::InvalidAuthorization {
                    reason: "cell public key is not a valid Ed25519 point".to_string(),
                },
                path.to_vec(),
            )
        })?;

        use ed25519_dalek::Verifier;
        verifying_key.verify(&message, &signature).map_err(|_| {
            (
                TurnError::InvalidAuthorization {
                    reason: "Ed25519 signature verification failed".to_string(),
                },
                path.to_vec(),
            )
        })
    }

    /// Verify a ZK proof against the target cell's verification key.
    fn verify_zk_proof(
        &self,
        action: &Action,
        target_cell: &Cell,
        proof_bytes: &[u8],
        path: &[usize],
    ) -> Result<(), (TurnError, Vec<usize>)> {
        if proof_bytes.is_empty() {
            return Err((
                TurnError::InvalidAuthorization {
                    reason: "proof bytes are empty".to_string(),
                },
                path.to_vec(),
            ));
        }
        if proof_bytes.len() > 65536 {
            return Err((
                TurnError::InvalidAuthorization {
                    reason: format!("proof too large: {} bytes (max 65536)", proof_bytes.len()),
                },
                path.to_vec(),
            ));
        }

        let vk = target_cell.verification_key.as_ref().ok_or_else(|| {
            (
                TurnError::InvalidAuthorization {
                    reason: "cell requires proof but has no verification key".to_string(),
                },
                path.to_vec(),
            )
        })?;

        let verifier = self.proof_verifier.as_ref().ok_or_else(|| {
            (
                TurnError::InvalidAuthorization {
                    reason: "no proof verifier configured (fail-closed)".to_string(),
                },
                path.to_vec(),
            )
        })?;

        let public_inputs = Self::compute_signing_message(action);

        if verifier.verify(proof_bytes, &public_inputs, &vk.data) {
            Ok(())
        } else {
            Err((
                TurnError::InvalidAuthorization {
                    reason: "ZK proof verification failed".to_string(),
                },
                path.to_vec(),
            ))
        }
    }

    /// Check breadstuff (capability token) authorization.
    fn check_breadstuff(
        &self,
        target_cell: &Cell,
        token: &[u8; 32],
        action_name: &str,
        auth_required: &AuthRequired,
        path: &[usize],
        target_id: CellId,
    ) -> Result<(), (TurnError, Vec<usize>)> {
        let has_matching = target_cell.capabilities.iter().any(|cap| {
            cap.breadstuff.as_ref() == Some(token)
        });
        if has_matching {
            Ok(())
        } else {
            Err((
                TurnError::PermissionDenied {
                    cell: target_id,
                    action: action_name.to_string(),
                    required: auth_required.clone(),
                },
                path.to_vec(),
            ))
        }
    }

    /// Compute the message that should be signed for an action.
    pub fn compute_signing_message(action: &Action) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(action.target.as_bytes());
        hasher.update(&action.method);
        for arg in &action.args {
            hasher.update(arg);
        }
        for effect in &action.effects {
            hasher.update(&effect.hash());
        }
        hasher.update(&[action.may_delegate as u8]);
        *hasher.finalize().as_bytes()
    }

    /// Determine ALL required permissions for an action based on its effects.
    fn determine_required_permissions(
        &self,
        action: &Action,
    ) -> Vec<(pyana_cell::permissions::Action, &'static str)> {
        let mut result = Vec::new();
        let mut has_send = false;
        let mut has_set_state = false;
        let mut has_increment_nonce = false;
        let mut has_delegate = false;

        for effect in &action.effects {
            match effect {
                Effect::Transfer { from, .. } if from == &action.target && !has_send => {
                    result.push((pyana_cell::permissions::Action::Send, "Send"));
                    has_send = true;
                }
                Effect::SetField { .. } if !has_set_state => {
                    result.push((pyana_cell::permissions::Action::SetState, "SetState"));
                    has_set_state = true;
                }
                Effect::IncrementNonce { .. } if !has_increment_nonce => {
                    result.push((pyana_cell::permissions::Action::IncrementNonce, "IncrementNonce"));
                    has_increment_nonce = true;
                }
                Effect::GrantCapability { .. } if !has_delegate => {
                    result.push((pyana_cell::permissions::Action::Delegate, "Delegate"));
                    has_delegate = true;
                }
                Effect::RevokeCapability { .. } if !has_delegate => {
                    result.push((pyana_cell::permissions::Action::Delegate, "Delegate"));
                    has_delegate = true;
                }
                _ => {}
            }
        }

        result
    }

    /// Apply a single effect to the ledger, recording undo entries in the journal.
    ///
    /// SECURITY: For any effect that names a cell other than `action_target`,
    /// we verify that the actor holds a capability to that cell AND that the
    /// relevant permission on that cell allows the operation.
    fn apply_effect(
        &self,
        effect: &Effect,
        ledger: &mut Ledger,
        path: &[usize],
        action_target: &CellId,
        actor: &CellId,
        journal: &mut LedgerJournal,
    ) -> Result<(), (TurnError, Vec<usize>)> {
        match effect {
            Effect::SetField { cell, index, value } => {
                if *index >= STATE_SLOTS {
                    return Err((
                        TurnError::InvalidFieldIndex { cell: *cell, index: *index },
                        path.to_vec(),
                    ));
                }
                if cell != action_target {
                    self.check_cross_cell_permission(
                        ledger, actor, cell,
                        pyana_cell::permissions::Action::SetState, "SetState", path,
                    )?;
                }
                let c = ledger.get_mut(cell).ok_or_else(|| {
                    (TurnError::CellNotFound { id: *cell }, path.to_vec())
                })?;
                journal.record_set_field(*cell, *index, c.state.fields[*index]);
                c.state.fields[*index] = *value;
                Ok(())
            }

            Effect::Transfer { from, to, amount } => {
                if from != action_target {
                    self.check_cross_cell_permission(
                        ledger, actor, from,
                        pyana_cell::permissions::Action::Send, "Send", path,
                    )?;
                }
                let from_cell = ledger.get(from).ok_or_else(|| {
                    (TurnError::CellNotFound { id: *from }, path.to_vec())
                })?;
                if from_cell.state.balance < *amount {
                    return Err((
                        TurnError::InsufficientBalance {
                            cell: *from,
                            required: *amount,
                            available: from_cell.state.balance,
                        },
                        path.to_vec(),
                    ));
                }
                if ledger.get(to).is_none() {
                    return Err((
                        TurnError::TransferDestNotFound { id: *to },
                        path.to_vec(),
                    ));
                }
                let to_balance = ledger.get(to).unwrap().state.balance;
                if to_balance.checked_add(*amount).is_none() {
                    return Err((
                        TurnError::BalanceOverflow { cell: *to },
                        path.to_vec(),
                    ));
                }
                // Record old balances, then apply.
                let old_from_balance = ledger.get(from).unwrap().state.balance;
                let old_to_balance = ledger.get(to).unwrap().state.balance;
                journal.record_set_balance(*from, old_from_balance);
                journal.record_set_balance(*to, old_to_balance);
                ledger.get_mut(from).unwrap().state.balance -= *amount;
                ledger.get_mut(to).unwrap().state.balance += *amount;
                Ok(())
            }

            Effect::GrantCapability { from, to, cap } => {
                if from != action_target {
                    self.check_cross_cell_permission(
                        ledger, actor, from,
                        pyana_cell::permissions::Action::Delegate, "Delegate", path,
                    )?;
                }

                let from_cell = ledger.get(from).ok_or_else(|| {
                    (TurnError::CellNotFound { id: *from }, path.to_vec())
                })?;

                let held_cap = from_cell.capabilities.lookup_by_target(&cap.target)
                    .ok_or_else(|| {
                        (TurnError::CapabilityNotHeld { actor: *from, target: cap.target }, path.to_vec())
                    })?;

                if !pyana_cell::is_attenuation(&held_cap.permissions, &cap.permissions) {
                    return Err((
                        TurnError::DelegationDenied {
                            parent: *from,
                            child_target: *to,
                        },
                        path.to_vec(),
                    ));
                }

                let to_cell = ledger.get_mut(to).ok_or_else(|| {
                    (TurnError::CellNotFound { id: *to }, path.to_vec())
                })?;
                let granted_slot = to_cell.capabilities.grant_with_breadstuff(
                    cap.target,
                    cap.permissions.clone(),
                    cap.breadstuff,
                );
                journal.record_grant_capability(*to, granted_slot);
                Ok(())
            }

            Effect::RevokeCapability { cell, slot } => {
                if cell != action_target {
                    self.check_cross_cell_permission(
                        ledger, actor, cell,
                        pyana_cell::permissions::Action::Delegate, "Delegate", path,
                    )?;
                }
                let c = ledger.get_mut(cell).ok_or_else(|| {
                    (TurnError::CellNotFound { id: *cell }, path.to_vec())
                })?;
                if let Some(old_cap) = c.capabilities.lookup(*slot).cloned() {
                    journal.record_revoke_capability(*cell, old_cap);
                }
                c.capabilities.revoke(*slot);
                Ok(())
            }

            Effect::EmitEvent { cell, .. } => {
                if ledger.get(cell).is_none() {
                    return Err((
                        TurnError::CellNotFound { id: *cell },
                        path.to_vec(),
                    ));
                }
                Ok(())
            }

            Effect::IncrementNonce { cell } => {
                if cell != action_target {
                    self.check_cross_cell_permission(
                        ledger, actor, cell,
                        pyana_cell::permissions::Action::IncrementNonce, "IncrementNonce", path,
                    )?;
                }
                let c = ledger.get_mut(cell).ok_or_else(|| {
                    (TurnError::CellNotFound { id: *cell }, path.to_vec())
                })?;
                journal.record_set_nonce(*cell, c.state.nonce);
                c.state.increment_nonce();
                Ok(())
            }

            Effect::CreateCell { public_key, token_id, balance } => {
                let new_cell = Cell::with_balance(*public_key, *token_id, *balance);
                let id = new_cell.id;
                ledger.insert_cell(new_cell).map_err(|_| {
                    (TurnError::CellAlreadyExists { id }, path.to_vec())
                })?;
                journal.record_create_cell(id);
                Ok(())
            }
        }
    }

    /// SECURITY: Check that the actor holds a capability to the given cell AND that
    /// the cell's permission for the given action is not denied.
    fn check_cross_cell_permission(
        &self,
        ledger: &Ledger,
        actor: &CellId,
        target_cell_id: &CellId,
        permission_action: pyana_cell::permissions::Action,
        action_name: &str,
        path: &[usize],
    ) -> Result<(), (TurnError, Vec<usize>)> {
        if actor != target_cell_id {
            let actor_cell = ledger.get(actor).ok_or_else(|| {
                (TurnError::CellNotFound { id: *actor }, path.to_vec())
            })?;
            if !actor_cell.capabilities.has_access(target_cell_id) {
                return Err((
                    TurnError::CapabilityNotHeld {
                        actor: *actor,
                        target: *target_cell_id,
                    },
                    path.to_vec(),
                ));
            }
        }

        let cell = ledger.get(target_cell_id).ok_or_else(|| {
            (TurnError::CellNotFound { id: *target_cell_id }, path.to_vec())
        })?;
        let required = cell.permissions.for_action(permission_action);
        if matches!(required, AuthRequired::Impossible) {
            return Err((
                TurnError::PermissionDenied {
                    cell: *target_cell_id,
                    action: action_name.to_string(),
                    required: required.clone(),
                },
                path.to_vec(),
            ));
        }
        if !matches!(required, AuthRequired::None) {
            return Err((
                TurnError::PermissionDenied {
                    cell: *target_cell_id,
                    action: action_name.to_string(),
                    required: required.clone(),
                },
                path.to_vec(),
            ));
        }

        Ok(())
    }

    /// Compute the cost of a single effect.
    fn compute_effect_cost(&self, effect: &Effect) -> u64 {
        let base = self.costs.effect_base;
        let extra = match effect {
            Effect::Transfer { .. } => self.costs.transfer,
            Effect::CreateCell { .. } => self.costs.create_cell,
            Effect::SetField { .. } => 0,
            Effect::GrantCapability { .. } => self.costs.effect_base,
            Effect::RevokeCapability { .. } => 0,
            Effect::EmitEvent { event, .. } => {
                (event.data.len() as u64) * self.costs.per_byte * 32
            }
            Effect::IncrementNonce { .. } => 0,
        };
        base.saturating_add(extra).saturating_add(
            (effect.data_bytes() as u64).saturating_mul(self.costs.per_byte),
        )
    }

    /// Estimate the cost of a tree (without actually applying it).
    fn estimate_tree_cost(&self, tree: &CallTree) -> u64 {
        let mut total = self.costs.action_base;

        total = total.saturating_add(match &tree.action.authorization {
            Authorization::Signature(_, _) => self.costs.signature_verify,
            Authorization::Proof(_) => self.costs.proof_verify,
            Authorization::Breadstuff(_) => self.costs.signature_verify / 2,
            Authorization::None => 0,
        });

        for effect in &tree.action.effects {
            total = total.saturating_add(self.compute_effect_cost(effect));
        }

        for child in &tree.children {
            total = total.saturating_add(self.estimate_tree_cost(child));
        }

        total
    }

    /// Compute a fresh state hash from the ledger by iterating all cells.
    #[allow(dead_code)]
    fn compute_state_hash(ledger: &Ledger) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        let mut entries: Vec<_> = ledger.iter().collect();
        entries.sort_by_key(|(id, _)| *id.as_bytes());
        for (id, cell) in entries {
            hasher.update(id.as_bytes());
            hasher.update(&cell.public_key);
            hasher.update(&cell.token_id);
            hasher.update(&cell.state.nonce.to_le_bytes());
            hasher.update(&cell.state.balance.to_le_bytes());
            for field in &cell.state.fields {
                hasher.update(field);
            }
        }
        *hasher.finalize().as_bytes()
    }

    /// Compute the BLAKE3 hash of all effect hashes combined.
    fn compute_effects_hash(&self, effect_hashes: &[[u8; 32]]) -> [u8; 32] {
        if effect_hashes.is_empty() {
            return [0u8; 32];
        }
        let mut hasher = blake3::Hasher::new();
        for h in effect_hashes {
            hasher.update(h);
        }
        *hasher.finalize().as_bytes()
    }

    /// Compute a LedgerDelta from journal entries and the current (post-mutation) ledger.
    ///
    /// The journal records the old (pre-mutation) values. By comparing those to the
    /// current state in the ledger, we derive the delta without needing a full ledger snapshot.
    fn compute_delta_from_journal(journal: &LedgerJournal, ledger: &Ledger) -> LedgerDelta {
        use std::collections::{HashMap, HashSet};

        let mut delta = LedgerDelta::new();
        let mut created_cells: HashSet<CellId> = HashSet::new();
        let mut updated_cells: HashMap<CellId, CellStateDelta> = HashMap::new();

        // Track the FIRST old balance/nonce per cell (the pre-turn value).
        let mut first_balance: HashMap<CellId, u64> = HashMap::new();
        let mut first_nonce: HashMap<CellId, u64> = HashMap::new();
        let mut first_fields: HashMap<(CellId, usize), [u8; 32]> = HashMap::new();

        for entry in journal.entries() {
            match entry {
                JournalEntry::CreateCell { cell } => {
                    if let Some(c) = ledger.get(cell) {
                        delta.created.push(c.clone());
                        created_cells.insert(*cell);
                    }
                }
                JournalEntry::SetField { cell, index, old_value } => {
                    if !created_cells.contains(cell) {
                        first_fields.entry((*cell, *index)).or_insert(*old_value);
                    }
                }
                JournalEntry::SetBalance { cell, old_balance } => {
                    if !created_cells.contains(cell) {
                        first_balance.entry(*cell).or_insert(*old_balance);
                    }
                }
                JournalEntry::SetNonce { cell, old_nonce } => {
                    if !created_cells.contains(cell) {
                        first_nonce.entry(*cell).or_insert(*old_nonce);
                    }
                }
                JournalEntry::GrantCapability { cell, slot } => {
                    if !created_cells.contains(cell) {
                        if let Some(c) = ledger.get(cell) {
                            if let Some(cap_ref) = c.capabilities.lookup(*slot) {
                                let e = updated_cells.entry(*cell).or_insert_with(CellStateDelta::empty);
                                e.capability_grants.push(cap_ref.clone());
                            }
                        }
                    }
                }
                JournalEntry::RevokeCapability { cell, old_cap } => {
                    if !created_cells.contains(cell) {
                        let e = updated_cells.entry(*cell).or_insert_with(CellStateDelta::empty);
                        e.capability_revocations.push(old_cap.slot);
                    }
                }
            }
        }

        // Compute field/balance/nonce deltas from first-old vs current.
        for ((cell_id, index), old_value) in &first_fields {
            if let Some(c) = ledger.get(cell_id) {
                let new_value = c.state.fields[*index];
                if new_value != *old_value {
                    let e = updated_cells.entry(*cell_id).or_insert_with(CellStateDelta::empty);
                    e.field_updates.push((*index, new_value));
                }
            }
        }

        for (cell_id, old_balance) in &first_balance {
            if let Some(c) = ledger.get(cell_id) {
                let diff = c.state.balance as i128 - *old_balance as i128;
                if diff != 0 {
                    let e = updated_cells.entry(*cell_id).or_insert_with(CellStateDelta::empty);
                    e.balance_change = diff as i64;
                }
            }
        }

        for (cell_id, old_nonce) in &first_nonce {
            if let Some(c) = ledger.get(cell_id) {
                if c.state.nonce > *old_nonce {
                    let e = updated_cells.entry(*cell_id).or_insert_with(CellStateDelta::empty);
                    e.nonce_increment = true;
                }
            }
        }

        // Collect non-empty cell deltas.
        for (cell_id, cell_delta) in updated_cells {
            if !cell_delta.field_updates.is_empty()
                || cell_delta.nonce_increment
                || cell_delta.balance_change != 0
                || cell_delta.permission_changes.is_some()
                || !cell_delta.capability_grants.is_empty()
                || !cell_delta.capability_revocations.is_empty()
            {
                delta.updated.push((cell_id, cell_delta));
            }
        }

        delta
    }
}
