//! Ring trade participation for the orderbook.
//!
//! Implements [`RingTradeParticipant`] on the orderbook state so that a solver
//! coordinator can atomically fill cross-pair orders. Each resting limit order
//! is exposed as an [`ExchangeSpec`]: "I offer `quote_asset` tokens at `price`
//! for `base_asset` tokens" (for buys) or "I offer `base_asset` at `price` for
//! `quote_asset`" (for sells).
//!
//! The [`OrderbookRingParticipant`] wrapper holds a mutable reference to the
//! engine and implements the three trait methods:
//!
//! - `exchange_offers` — enumerate every resting limit order as an `ExchangeSpec`.
//! - `settle_leg` — fill the matching order with `settlement.amount` and mark it
//!   as partially or fully filled.
//! - `rollback_leg` — restore the order's `remaining_amount` and status if a peer
//!   leg fails.
//!
//! # Cross-pair atomic settlement
//!
//! A user who has:
//! - Order A on ETH/USDC: "sell 1 ETH for 3000 USDC"
//! - Order B on BTC/ETH:  "buy 0.05 BTC for 1 ETH"
//!
//! can be matched atomically by a solver that finds the ring:
//! A gives ETH → solver gives BTC → B gives USDC.
//! Both orders fill simultaneously, with no intermediate hop slippage.

use pyana_app_framework::ring_trade::{ExchangeSpec, RingTradeParticipant, Settlement};
use pyana_intent::exchange::AssetId;

use crate::book::OrderBook;
use crate::order::{Order, OrderId, OrderStatus, OrderType, Side};

// =============================================================================
// Asset encoding helpers
// =============================================================================

