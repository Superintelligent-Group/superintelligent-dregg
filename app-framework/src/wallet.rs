//! App-framework wallet handle.
//!
//! Apps are *userspace*: they should not reach past the SDK into
//! `pyana_turn::builder::ActionBuilder` or hand-encode `[0u8; 64]`
//! placeholder signatures. Instead, the framework hands them a narrow,
//! wallet-bound action-construction surface backed by the SDK's
//! [`pyana_sdk::AgentWallet`].
//!
//! ## What this gives apps
//!
//! - [`AppWallet::cell_id`] — the agent's canonical CellId in its default
//!   federation domain (no string-threading every call).
//! - [`AppWallet::public_key`] — the wallet's identity (32-byte public key).
//! - [`AppWallet::make_action`] — build a single-method action with
//!   multiple effects, signed for the framework's federation_id binding.
//! - [`AppWallet::make_turn`] — wrap a signed action in a Turn with
//!   sane defaults (nonce/forest hash filled by the executor path).
//! - [`AppWallet::sign_action`] — re-sign a pre-built action.
//!
//! ## What apps cannot do through this handle
//!
//! - Extract the underlying signing key (only the framework holds the SDK
//!   wallet; apps see [`AppWallet`] which deliberately exposes no
//!   key-export methods).
//! - Mutate the wallet's receipt chain or token list.
//! - Reach into `AgentWallet`'s 107-method surface — that's an SDK
//!   concern, not an app concern.
//!
//! ## Why a wrapper and not `&AgentWallet`?
//!
//! Exposing `AgentWallet` directly to apps couples the userspace surface
//! to every method we add to the SDK. The framework wallet handle is the
//! intentional narrow waist — when an app needs a new primitive, it's
//! either a *new framework method* (small, reviewed) or a *missing SDK
//! method* (we add it once, the framework method delegates).
//!
//! ## Federation binding
//!
//! Action signatures carry a 32-byte `federation_id` to prevent
//! cross-federation replay (see `pyana_turn::executor::TurnExecutor::compute_signing_message`).
//! The framework holds *one* federation_id per process — set at
//! [`AppWallet::new`] — and threads it into every `make_action` /
//! `sign_action` call. Apps never see it.

use std::sync::Arc;

use pyana_sdk::AgentWallet;
use pyana_turn::Turn;
use pyana_turn::action::{Action, Effect};
use pyana_types::{CellId, PublicKey};

/// A wallet handle suitable for app-level userspace.
///
/// Wraps an [`AgentWallet`] and a `federation_id`, exposing only the
/// methods apps need to build signed actions and turns. Cheap to clone
/// (internally `Arc<AgentWallet>`).
#[derive(Clone)]
pub struct AppWallet {
    inner: Arc<AgentWallet>,
    federation_id: [u8; 32],
    domain: String,
}

impl AppWallet {
    /// Construct an app wallet from an SDK wallet and the federation
    /// identifier this app operates in.
    ///
    /// The default domain is `"default"` — matches `AgentWallet::cell_id("default")`.
    /// Override with [`Self::with_domain`].
    pub fn new(wallet: AgentWallet, federation_id: [u8; 32]) -> Self {
        Self {
            inner: Arc::new(wallet),
            federation_id,
            domain: "default".to_string(),
        }
    }

