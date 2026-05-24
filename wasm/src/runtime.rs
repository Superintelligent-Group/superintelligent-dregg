//! PyanaRuntime: full in-browser distributed system simulation.
//!
//! Encapsulates a complete pyana environment with:
//! - A Ledger (cells + Merkle state)
//! - A TurnExecutor
//! - A NullifierSet (note double-spend tracking)
//! - An IntentPool (simplified)
//! - A RevocationChannelSet
//! - Multiple AgentWallet instances (for multi-party simulation)
//! - Federation simulation (in-memory, no networking)

use std::collections::HashMap;

use serde::Serialize;
use zeroize::Zeroizing;

use pyana_cell::{
    AuthRequired, Cell, CellId, Ledger, Note, NoteCommitment, Nullifier, NullifierSet,
    RevocationChannel, RevocationChannelSet,
};
use pyana_intent::matcher::{HeldCapability, MatchResult, Sensitivity, match_intent};
use pyana_intent::{
    ActionPattern, CommitmentId, Constraint, Intent, IntentKind, MatchSpec, VerificationMode,
};
use pyana_sdk::AgentWallet;
use pyana_turn::action::Authorization;
use pyana_turn::conditional::{ConditionalTurn, ProofCondition};
use pyana_turn::forest::CallTree;
use pyana_turn::{
    ComputronCosts, Effect, Turn, TurnBuilder, TurnExecutor, TurnReceipt, TurnResult,
};

/// Cell-ID domain shared by every wasm-sim agent. AgentWallet derives the
/// CellId deterministically as `f(public_key, domain)` so this string is part
/// of the agent's identity surface.
const WASM_SIM_DOMAIN: &str = "pyana-wasm-default-domain";

// ============================================================================
// Internal state types
// ============================================================================

/// An agent in the wasm runtime: a real `pyana_sdk::AgentWallet` plus the
/// auxiliary state we need for in-browser scenarios (cached cell_id, an
/// intent-matcher-shaped token list, a commitment id, a counter for token-id
/// generation, and a friendly name).
///
/// `held_tokens` here is the `pyana_intent::matcher::HeldCapability` shape
/// used by the intent matcher — distinct from `wallet.tokens()` which is
/// the SDK's macaroon-backed `HeldToken`. Both legitimately coexist.
#[derive(Debug)]
pub struct SimAgent {
    pub name: String,
    pub wallet: AgentWallet,
    pub public_key: [u8; 32],
    pub cell_id: CellId,
    pub held_tokens: Vec<HeldCapability>,
    pub commitment_id: CommitmentId,
    pub token_counter: u64,
}

// Federation types intentionally removed from the wasm runtime.
//
// The canonical implementation lives in `pyana_federation` (Federation,
// FederationNode, FederationReceipt). That crate currently pulls `tokio`
// (full) and `crossbeam-channel` and so does not cross-compile to wasm32.
// Rather than reintroduce a parallel "Sim*" set of types that drift from
// the canonical behavior, the runtime has no federation surface until
// `pyana_federation` gains a wasm32-compatible feature gate. Federation
// inspectors in the Studio show a "coming soon" state until then.

/// A pending conditional turn.
#[derive(Clone, Debug)]
pub struct PendingConditional {
    pub id: [u8; 32],
    pub conditional: ConditionalTurn,
    pub submitted_height: u64,
}

/// Execution trace step (for step-by-step visualization).
#[derive(Clone, Debug, Serialize)]
pub struct TraceStep {
    pub action_path: Vec<usize>,
    pub target_cell: String,
    pub method: String,
    pub effects: Vec<String>,
    pub result: String,
    pub computrons_used: u64,
}

// ============================================================================
// PyanaRuntime: the core state container
// ============================================================================

/// The main runtime struct holding all simulation state.
/// NOT exposed directly via wasm_bindgen (we use an index-based handle instead).
pub struct PyanaRuntime {
    pub ledger: Ledger,
    pub executor: TurnExecutor,
    pub nullifier_set: NullifierSet,
    pub agents: Vec<SimAgent>,
    pub agent_names: HashMap<String, usize>,
    pub intents: Vec<Intent>,
    pub revocation_channels: RevocationChannelSet,
    pub conditionals: Vec<PendingConditional>,
    pub current_height: u64,
    pub current_timestamp: i64,
    pub receipts: Vec<TurnReceipt>,
}