/// Encode a trading pair side into a 32-byte asset ID.
///
/// Asset IDs are derived from the pair string and side: e.g., "ETH/USDC buy"
/// gives a stable 32-byte hash usable in `ExchangeSpec`.
fn encode_asset(pair_side: &str) -> AssetId {
    let mut hasher = blake3::Hasher::new_derive_key("pyana-orderbook-asset-v1");
    hasher.update(pair_side.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Asset ID for the base asset of a pair (e.g., "ETH" in "ETH/USDC").
pub fn base_asset_id(base: &str) -> AssetId {
    encode_asset(base)
}

/// Asset ID for the quote asset of a pair (e.g., "USDC" in "ETH/USDC").
pub fn quote_asset_id(quote: &str) -> AssetId {
    encode_asset(quote)
}

// =============================================================================
// RingLeg: snapshot of order state before settlement (for rollback)
// =============================================================================

/// Snapshot of an order's mutable fields before a ring leg is settled.
/// Used to roll back the order if a downstream leg fails.
///
/// If `full_order` is `Some`, the order was fully consumed by the settle and
/// removed from the book; rollback must re-insert this stored copy. Otherwise
/// the (partially-filled) order is still on the book and we restore its fields.
#[derive(Clone, Debug)]
pub struct RingLegSnapshot {
    pub order_id: OrderId,
    pub remaining_before: u64,
    pub status_before: OrderStatus,
    /// Full order copy, only populated when the order was fully consumed.
    pub full_order: Option<Order>,
}

// =============================================================================
// OrderbookRingParticipant
// =============================================================================

/// Wraps an [`OrderBook`] and implements [`RingTradeParticipant`].
///
/// The participant enumerates all resting limit orders as exchange offers and
/// applies/rolls back fills as directed by the ring solver coordinator.
pub struct OrderbookRingParticipant<'a> {
    /// The trading pair base ticker (e.g., "ETH").
    pub base: String,
    /// The trading pair quote ticker (e.g., "USDC").
    pub quote: String,
    /// The live order book.
    pub book: &'a mut OrderBook,
    /// Snapshots for rollback, stored as a LIFO stack so the most-recent
    /// settle is the first to be undone.
    snapshots: Vec<RingLegSnapshot>,
}

impl<'a> OrderbookRingParticipant<'a> {
    /// Create a new participant for the given trading pair and order book.
    pub fn new(base: impl Into<String>, quote: impl Into<String>, book: &'a mut OrderBook) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            book,
            snapshots: Vec::new(),
        }
    }

    /// Encode an order as an `ExchangeSpec`.
    ///
    /// A buy order at price P for amount Q says:
    ///   "I offer Q * P quote tokens, I want Q base tokens."
    /// A sell order at price P for amount Q says:
    ///   "I offer Q base tokens, I want Q * P quote tokens."
    fn order_to_exchange_spec(&self, order: &Order) -> Option<ExchangeSpec> {
        match &order.order_type {
            OrderType::Limit { price, side, .. } => {
                let q = order.remaining_amount;
                if q == 0 {
                    return None;
                }
                let base = base_asset_id(&self.base);
                let quote = quote_asset_id(&self.quote);
                match side {
                    Side::Buy => Some(ExchangeSpec {
                        // Buyer offers quote tokens (price * amount).
                        offer_asset: quote,
                        offer_amount: price.saturating_mul(q),
                        // Buyer wants base tokens.
                        want_asset: base,
                        want_min_amount: q,
                        min_rate: None,
                        max_rate: None,
                    }),
                    Side::Sell => Some(ExchangeSpec {
                        // Seller offers base tokens.
                        offer_asset: base,
                        offer_amount: q,
                        // Seller wants quote tokens (price * amount).
                        want_asset: quote,
                        want_min_amount: price.saturating_mul(q),
                        min_rate: None,
                        max_rate: None,
                    }),
                }
            }
            _ => None, // Market and stop-loss orders are not exposed to the ring solver.
        }
    }

    /// Find a resting limit order whose asset pair matches the settlement.
    ///
    /// We match by checking if the settlement's `asset` corresponds to the order's
    /// offer asset (base for sells, quote for buys) and the amount fits.
    ///
    // REVIEW[P1]: `Settlement` carries only (asset, amount) — there is no price
    // or rate, and no link back to the order the solver expected to fill. As a
    // result this picks the FIRST order with sufficient remaining amount, which
    // (a) silently violates price-time priority on a per-leg basis, and (b)
    // gives the solver no way to refer to a specific resting order. Long-term
    // fix: extend `Settlement` (in app-framework, out of scope here) with a
    // counterparty/order identifier or include a `min_rate` enforced here.
    fn find_matching_order_id(&self, settlement: &Settlement) -> Option<OrderId> {
        let base = base_asset_id(&self.base);
        let quote = quote_asset_id(&self.quote);

        // Walk all resting orders in the book.
        for level in self.book.ask_levels() {
            for order in &level.orders {
                if !order.is_active() {
                    continue;
                }
                // Ask (sell) orders offer base tokens.
                if settlement.asset == base && order.remaining_amount >= settlement.amount {
                    return Some(order.id);
                }
            }
        }
        for level in self.book.bid_levels() {
            for order in &level.orders {
                if !order.is_active() {
                    continue;
                }
                // Bid (buy) orders offer quote tokens (price * remaining).
                if let OrderType::Limit { price, .. } = &order.order_type {
                    let quote_offered = price.saturating_mul(order.remaining_amount);
                    if settlement.asset == quote && quote_offered >= settlement.amount {
                        return Some(order.id);
                    }
                }
            }
        }
        None
    }
}

// =============================================================================
// RingTradeError
// =============================================================================

/// Errors from ring trade operations on the orderbook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RingTradeError {
    /// No matching order found in the book for the settlement spec.
    NoMatchingOrder,
    /// The matched order has insufficient remaining amount.
    InsufficientAmount { available: u64, requested: u64 },
    /// The order to roll back was not found (idempotent: treated as success).
    OrderNotFound,
}

impl std::fmt::Display for RingTradeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMatchingOrder => write!(f, "no matching order for settlement"),
            Self::InsufficientAmount {
                available,
                requested,
            } => write!(
                f,
                "insufficient amount: available={}, requested={}",
                available, requested
            ),
            Self::OrderNotFound => write!(f, "order not found (already removed or never existed)"),
        }
    }
}

// =============================================================================
// RingTradeParticipant impl
// =============================================================================

impl<'a> RingTradeParticipant for OrderbookRingParticipant<'a> {
    type Error = RingTradeError;