    /// Set the default domain used by [`Self::cell_id`] and
    /// [`Self::make_turn`].
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = domain.into();
        self
    }

    /// This wallet's public key (the agent identity).
    pub fn public_key(&self) -> PublicKey {
        self.inner.public_key()
    }

    /// This wallet's CellId in the framework's default domain.
    pub fn cell_id(&self) -> CellId {
        self.inner.cell_id(&self.domain)
    }

    /// This wallet's CellId in an explicit domain (rarely needed; prefer
    /// [`Self::cell_id`]).
    pub fn cell_id_for(&self, domain: &str) -> CellId {
        self.inner.cell_id(domain)
    }

    /// The federation_id this wallet signs against.
    pub fn federation_id(&self) -> &[u8; 32] {
        &self.federation_id
    }

    /// Build a self-signed [`Action`] targeting one cell with a list of
    /// effects.
    ///
    /// The action carries a real `Authorization::Signature(..)` — no
    /// `[0u8; 64]` placeholders. The signature binds to this wallet's
    /// public key, the action's canonical bytes, and the framework's
    /// federation_id.
    pub fn make_action(&self, target: CellId, method: &str, effects: Vec<Effect>) -> Action {
        self.inner
            .make_action(target, method, effects, &self.federation_id)
    }

    /// Re-sign an already-built [`Action`] with this wallet, overwriting
    /// its existing authorization.
    ///
    /// Use this when an action needs to be assembled by lower-level
    /// builders (e.g. multi-step `ActionBuilder` typestate flows that
    /// the framework cannot anticipate) but should still carry a real
    /// framework-issued signature.
    pub fn sign_action(&self, action: Action) -> Action {
        self.inner.sign_action(action, &self.federation_id)
    }

    /// Wrap a signed [`Action`] in a [`Turn`] ready for submission.
    ///
    /// The Turn's `agent` is `self.cell_id()`, `previous_receipt_hash` is
    /// pulled from the wallet's chain head, `nonce` defaults to 0 (the
    /// caller's submission path is expected to set the real nonce; see
    /// `pyana_sdk::AgentRuntime::execute`), and forest/tree hashes are
    /// zeroed and filled in by `compute_turn_bytes` at signing time.
    pub fn make_turn(&self, action: Action) -> Turn {
        self.inner.make_turn_for(&self.domain, action)
    }

    /// Get a reference to the underlying SDK wallet.
    ///
    /// Escape hatch for framework-internal code that legitimately needs
    /// the full SDK surface (e.g. signing a federation registration
    /// envelope, generating a STARK presentation proof). App code should
    /// not call this — if you find yourself reaching here from an `apps/*`
    /// crate, the framework is missing a narrow method.
    pub fn sdk_wallet(&self) -> &AgentWallet {
        &self.inner
    }
}

impl std::fmt::Debug for AppWallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppWallet")
            .field("public_key", &hex_short(&self.inner.public_key().0))
            .field("domain", &self.domain)
            .field("federation_id", &hex_short(&self.federation_id))
            .finish()
    }
}

fn hex_short(bytes: &[u8]) -> String {
    let n = bytes.len().min(8);
    let mut s = String::with_capacity(2 * n + 1);
    for b in &bytes[..n] {
        s.push_str(&format!("{b:02x}"));
    }
    if bytes.len() > n {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_signs_action_with_real_signature() {
        let sdk = AgentWallet::new();
        let fed = [7u8; 32];
        let wallet = AppWallet::new(sdk, fed);
        let target = CellId::from_bytes([1u8; 32]);

        let action = wallet.make_action(target, "noop", vec![]);

        // The whole point: not Unchecked, and not a zero signature.
        match action.authorization {
            pyana_turn::action::Authorization::Signature(a, b) => {
                assert!(
                    a != [0u8; 32] || b != [0u8; 32],
                    "signature must be non-zero"
                );
            }
            other => panic!("expected Signature variant, got {other:?}"),
        }
    }

    #[test]
    fn wallet_make_turn_binds_to_default_domain() {
        let sdk = AgentWallet::new();
        let wallet = AppWallet::new(sdk, [0u8; 32]);
        let cell = wallet.cell_id();
        let action = wallet.make_action(cell, "noop", vec![]);
        let turn = wallet.make_turn(action);
        assert_eq!(turn.agent, cell);
        assert_eq!(turn.nonce, 0);
    }

    #[test]
    fn with_domain_changes_cell_id() {
        let sdk = AgentWallet::new();
        let w1 = AppWallet::new(sdk, [0u8; 32]);
        let w2 = w1.clone().with_domain("alt-domain");
        assert_ne!(w1.cell_id(), w2.cell_id());
    }

    // NOTE: the "sign_action overwrites Unchecked" test lives in
    // `app-framework/tests/wallet_sign_action.rs` (an integration test
    // directory) — the in-`src/` grep guard
    // (`tests/no_unchecked.rs`) refuses to allow the literal
    // `Authorization::Unchecked` anywhere under `src/`, including in
    // `#[cfg(test)]` blocks. That is by design.
}