impl PyanaRuntime {
    pub fn new() -> Self {
        let costs = ComputronCosts::default_costs();
        let mut executor = TurnExecutor::new(costs);
        executor.set_timestamp(1000);
        executor.set_block_height(0);

        PyanaRuntime {
            ledger: Ledger::new(),
            executor,
            nullifier_set: NullifierSet::new(),
            agents: Vec::new(),
            agent_names: HashMap::new(),
            intents: Vec::new(),
            revocation_channels: RevocationChannelSet::new(),
            conditionals: Vec::new(),
            current_height: 0,
            current_timestamp: 1000,
            receipts: Vec::new(),
        }
    }

    /// Create an agent with a name. The Ed25519 key is derived deterministically
    /// from (name, idx) so a reproducible browser session can replay an
    /// identical history. The derivation is BLAKE3-of-name-and-index for the
    /// seed; the rest of the agent — public key, cell id, signing — comes
    /// from `pyana_sdk::AgentWallet`, the same wallet used by native callers.
    /// This is not a sim-shaped reimplementation; the wallet IS the canonical
    /// implementation, just constructed with a deterministic seed for
    /// reproducibility.
    pub fn create_agent(&mut self, name: &str, initial_balance: u64) -> usize {
        let idx = self.agents.len();

        // Deterministic Ed25519 seed.
        let mut hasher = blake3::Hasher::new_derive_key("pyana-wasm-agent-key");
        hasher.update(name.as_bytes());
        hasher.update(&(idx as u64).to_le_bytes());
        let key_hash = hasher.finalize();
        let seed_bytes: [u8; 32] = *key_hash.as_bytes();

        // CommitmentId derivation needs the raw seed; compute it before the
        // seed is moved into the wallet (where it's zeroized).
        let commitment_id = CommitmentId::derive(&seed_bytes, "pyana-wasm-commitment");

        let wallet = AgentWallet::from_key_bytes(Zeroizing::new(seed_bytes));
        let public_key = wallet.public_key().0;
        let cell_id = wallet.cell_id(WASM_SIM_DOMAIN);

        // Create the cell in the ledger. (Genesis-by-fiat for now; eventual
        // refactor is to mint cells via Effect::CreateCell. See task #15.)
        let token_id: [u8; 32] = *blake3::hash(WASM_SIM_DOMAIN.as_bytes()).as_bytes();
        let cell = Cell::with_balance(public_key, token_id, initial_balance);
        self.ledger.insert_cell(cell).unwrap();

        let agent = SimAgent {
            name: name.to_string(),
            wallet,
            public_key,
            cell_id,
            held_tokens: Vec::new(),
            commitment_id,
            token_counter: 0,
        };

        self.agent_names.insert(name.to_string(), idx);
        self.agents.push(agent);
        idx
    }

    /// Mint a token for an agent (adds to their held_tokens for intent matching).
    pub fn agent_mint_token(
        &mut self,
        agent_idx: usize,
        resource: &str,
        actions: &[String],
        expiry: Option<u64>,
    ) -> usize {
        let agent = &mut self.agents[agent_idx];
        agent.token_counter += 1;
        let token_id = format!("tok_{}_{}", agent.name, agent.token_counter);

        let held = HeldCapability {
            token_id,
            actions: actions.to_vec(),
            resource: resource.to_string(),
            app_id: None,
            service: None,
            user_id: None,
            features: Vec::new(),
            oauth_provider: None,
            expiry,
            budget: None,
            sensitivity: Sensitivity::Normal,
        };

        let idx = agent.held_tokens.len();
        agent.held_tokens.push(held);
        idx
    }

    /// Grant a capability from one agent's cell to another agent's cell.
    pub fn grant_capability(
        &mut self,
        from_agent: usize,
        to_agent: usize,
        permissions: AuthRequired,
    ) -> Option<u32> {
        let from_cell_id = self.agents[from_agent].cell_id;
        let to_cell_id = self.agents[to_agent].cell_id;

        // Grant capability on the target cell (to_agent gets cap pointing to from_agent).
        let to_cell = self.ledger.get_mut(&to_cell_id)?;
        to_cell.capabilities.grant(from_cell_id, permissions)
    }

