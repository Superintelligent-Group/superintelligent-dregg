//! TurnBuilder and ActionBuilder: ergonomic APIs for constructing turns.
//!
//! These builders provide a fluent interface for constructing turns and actions
//! without manually assembling all the nested structures.

use pyana_cell::{CapabilityRef, CellId, Preconditions};
use pyana_cell::state::FieldElement;

use crate::action::{Action, Authorization, DelegationMode, Effect, Event, symbol};
use crate::forest::CallForest;
use crate::turn::Turn;

/// Builder for constructing a Turn step by step.
pub struct TurnBuilder {
    agent: CellId,
    nonce: u64,
    fee: u64,
    memo: Option<String>,
    valid_until: Option<i64>,
    action_builders: Vec<ActionBuilder>,
}

impl TurnBuilder {
    /// Create a new TurnBuilder for the given agent and nonce.
    pub fn new(agent: CellId, nonce: u64) -> Self {
        TurnBuilder {
            agent,
            nonce,
            fee: 0,
            memo: None,
            valid_until: None,
            action_builders: Vec::new(),
        }
    }

    /// Add a root-level action targeting the given cell with the given method.
    /// Returns a mutable reference to the ActionBuilder for further configuration.
    pub fn action(&mut self, target: CellId, method: &str) -> &mut ActionBuilder {
        self.action_builders.push(ActionBuilder::new(target, method));
        self.action_builders.last_mut().unwrap()
    }

    /// Set the computron fee for this turn.
    pub fn fee(mut self, fee: u64) -> Self {
        self.fee = fee;
        self
    }

    /// Set the fee (chainable from &mut self).
    pub fn set_fee(&mut self, fee: u64) -> &mut Self {
        self.fee = fee;
        self
    }

    /// Set an optional memo.
    pub fn memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = Some(memo.into());
        self
    }

    /// Set the memo (chainable from &mut self).
    pub fn set_memo(&mut self, memo: impl Into<String>) -> &mut Self {
        self.memo = Some(memo.into());
        self
    }

    /// Set the expiration timestamp.
    pub fn valid_until(mut self, ts: i64) -> Self {
        self.valid_until = Some(ts);
        self
    }

    /// Set the expiration timestamp (chainable from &mut self).
    pub fn set_valid_until(&mut self, ts: i64) -> &mut Self {
        self.valid_until = Some(ts);
        self
    }

    /// Build the Turn from the accumulated configuration.
    pub fn build(self) -> Turn {
        let mut forest = CallForest::new();

        for ab in self.action_builders {
            let tree = forest.add_root(ab.build_action());
            // Add children recursively.
            ab.build_children_into(tree);
        }

        Turn {
            agent: self.agent,
            nonce: self.nonce,
            call_forest: forest,
            fee: self.fee,
            memo: self.memo,
            valid_until: self.valid_until,
        }
    }
}

/// Builder for constructing an Action with its children.
pub struct ActionBuilder {
    target: CellId,
    method: String,
    args: Vec<FieldElement>,
    authorization: Authorization,
    preconditions: Preconditions,
    effects: Vec<Effect>,
    may_delegate: DelegationMode,
    children: Vec<ActionBuilder>,
}

impl ActionBuilder {
    /// Create a new ActionBuilder.
    pub fn new(target: CellId, method: &str) -> Self {
        ActionBuilder {
            target,
            method: method.to_string(),
            args: Vec::new(),
            authorization: Authorization::None,
            preconditions: Preconditions::default(),
            effects: Vec::new(),
            may_delegate: DelegationMode::None,
            children: Vec::new(),
        }
    }

    /// Add an argument to the action.
    pub fn arg(&mut self, value: FieldElement) -> &mut Self {
        self.args.push(value);
        self
    }

    /// Set the authorization to a signature.
    pub fn authorize_signature(&mut self, sig: [u8; 64]) -> &mut Self {
        self.authorization = Authorization::from_sig_bytes(sig);
        self
    }

    /// Set the authorization to a ZK proof.
    pub fn authorize_proof(&mut self, proof: Vec<u8>) -> &mut Self {
        self.authorization = Authorization::Proof(proof);
        self
    }

