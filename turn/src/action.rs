//! Action types: the building blocks of a call forest.
//!
//! An Action is a single operation in the call forest, analogous to Mina's AccountUpdate.
//! Each action targets a cell, specifies a method, carries authorization, declares
//! preconditions, and produces effects.

use pyana_cell::{CellId, CapabilityRef, Preconditions};
use pyana_cell::state::FieldElement;
use serde::{Deserialize, Serialize};

/// A Symbol is a BLAKE3-hashed method or topic name, stored as a field element.
pub type Symbol = FieldElement;

/// Compute a symbol from a string name.
pub fn symbol(name: &str) -> Symbol {
    *blake3::hash(name.as_bytes()).as_bytes()
}

/// A single operation in the call forest.
///
/// Analogous to Mina's AccountUpdate: targets a cell, performs a method,
/// requires authorization, checks preconditions, and produces effects.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    /// Which cell is being acted upon.
    pub target: CellId,
    /// What operation (method name hashed to symbol).
    pub method: Symbol,
    /// Arguments to the operation.
    pub args: Vec<FieldElement>,
    /// How this action is authorized.
    pub authorization: Authorization,
    /// What must be true before this action can execute.
    pub preconditions: Preconditions,
    /// What changes result from this action.
    pub effects: Vec<Effect>,
    /// Can children use parent's capabilities?
    pub may_delegate: DelegationMode,
}

/// How an action is authorized.
///
/// Maps to the authorization models in Mina: signature, proof, or none.
/// Adds `Breadstuff` for capability token authorization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Authorization {
    /// Ed25519 signature over the action hash (stored as two 32-byte halves).
    Signature([u8; 32], [u8; 32]),
    /// Zero-knowledge proof bytes (opaque to the executor).
    Proof(Vec<u8>),
    /// Capability token hash (breadstuff authorization).
    Breadstuff([u8; 32]),
    /// No authorization (only valid if the cell's permissions allow it).
    None,
}

impl Authorization {
    /// Map this authorization to the corresponding AuthKind for permission checking.
    /// Returns None for Authorization::None and Authorization::Breadstuff (handled separately).
    pub fn to_auth_kind(&self) -> Option<pyana_cell::AuthKind> {
        match self {
            Authorization::Signature(_, _) => Some(pyana_cell::AuthKind::Signature),
            Authorization::Proof(_) => Some(pyana_cell::AuthKind::Proof),
            Authorization::Breadstuff(_) => None,
            Authorization::None => None,
        }
    }

    /// Create a Signature authorization from a 64-byte signature.
    pub fn from_sig_bytes(bytes: [u8; 64]) -> Self {
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[..32]);
        s.copy_from_slice(&bytes[32..]);
        Authorization::Signature(r, s)
    }
}

/// Whether/how children can use their parent's capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelegationMode {
    /// Children cannot use parent's capabilities.
    None,
    /// Children can use capabilities that the parent owns.
    ParentsOwn,
    /// Children inherit parent's delegation mode transitively.
    Inherit,
}

/// An effect produced by an action — what changes in the ledger.
///
/// Analogous to Mina's balance_change + state updates, but generalized for
/// the cell model.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Effect {
    /// Set a state field on a cell.
    SetField {
        cell: CellId,
        index: usize,
        value: FieldElement,
    },
    /// Transfer computrons between cells.
    Transfer {
        from: CellId,
        to: CellId,
        amount: u64,
    },
    /// Grant a capability from one cell to another.
    GrantCapability {
        from: CellId,
        to: CellId,
        cap: CapabilityRef,
    },
    /// Revoke a capability from a cell.
    RevokeCapability { cell: CellId, slot: u32 },
    /// Emit an event from a cell (does not modify state, but is part of the receipt).
    EmitEvent { cell: CellId, event: Event },
    /// Increment a cell's nonce by 1.
    IncrementNonce { cell: CellId },
    /// Create a new cell in the ledger.
    CreateCell {
        public_key: [u8; 32],
        token_id: [u8; 32],
        balance: u64,
    },
}