    /// Return all resting limit orders as exchange specs for the solver.
    fn exchange_offers(&self) -> Vec<ExchangeSpec> {
        let mut offers = Vec::new();
        for level in self.book.ask_levels() {
            for order in &level.orders {
                if order.is_active() {
                    if let Some(spec) = self.order_to_exchange_spec(order) {
                        offers.push(spec);
                    }
                }
            }
        }
        for level in self.book.bid_levels() {
            for order in &level.orders {
                if order.is_active() {
                    if let Some(spec) = self.order_to_exchange_spec(order) {
                        offers.push(spec);
                    }
                }
            }
        }
        offers
    }

    /// Fill the indicated order with `settlement.amount`.
    ///
    /// Stores a snapshot for rollback before modifying state.
    fn settle_leg(&mut self, settlement: &Settlement) -> Result<(), RingTradeError> {
        let order_id = self
            .find_matching_order_id(settlement)
            .ok_or(RingTradeError::NoMatchingOrder)?;

        // We need to mutate the order in-place. To do this we re-borrow the book's
        // internal structures.  The safest approach is to remove, modify, and
        // re-insert if residual remains.
        let mut order = self
            .book
            .remove_order(&order_id)
            .ok_or(RingTradeError::OrderNotFound)?;

        if order.remaining_amount < settlement.amount {
            let available = order.remaining_amount;
            // Put the order back before returning an error.
            self.book.insert_order(order);
            return Err(RingTradeError::InsufficientAmount {
                available,
                requested: settlement.amount,
            });
        }

        // Snapshot state before modification. Capture the full order if we're
        // about to fully consume it so rollback can re-insert the exact order
        // (including price level, creation time, TIF, etc.) rather than dropping
        // it on the floor.
        let remaining_before = order.remaining_amount;
        let status_before = order.status.clone();
        let will_be_fully_filled = settlement.amount >= order.remaining_amount;
        let full_order = if will_be_fully_filled {
            Some(order.clone())
        } else {
            None
        };

        // Apply the fill.
        order.remaining_amount -= settlement.amount;
        if order.remaining_amount == 0 {
            order.status = OrderStatus::Filled;
            // Fully filled: don't re-insert; the order is consumed.
        } else {
            let filled_amount = match &order.order_type {
                OrderType::Limit { amount, .. } => amount.saturating_sub(order.remaining_amount),
                _ => settlement.amount,
            };
            order.status = OrderStatus::PartiallyFilled { filled_amount };
            // Re-insert the partially filled order. NOTE[P2]: this puts the
            // residual at the BACK of its price level, silently demoting its
            // time priority. A faithful restore would need a price-level
            // mutate-in-place API on OrderBook (out of scope here — flagged).
            self.book.insert_order(order);
        }

        self.snapshots.push(RingLegSnapshot {
            order_id,
            remaining_before,
            status_before,
            full_order,
        });

        Ok(())
    }

    /// Roll back a previously settled leg.
    ///
    /// Restores the order's `remaining_amount` and `status` to their pre-settle
    /// values. Idempotent: if there is nothing to undo it is silently ignored.
    ///
    /// Rollbacks are processed in LIFO order (the last settled leg is undone first).
    fn rollback_leg(&mut self, _settlement: &Settlement) -> Result<(), RingTradeError> {
        // Pop the most-recently settled snapshot (LIFO undo).
        let Some(snap) = self.snapshots.pop() else {
            // Nothing to undo: rollback is idempotent.
            return Ok(());
        };

        // Remove the (possibly partial) order currently on the book, if any.
        let maybe_order = self.book.remove_order(&snap.order_id);

        if let Some(mut order) = maybe_order {
            // Partial-fill case: order is still on the book, restore fields.
            order.remaining_amount = snap.remaining_before;
            order.status = snap.status_before;
            self.book.insert_order(order);
        } else if let Some(full) = snap.full_order {
            // Full-fill case: the original was consumed during settle_leg.
            // Re-insert the saved copy so the book is exactly restored.
            self.book.insert_order(full);
        }
        // Rollback is idempotent: no error if nothing to undo.
        Ok(())
    }
}