    /// Build and execute a turn using the TurnBuilder API.
    ///
    /// The legacy `TurnBuilder::action()` API stamps every action with
    /// `Authorization::Unchecked`, which gets rejected by cells with default
    /// (`Signature`-required) permissions. We post-process the built turn,
    /// walking the call forest and replacing every `Unchecked` authorization
    /// with a real Ed25519 signature from the agent's signing key. The
    /// TurnExecutor verifies these signatures against the cell's stored
    /// public key — the same code path real wallets exercise.
    pub fn execute_turn_for_agent(
        &mut self,
        agent_idx: usize,
        effects: Vec<Effect>,
        fee: u64,
    ) -> TurnResult {
        let cell_id = self.agents[agent_idx].cell_id;

        // Get current nonce.
        let nonce = self
            .ledger
            .get(&cell_id)
            .map(|c| c.state.nonce())
            .unwrap_or(0);

        let mut builder = TurnBuilder::new(cell_id, nonce);
        builder.set_fee(fee);

        {
            let action = builder.action(cell_id, "execute");
            for effect in effects {
                action.effect(effect);
            }
        }

        let mut turn = builder.build();

        // Receipt chaining: every turn after the first from a given agent must
        // reference the previous turn's receipt hash. The executor tracks the
        // per-agent head; reuse it so callers don't have to.
        if turn.previous_receipt_hash.is_none() {
            if let Some(prev) = self.executor.get_last_receipt_hash(&cell_id) {
                turn.previous_receipt_hash = Some(prev);
            }
        }

        // Sign every Unchecked action with the agent's wallet — same code
        // path native callers exercise via `AgentWallet::sign_action`.
        let federation_id = self.executor.local_federation_id;
        let wallet = &self.agents[agent_idx].wallet;
        sign_call_forest(&mut turn, wallet, &federation_id);

        let result = self.executor.execute(&turn, &mut self.ledger);

        if let TurnResult::Committed { ref receipt, .. } = result {
            self.receipts.push(receipt.clone());
        }

        result
    }

    /// Create a note for an agent. Randomness derives deterministically from
    /// the wallet (so the same agent + same value yields the same commitment
    /// for reproducibility), via `AgentWallet::derive_symmetric_key` rather
    /// than exposing raw signing material.
    pub fn create_note(&mut self, agent_idx: usize, value: u64, asset_type: u64) -> NoteCommitment {
        let agent = &self.agents[agent_idx];
        let mut fields = [0u64; 8];
        fields[0] = asset_type;
        fields[1] = value;
        let randomness = agent
            .wallet
            .derive_symmetric_key("pyana-wasm-note-randomness");
        let note = Note::with_randomness(agent.public_key, fields, randomness);
        note.commitment()
    }

    /// Spend a note (reveal nullifier). Spending key derived from the wallet
    /// the same way `create_note` derives randomness — same deterministic
    /// key so the nullifier is reproducible.
    pub fn spend_note(
        &mut self,
        agent_idx: usize,
        value: u64,
        asset_type: u64,
    ) -> Result<Nullifier, String> {
        let agent = &self.agents[agent_idx];
        let mut fields = [0u64; 8];
        fields[0] = asset_type;
        fields[1] = value;
        let randomness = agent
            .wallet
            .derive_symmetric_key("pyana-wasm-note-randomness");
        let spending = agent
            .wallet
            .derive_symmetric_key("pyana-wasm-note-spending");
        let note = Note::with_randomness(agent.public_key, fields, randomness);
        let nullifier = note.nullifier(&spending);
        self.nullifier_set
            .insert(nullifier)
            .map_err(|e| e.to_string())?;
        Ok(nullifier)
    }

    // create_federation / propose_block / simulate_consensus_round removed:
    // they backed wasm-fictional federation/consensus that didn't reflect
    // pyana_federation::{Federation, FederationNode, FederationReceipt}.
    // Federation views in the Studio show "awaiting pyana-federation wasm32
    // support" until that crate gains the same feature surgery pyana-sdk got.

    /// Create an intent.
    pub fn create_intent(
        &mut self,
        agent_idx: usize,
        kind: IntentKind,
        actions: Vec<ActionPattern>,
        constraints: Vec<Constraint>,
        resource_pattern: Option<String>,
        expiry: u64,
    ) -> [u8; 32] {
        let agent = &self.agents[agent_idx];
        let spec = MatchSpec {
            actions,
            constraints,
            min_budget: None,
            resource_pattern,
            compound: None,
            predicate_requirements: vec![],
            strict_resource_matching: false,
        };
        let intent = Intent::new(kind, spec, agent.commitment_id, expiry, None);
        let id = intent.id;
        self.intents.push(intent);
        id
    }