/// An event emitted by an action.
///
/// Events are logged in the receipt but do not modify ledger state.
/// They are indexed by topic for off-chain consumption.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    /// The topic of this event (hashed method/event name).
    pub topic: Symbol,
    /// Arbitrary data fields.
    pub data: Vec<FieldElement>,
}

impl Action {
    /// Compute the BLAKE3 hash of this action (for Merkle tree inclusion).
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.target.as_bytes());
        hasher.update(&self.method);
        for arg in &self.args {
            hasher.update(arg);
        }
        // Hash authorization discriminant + data.
        match &self.authorization {
            Authorization::Signature(r, s) => {
                hasher.update(&[0u8]);
                hasher.update(r);
                hasher.update(s);
            }
            Authorization::Proof(proof) => {
                hasher.update(&[1u8]);
                hasher.update(proof);
            }
            Authorization::Breadstuff(token) => {
                hasher.update(&[2u8]);
                hasher.update(token);
            }
            Authorization::None => {
                hasher.update(&[3u8]);
            }
        }
        // Hash delegation mode.
        hasher.update(&[self.may_delegate as u8]);
        // Hash effects.
        for effect in &self.effects {
            hasher.update(&effect.hash());
        }
        *hasher.finalize().as_bytes()
    }
}

impl Effect {
    /// Compute the BLAKE3 hash of this effect.
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        match self {
            Effect::SetField { cell, index, value } => {
                hasher.update(&[0u8]);
                hasher.update(cell.as_bytes());
                hasher.update(&(*index as u64).to_le_bytes());
                hasher.update(value);
            }
            Effect::Transfer { from, to, amount } => {
                hasher.update(&[1u8]);
                hasher.update(from.as_bytes());
                hasher.update(to.as_bytes());
                hasher.update(&amount.to_le_bytes());
            }
            Effect::GrantCapability { from, to, cap } => {
                hasher.update(&[2u8]);
                hasher.update(from.as_bytes());
                hasher.update(to.as_bytes());
                hasher.update(cap.target.as_bytes());
                hasher.update(&cap.slot.to_le_bytes());
            }
            Effect::RevokeCapability { cell, slot } => {
                hasher.update(&[3u8]);
                hasher.update(cell.as_bytes());
                hasher.update(&slot.to_le_bytes());
            }
            Effect::EmitEvent { cell, event } => {
                hasher.update(&[4u8]);
                hasher.update(cell.as_bytes());
                hasher.update(&event.topic);
                for d in &event.data {
                    hasher.update(d);
                }
            }
            Effect::IncrementNonce { cell } => {
                hasher.update(&[5u8]);
                hasher.update(cell.as_bytes());
            }
            Effect::CreateCell { public_key, token_id, balance } => {
                hasher.update(&[6u8]);
                hasher.update(public_key);
                hasher.update(token_id);
                hasher.update(&balance.to_le_bytes());
            }
        }
        *hasher.finalize().as_bytes()
    }

    /// Return the number of bytes of data in this effect (for cost estimation).
    pub fn data_bytes(&self) -> usize {
        match self {
            Effect::SetField { .. } => 32 + 8 + 32, // cell + index + value
            Effect::Transfer { .. } => 32 + 32 + 8,
            Effect::GrantCapability { .. } => 32 + 32 + 36,
            Effect::RevokeCapability { .. } => 32 + 4,
            Effect::EmitEvent { event, .. } => 32 + 32 + event.data.len() * 32,
            Effect::IncrementNonce { .. } => 32,
            Effect::CreateCell { .. } => 32 + 32 + 8,
        }
    }
}

impl Event {
    /// Create a new event.
    pub fn new(topic: Symbol, data: Vec<FieldElement>) -> Self {
        Self { topic, data }
    }
}