    /// Set the authorization to a breadstuff capability token.
    pub fn authorize_breadstuff(&mut self, token: [u8; 32]) -> &mut Self {
        self.authorization = Authorization::Breadstuff(token);
        self
    }

    /// Add an effect to this action.
    pub fn effect(&mut self, effect: Effect) -> &mut Self {
        self.effects.push(effect);
        self
    }

    /// Set the delegation mode for children.
    pub fn delegation(&mut self, mode: DelegationMode) -> &mut Self {
        self.may_delegate = mode;
        self
    }

    /// Set a nonce precondition.
    pub fn require_nonce(&mut self, nonce: u64) -> &mut Self {
        let cell_pre = self.preconditions.cell_state.get_or_insert_with(Default::default);
        cell_pre.nonce = Some(nonce);
        self
    }

    /// Set a minimum balance precondition.
    pub fn require_min_balance(&mut self, min: u64) -> &mut Self {
        let cell_pre = self.preconditions.cell_state.get_or_insert_with(Default::default);
        cell_pre.min_balance = Some(min);
        self
    }

    /// Set a state field equality precondition.
    pub fn require_field_equals(&mut self, index: usize, value: FieldElement) -> &mut Self {
        let cell_pre = self.preconditions.cell_state.get_or_insert_with(Default::default);
        cell_pre.field_equals.push((index, value));
        self
    }

    /// Set preconditions directly.
    pub fn preconditions(&mut self, pre: Preconditions) -> &mut Self {
        self.preconditions = pre;
        self
    }

    /// Add a child action.
    pub fn child(&mut self, target: CellId, method: &str) -> &mut ActionBuilder {
        self.children.push(ActionBuilder::new(target, method));
        self.children.last_mut().unwrap()
    }

    /// Build the Action (without children — those are attached separately).
    fn build_action(&self) -> Action {
        Action {
            target: self.target,
            method: symbol(&self.method),
            args: self.args.clone(),
            authorization: self.authorization.clone(),
            preconditions: self.preconditions.clone(),
            effects: self.effects.clone(),
            may_delegate: self.may_delegate,
        }
    }

    /// Recursively attach children to a CallTree node.
    fn build_children_into(self, tree: &mut crate::forest::CallTree) {
        for child_builder in self.children {
            let child_action = child_builder.build_action();
            let child_tree = tree.add_child(child_action);
            child_builder.build_children_into(child_tree);
        }
    }
}

/// Convenience functions for building common effect types.
impl ActionBuilder {
    /// Add a SetField effect.
    pub fn set_field(&mut self, cell: CellId, index: usize, value: FieldElement) -> &mut Self {
        self.effects.push(Effect::SetField { cell, index, value });
        self
    }

    /// Add a Transfer effect.
    pub fn transfer(&mut self, from: CellId, to: CellId, amount: u64) -> &mut Self {
        self.effects.push(Effect::Transfer { from, to, amount });
        self
    }

    /// Add an IncrementNonce effect.
    pub fn increment_nonce(&mut self, cell: CellId) -> &mut Self {
        self.effects.push(Effect::IncrementNonce { cell });
        self
    }

    /// Add an EmitEvent effect.
    pub fn emit_event(&mut self, cell: CellId, topic: &str, data: Vec<FieldElement>) -> &mut Self {
        self.effects.push(Effect::EmitEvent {
            cell,
            event: Event::new(symbol(topic), data),
        });
        self
    }

    /// Add a GrantCapability effect.
    pub fn grant_capability(
        &mut self,
        from: CellId,
        to: CellId,
        cap: CapabilityRef,
    ) -> &mut Self {
        self.effects.push(Effect::GrantCapability { from, to, cap });
        self
    }

    /// Add a RevokeCapability effect.
    pub fn revoke_capability(&mut self, cell: CellId, slot: u32) -> &mut Self {
        self.effects.push(Effect::RevokeCapability { cell, slot });
        self
    }

    /// Add a CreateCell effect.
    pub fn create_cell(
        &mut self,
        public_key: [u8; 32],
        token_id: [u8; 32],
        balance: u64,
    ) -> &mut Self {
        self.effects.push(Effect::CreateCell { public_key, token_id, balance });
        self
    }
}
