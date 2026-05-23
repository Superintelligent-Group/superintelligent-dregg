//! Content creators: identities that publish content on a tier schedule.
//!
//! A creator owns one or more **tiers** (e.g. `"free"`, `"premium"`). Each
//! tier has a per-epoch price and an optional credential requirement.
//!
//! Creators don't actually mint cryptocurrency here — the in-memory `Ledger`
//! in `payments.rs` does that bookkeeping. Creators are pure-data: they
//! describe what's for sale.

use pyana_types::PublicKey;
use serde::{Deserialize, Serialize};

/// A subscription tier offered by a creator.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tier {
    /// Stable identifier (e.g. `"premium"`).
    pub id: String,
    /// Human-readable label shown in the UI.
    pub label: String,
    /// Price charged per epoch, in units of `asset_id`.
    pub price_per_epoch: u64,
    /// The asset (token) used to pay for this tier. Identifies the column of
    /// the `Ledger` that auto-debit operates on.
    pub asset_id: u64,
    /// Optional credential requirement.
    ///
    /// - `None` → anyone may subscribe (free tier semantics).
    /// - `Some(pk)` → subscriber must present a `DelegatedToken` signed by
    ///   `pk` (the *premium issuer*) before they can subscribe. See
    ///   `subscriber.rs::Subscriber::verify_tier_credential`.
    pub credential_issuer: Option<PublicKey>,
}

impl Tier {
    /// Build a free tier (no credential, no price).
    pub fn free(id: impl Into<String>, label: impl Into<String>, asset_id: u64) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            price_per_epoch: 0,
            asset_id,
            credential_issuer: None,
        }
    }

    /// Build a premium tier.
    pub fn premium(
        id: impl Into<String>,
        label: impl Into<String>,
        asset_id: u64,
        price_per_epoch: u64,
        issuer: PublicKey,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            price_per_epoch,
            asset_id,
            credential_issuer: Some(issuer),
        }
    }

    /// Whether this tier requires a credential.
    pub fn is_gated(&self) -> bool {
        self.credential_issuer.is_some()
    }
}

/// A content creator: identity + tier catalog + published content log.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Creator {
    /// Creator identity (Ed25519 public key, used as a sender id in inboxes).
    pub identity: PublicKey,
    /// Tiers this creator offers, keyed by `Tier::id`.
    pub tiers: Vec<Tier>,
    /// Append-only log of published items. Each item is content-addressed by
    /// `blake3(body)`. We retain this so creators can re-publish to late-
    /// arriving subscribers without re-uploading.
    pub published: Vec<ContentItem>,
}

/// One published piece of content, prior to encryption.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentItem {
    /// `blake3` of `body`.
    pub content_hash: [u8; 32],
    /// Tier this content belongs to (only subscribers of this tier receive it).
    pub tier_id: String,
    /// Epoch at which this content was published.
    pub epoch: u64,
    /// Raw bytes. The server holds these so it can encrypt-per-subscriber on
    /// each push. In a production system, the raw bytes would live in
    /// content-addressed storage and we'd only store the hash here.
    pub body: Vec<u8>,
}

impl Creator {
    /// Create a new creator with the given identity and an empty tier list.
    pub fn new(identity: PublicKey) -> Self {
        Self {
            identity,
            tiers: Vec::new(),
            published: Vec::new(),
        }
    }

    /// Add a tier. Overwrites if `tier.id` already exists.
    pub fn add_tier(&mut self, tier: Tier) {
        self.tiers.retain(|t| t.id != tier.id);
        self.tiers.push(tier);
    }

    /// Look up a tier by id.
    pub fn tier(&self, id: &str) -> Option<&Tier> {
        self.tiers.iter().find(|t| t.id == id)
    }

    /// Publish a content item. Appends to `self.published`.
    ///
    /// Returns the `content_hash`.
    pub fn publish(&mut self, tier_id: impl Into<String>, body: Vec<u8>, epoch: u64) -> [u8; 32] {
        let tier_id = tier_id.into();
        let content_hash = *blake3::hash(&body).as_bytes();
        self.published.push(ContentItem {
            content_hash,
            tier_id,
            epoch,
            body,
        });
        content_hash
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(seed: u8) -> PublicKey {
        let mut k = [0u8; 32];
        k[0] = seed;
        PublicKey(k)
    }

    #[test]
    fn free_tier_has_no_credential() {
        let t = Tier::free("free", "Free updates", 1);
        assert!(!t.is_gated());
        assert_eq!(t.price_per_epoch, 0);
    }

    #[test]
    fn premium_tier_is_gated() {
        let t = Tier::premium("vip", "Premium", 1, 100, pk(7));
        assert!(t.is_gated());
        assert_eq!(t.credential_issuer.unwrap(), pk(7));
    }

    #[test]
    fn publish_appends_item() {
        let mut creator = Creator::new(pk(1));
        creator.add_tier(Tier::free("free", "F", 1));
        let h = creator.publish("free", b"hello".to_vec(), 3);
        assert_eq!(creator.published.len(), 1);
        assert_eq!(creator.published[0].content_hash, h);
        assert_eq!(creator.published[0].epoch, 3);
    }

    #[test]
    fn add_tier_replaces_same_id() {
        let mut c = Creator::new(pk(1));
        c.add_tier(Tier::free("t", "v1", 1));
        c.add_tier(Tier::premium("t", "v2", 1, 100, pk(2)));
        assert_eq!(c.tiers.len(), 1);
        assert!(c.tier("t").unwrap().is_gated());
    }
}