    /// Match an intent against an agent's held tokens.
    pub fn match_intent_for_agent(&self, intent_idx: usize, agent_idx: usize) -> MatchResult {
        let intent = &self.intents[intent_idx];
        let agent = &self.agents[agent_idx];
        match_intent(
            intent,
            &agent.held_tokens,
            agent.commitment_id,
            VerificationMode::Trusted,
            self.current_timestamp as u64,
        )
    }

    /// Submit a conditional turn.
    pub fn submit_conditional(
        &mut self,
        agent_idx: usize,
        effects: Vec<Effect>,
        fee: u64,
        condition: ProofCondition,
        timeout_blocks: u64,
    ) -> [u8; 32] {
        let agent = &self.agents[agent_idx];
        let cell_id = agent.cell_id;
        let nonce = self
            .ledger
            .get(&cell_id)
            .map(|c| c.state.nonce())
            .unwrap_or(0);

        let mut builder = TurnBuilder::new(cell_id, nonce);
        builder.set_fee(fee);
        {
            let action = builder.action(cell_id, "conditional");
            for effect in effects {
                action.effect(effect);
            }
        }
        let turn = builder.build();
        let turn_hash = turn.hash();

        let deposit_amount = pyana_turn::compute_conditional_deposit(
            self.current_height + timeout_blocks,
            self.current_height,
        );
        let conditional = ConditionalTurn {
            turn,
            condition,
            timeout_height: self.current_height + timeout_blocks,
            submitted_at: self.current_height,
            deposit_amount,
        };

        self.conditionals.push(PendingConditional {
            id: turn_hash,
            conditional,
            submitted_height: self.current_height,
        });

        turn_hash
    }

    /// Advance the block height (for timeout simulation).
    pub fn advance_height(&mut self, blocks: u64) {
        self.current_height += blocks;
        self.current_timestamp += (blocks * 12) as i64; // ~12s per block
        self.executor.set_block_height(self.current_height);
        self.executor.set_timestamp(self.current_timestamp);
    }

    /// Create a revocation channel.
    pub fn create_revocation_channel(&mut self, revoker_agent: usize) -> [u8; 32] {
        let revoker_cell_id = self.agents[revoker_agent].cell_id;
        let nonce = self.revocation_channels.len() as u64;
        let channel = RevocationChannel::new(revoker_cell_id, nonce, self.current_height);
        let channel_id = channel.channel_id;
        self.revocation_channels.register(channel).unwrap();
        channel_id
    }

    /// Trip (revoke) a channel.
    pub fn trip_channel(
        &mut self,
        channel_id: &[u8; 32],
        revoker_agent: usize,
        reason: [u8; 32],
    ) -> bool {
        let revoker_cell_id = self.agents[revoker_agent].cell_id;
        self.revocation_channels
            .trip_channel(channel_id, &revoker_cell_id, reason, self.current_height)
            .is_ok()
    }

    /// Check if a channel is active.
    pub fn is_channel_active(&self, channel_id: &[u8; 32]) -> bool {
        self.revocation_channels
            .get(channel_id)
            .map(|ch| ch.state.is_active())
            .unwrap_or(false)
    }
}

// ConsensusRoundResult removed alongside simulate_consensus_round.

/// Walk the turn's call forest and replace every `Authorization::Unchecked`
/// with a real Ed25519 signature via `AgentWallet::sign_action`. Existing
/// non-Unchecked authorizations are left intact so callers can pre-sign or
/// pre-prove specific actions. Uses the SDK's canonical signing path — no
/// hand-rolled cryptography.
fn sign_call_forest(turn: &mut Turn, wallet: &AgentWallet, federation_id: &[u8; 32]) {
    for tree in &mut turn.call_forest.roots {
        sign_call_tree(tree, wallet, federation_id);
    }
    // Mutating actions invalidates any cached forest hash; clear so the
    // executor recomputes from the now-signed actions.
    turn.call_forest.forest_hash = [0u8; 32];
}

fn sign_call_tree(tree: &mut CallTree, wallet: &AgentWallet, federation_id: &[u8; 32]) {
    if matches!(tree.action.authorization, Authorization::Unchecked) {
        // Clone the action because sign_action returns a fresh one; replace in place.
        tree.action = wallet.sign_action(tree.action.clone(), federation_id);
    }
    tree.hash = [0u8; 32]; // invalidate cached action hash
    for child in &mut tree.children {
        sign_call_tree(child, wallet, federation_id);
    }
}
