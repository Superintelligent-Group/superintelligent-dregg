//! Cordial Dissemination Protocol for the Blocklace.
//!
//! Implements the communication protocol from the Cordial Miners paper.
//! The key principle: "send to others blocks you know and think they need."
//!
//! Block pointers encode what each node knows, enabling efficient catch-up
//! without explicit protocol messages. Each node maintains an estimate of
//! what each peer has seen, based on blocks received FROM that peer.
//!
//! # Protocol Messages
//!
//! - **Push**: Proactively send blocks we think a peer needs (delta groups).
//! - **Pull**: Request blocks we know we're missing (predecessor gaps).
//! - **PullResponse**: Reply with a causally-closed set of requested blocks.
//! - **HaveFrontier**: Lightweight sync: exchange frontier tip IDs.
//!
//! # Causal Closure
//!
//! All transmitted sets of blocks must be causally closed: for every block B
//! in the set, all predecessors of B are either already known to the receiver
//! or included earlier in the same set.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{Block, BlockId, Blocklace, NodeKey};

/// Maximum number of blocks to include in a single push message.
/// Chunks are sent sequentially to avoid OOM on large syncs.
pub const MAX_BLOCKS_PER_PUSH: usize = 100;

// =============================================================================
// Peer Knowledge Tracking
// =============================================================================

/// Tracks what we believe each peer has seen.
///
/// When we receive a block from a peer, we know they have that block AND
/// all of its transitive predecessors (causal closure). We use this to
/// avoid sending redundant blocks.
#[derive(Clone, Debug, Default)]
pub struct PeerKnowledge {
    /// The latest block we've received from each peer (by their node key).
    latest_from: HashMap<NodeKey, BlockId>,
    /// Our estimate of what blocks each peer has (their causal past).
    /// This is a conservative over-approximation: we may think they have
    /// blocks they don't, but we never think they lack blocks they have.
    /// (In practice it's exact for received-from knowledge, and conservative
    /// for inferred knowledge.)
    estimated_known: HashMap<NodeKey, HashSet<BlockId>>,
}

impl PeerKnowledge {
    /// Create empty peer knowledge.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get our estimate of what a peer knows.
    pub fn known_by(&self, peer: &NodeKey) -> Option<&HashSet<BlockId>> {
        self.estimated_known.get(peer)
    }

    /// Get the latest block received from a peer.
    pub fn latest_from(&self, peer: &NodeKey) -> Option<&BlockId> {
        self.latest_from.get(peer)
    }

    /// Record that we received a block from a peer.
    ///
    /// This implies the peer has this block and all its causal predecessors.
    /// We update our knowledge estimate using the blocklace to compute the
    /// full causal past.
    pub fn record_received(&mut self, peer: &NodeKey, block: &Block, blocklace: &Blocklace) {
        let block_id = block.id();
        self.latest_from.insert(*peer, block_id);

        let known = self.estimated_known.entry(*peer).or_default();
        known.insert(block_id);

        // The peer must have all predecessors (causal closure).
        // Compute the full transitive closure from the blocklace.
        let past = blocklace.causal_past(&block_id);
        known.extend(past);
    }

    /// Record that we believe a peer has a specific set of blocks.
    ///
    /// Used when processing frontier announcements or other out-of-band
    /// knowledge (e.g., after a successful push).
    pub fn record_has(&mut self, peer: &NodeKey, block_ids: &HashSet<BlockId>) {
        let known = self.estimated_known.entry(*peer).or_default();
        known.extend(block_ids.iter());
    }

    /// Record that we sent blocks to a peer (and assume they received them).
    ///
    /// After a successful push, we update our estimate so we don't re-send.
    pub fn record_sent(&mut self, peer: &NodeKey, block_ids: &[BlockId]) {
        let known = self.estimated_known.entry(*peer).or_default();
        for id in block_ids {
            known.insert(*id);
        }
    }

    /// Update knowledge from a frontier announcement.
    ///
    /// If a peer announces frontier tips, we know they have the causal past
    /// of each tip block.
    pub fn update_from_frontier(
        &mut self,
        peer: &NodeKey,
        frontier_tips: &[BlockId],
        blocklace: &Blocklace,
    ) {
        let known = self.estimated_known.entry(*peer).or_default();
        for tip in frontier_tips {
            let past = blocklace.causal_past(tip);
            known.extend(past);
        }
    }
}

// =============================================================================
// Delta Group (Causally-Closed Batch)
// =============================================================================

/// A delta group: a causally-closed subset of blocks for transmission.
///
/// Blocks are ordered such that predecessors appear before dependents.
/// The receiver can insert them sequentially without encountering
/// missing-predecessor errors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeltaGroup {
    /// Blocks in causal order (predecessors before dependents).
    pub blocks: Vec<Block>,
}

impl DeltaGroup {
    /// Create a new empty delta group.
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    /// Create a delta group from a vec of blocks (assumed to be in causal order).
    pub fn from_blocks(blocks: Vec<Block>) -> Self {
        Self { blocks }
    }

    /// Verify this delta group is valid (causally closed) given a set of
    /// blocks the receiver already has.
    ///
    /// A delta group is valid if for every block in it, all of its
    /// predecessors are either:
    /// 1. In `existing` (already known to receiver), OR
    /// 2. Earlier in this delta group.
    pub fn is_valid(&self, existing: &HashSet<BlockId>) -> bool {
        let mut known = existing.clone();
        for block in &self.blocks {
            if !block.predecessors.iter().all(|p| known.contains(p)) {
                return false;
            }
            known.insert(block.id());
        }
        true
    }

    /// Number of blocks in this delta group.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Whether this delta group is empty.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Get the set of block IDs in this delta group.
    pub fn block_ids(&self) -> HashSet<BlockId> {
        self.blocks.iter().map(|b| b.id()).collect()
    }
}

impl Default for DeltaGroup {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Request / Response
// =============================================================================

/// A request for specific missing blocks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockRequest {
    /// The block IDs we need.
    pub missing: Vec<BlockId>,
    /// The requester's identity.
    pub from: NodeKey,
}

/// A response containing requested blocks as a causally-closed delta group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockResponse {
    /// A causally-closed set containing all requested blocks plus any
    /// predecessors the requester might need.
    pub delta: DeltaGroup,
}

// =============================================================================
// Frontier
// =============================================================================

/// Lightweight frontier: the tip block IDs per creator.
///
/// Used for efficient sync negotiation: nodes exchange frontiers to determine
/// what delta to send without transmitting full block data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frontier {
    /// Creator -> latest block ID from that creator.
    pub tips: HashMap<NodeKey, BlockId>,
}

impl Frontier {
    /// Create a frontier from the current blocklace state.
    pub fn from_blocklace(blocklace: &Blocklace) -> Self {
        Self {
            tips: blocklace.tips().clone(),
        }
    }

    /// Create an empty frontier.
    pub fn empty() -> Self {
        Self {
            tips: HashMap::new(),
        }
    }
}

// =============================================================================
// Protocol Messages
// =============================================================================

/// Wire-level dissemination protocol messages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisseminationMessage {
    /// "Here are blocks I think you need."
    Push(DeltaGroup),
    /// "I need these blocks I don't have."
    Pull(BlockRequest),
    /// "Here are the blocks you asked for."
    PullResponse(BlockResponse),
    /// "I have blocks up to this frontier." (lightweight sync)
    HaveFrontier(Frontier),
}

// =============================================================================
// Disseminator Engine
// =============================================================================

/// The cordial dissemination engine.
///
/// Manages a local blocklace and peer knowledge estimates to efficiently
/// propagate blocks between nodes.
pub struct Disseminator {
    /// Our local blocklace state.
    blocklace: Blocklace,
    /// What we think each peer knows.
    peer_knowledge: PeerKnowledge,
    /// Our identity (public key).
    self_key: NodeKey,
    /// Blocks we've received but can't insert yet (missing predecessors).
    /// Maps block_id -> (block, missing_predecessor_ids).
    pending: HashMap<BlockId, (Block, HashSet<BlockId>)>,
}

impl Disseminator {
    /// Create a new disseminator for the given node identity.
    pub fn new(self_key: NodeKey) -> Self {
        Self {
            blocklace: Blocklace::new(),
            peer_knowledge: PeerKnowledge::new(),
            self_key,
            pending: HashMap::new(),
        }
    }

    /// Create a disseminator with an existing blocklace.
    pub fn with_blocklace(self_key: NodeKey, blocklace: Blocklace) -> Self {
        Self {
            blocklace,
            peer_knowledge: PeerKnowledge::new(),
            self_key,
            pending: HashMap::new(),
        }
    }

    /// Get a reference to the local blocklace.
    pub fn blocklace(&self) -> &Blocklace {
        &self.blocklace
    }

    /// Get a mutable reference to the local blocklace.
    pub fn blocklace_mut(&mut self) -> &mut Blocklace {
        &mut self.blocklace
    }

    /// Get a reference to the peer knowledge tracker.
    pub fn peer_knowledge(&self) -> &PeerKnowledge {
        &self.peer_knowledge
    }

    /// Our node key.
    pub fn self_key(&self) -> &NodeKey {
        &self.self_key
    }

    /// Create a new block and insert it into our local blocklace.
    ///
    /// The block's predecessors are the current frontier of the blocklace
    /// (all current tip blocks). Returns the block for broadcasting.
    pub fn create_block(&mut self, payload: Vec<u8>) -> Block {
        let sequence = self
            .blocklace
            .tip_for(&self.self_key)
            .and_then(|tip| self.blocklace.get(tip))
            .map(|b| b.sequence + 1)
            .unwrap_or(0);

        let predecessors: Vec<BlockId> = self.blocklace.frontier().iter().copied().collect();

        let block = Block::new(self.self_key, sequence, predecessors, payload);
        let _ = self.blocklace.insert(block.clone());
        block
    }

    /// Determine what blocks to send to a specific peer.
    ///
    /// Returns a causally-closed delta group containing blocks that:
    /// 1. We have locally
    /// 2. We think the peer does NOT have
    /// 3. Are causally closed (all predecessors are either known to the peer
    ///    or included earlier in the delta)
    pub fn blocks_to_send(&self, peer: &NodeKey) -> DeltaGroup {
        let peer_known = self
            .peer_knowledge
            .known_by(peer)
            .cloned()
            .unwrap_or_default();

        let local_ids = self.blocklace.block_ids();

        // Blocks the peer doesn't have.
        let unknown_to_peer: HashSet<BlockId> =
            local_ids.difference(&peer_known).copied().collect();

        if unknown_to_peer.is_empty() {
            return DeltaGroup::new();
        }

        // Get them in topological order.
        let ordered = self.blocklace.topological_subset(&unknown_to_peer);

        // Filter to only causally-closed subset.
        // A block is sendable if all its predecessors are either known to
        // the peer or already in the send set.
        let mut sendable = Vec::new();
        let mut peer_will_know = peer_known.clone();

        for block_id in &ordered {
            if let Some(block) = self.blocklace.get(block_id) {
                if block
                    .predecessors
                    .iter()
                    .all(|p| peer_will_know.contains(p))
                {
                    sendable.push(block.clone());
                    peer_will_know.insert(*block_id);
                }
            }
        }

        DeltaGroup::from_blocks(sendable)
    }

    /// Determine what blocks to send to a specific peer, split into chunks.
    ///
    /// Each chunk is a causally-closed delta group of at most `max_per_chunk`
    /// blocks. Chunks are ordered so that predecessors appear in earlier chunks.
    /// The receiver can process them sequentially without gaps.
    pub fn blocks_to_send_chunked(&self, peer: &NodeKey, max_per_chunk: usize) -> Vec<DeltaGroup> {
        let full_delta = self.blocks_to_send(peer);
        if full_delta.is_empty() {
            return vec![];
        }
        chunk_delta_group(full_delta, max_per_chunk)
    }

    /// Compute blocks created since a set of known tips.
    ///
    /// Returns all blocks in our blocklace whose IDs are NOT in the causal
    /// past of `known_tips`, in topological order. This is used for
    /// incremental updates after the initial frontier exchange.
    pub fn blocks_since(&self, known_tips: &HashMap<NodeKey, BlockId>) -> Vec<Block> {
        let mut their_known: HashSet<BlockId> = HashSet::new();
        for tip_id in known_tips.values() {
            if self.blocklace.contains(tip_id) {
                let past = self.blocklace.causal_past(tip_id);
                their_known.extend(past);
                their_known.insert(*tip_id);
            }
        }

        let local_ids = self.blocklace.block_ids();
        let unknown_to_them: HashSet<BlockId> =
            local_ids.difference(&their_known).copied().collect();

        if unknown_to_them.is_empty() {
            return vec![];
        }

        let ordered = self.blocklace.topological_subset(&unknown_to_them);

        // Filter to causally-closed subset (predecessors first).
        let mut result = Vec::new();
        let mut they_will_know = their_known;

        for block_id in &ordered {
            if let Some(block) = self.blocklace.get(block_id) {
                if block
                    .predecessors
                    .iter()
                    .all(|p| they_will_know.contains(p))
                {
                    result.push(block.clone());
                    they_will_know.insert(*block_id);
                }
            }
        }

        result
    }

    /// Process a block received from a peer.
    ///
    /// Updates peer knowledge and inserts the block into the local blocklace.
    /// Returns `Ok(block_id)` on success, or `Err(missing)` if predecessors
    /// are missing (in which case the block is buffered in `pending`).
    pub fn received_from(&mut self, peer: &NodeKey, block: Block) -> Result<BlockId, Vec<BlockId>> {
        let block_id = block.id();

        // Update peer knowledge: they have this block and all its predecessors.
        self.peer_knowledge
            .record_received(peer, &block, &self.blocklace);

        // Try to insert into blocklace.
        match self.blocklace.insert(block.clone()) {
            Ok(id) => {
                // Check if any pending blocks can now be inserted.
                self.try_flush_pending();
                Ok(id)
            }
            Err(missing) => {
                // Buffer the block until predecessors arrive.
                self.pending
                    .insert(block_id, (block, missing.iter().copied().collect()));
                Err(missing)
            }
        }
    }

    /// Process a delta group received from a peer.
    ///
    /// Inserts blocks in order. Returns the list of block IDs that were
    /// successfully inserted, and a list of any blocks that couldn't be
    /// inserted (missing predecessors not in the delta or our blocklace).
    pub fn receive_delta(
        &mut self,
        peer: &NodeKey,
        delta: &DeltaGroup,
    ) -> (Vec<BlockId>, Vec<BlockId>) {
        let mut inserted = Vec::new();
        let mut failed = Vec::new();

        for block in &delta.blocks {
            match self.received_from(peer, block.clone()) {
                Ok(id) => inserted.push(id),
                Err(_) => failed.push(block.id()),
            }
        }

        (inserted, failed)
    }

    /// Handle an incoming dissemination message from a peer.
    ///
    /// Returns an optional response message to send back.
    pub fn handle_message(
        &mut self,
        from: &NodeKey,
        msg: DisseminationMessage,
    ) -> Option<DisseminationMessage> {
        match msg {
            DisseminationMessage::Push(delta) => {
                self.receive_delta(from, &delta);
                None
            }
            DisseminationMessage::Pull(request) => {
                let response = self.handle_pull(&request);
                Some(DisseminationMessage::PullResponse(response))
            }
            DisseminationMessage::PullResponse(response) => {
                self.receive_delta(from, &response.delta);
                None
            }
            DisseminationMessage::HaveFrontier(frontier) => {
                // Update peer knowledge from their frontier.
                let tip_ids: Vec<BlockId> = frontier.tips.values().copied().collect();
                self.peer_knowledge
                    .update_from_frontier(from, &tip_ids, &self.blocklace);
                // Respond with our own frontier so they know what we have.
                // (Only if we have blocks they might not know about.)
                let our_frontier = Frontier::from_blocklace(&self.blocklace);
                if our_frontier.tips != frontier.tips {
                    Some(DisseminationMessage::HaveFrontier(our_frontier))
                } else {
                    None
                }
            }
        }
    }

    /// Handle a pull request: build a causally-closed response containing
    /// the requested blocks and any predecessors the requester needs.
    fn handle_pull(&self, request: &BlockRequest) -> BlockResponse {
        // For each requested block, include it and all predecessors that
        // the requester might not have.
        let requester_known = self
            .peer_knowledge
            .known_by(&request.from)
            .cloned()
            .unwrap_or_default();

        let mut to_include: HashSet<BlockId> = HashSet::new();

        for &block_id in &request.missing {
            if self.blocklace.contains(&block_id) {
                // Include the block and all its predecessors not known to requester.
                let past = self.blocklace.causal_past(&block_id);
                for p in past {
                    if !requester_known.contains(&p) {
                        to_include.insert(p);
                    }
                }
            }
        }

        // Order topologically.
        let ordered = self.blocklace.topological_subset(&to_include);
        let blocks: Vec<Block> = ordered
            .iter()
            .filter_map(|id| self.blocklace.get(id).cloned())
            .collect();

        BlockResponse {
            delta: DeltaGroup::from_blocks(blocks),
        }
    }

    /// Get the list of block IDs we're missing (referenced by pending blocks).
    pub fn missing_blocks(&self) -> HashSet<BlockId> {
        let mut missing = HashSet::new();
        for (_, (_, deps)) in &self.pending {
            for dep in deps {
                if !self.blocklace.contains(dep) {
                    missing.insert(*dep);
                }
            }
        }
        missing
    }

    /// Generate a pull request for all blocks we're currently missing.
    pub fn generate_pull_request(&self) -> Option<DisseminationMessage> {
        let missing: Vec<BlockId> = self.missing_blocks().into_iter().collect();
        if missing.is_empty() {
            return None;
        }
        Some(DisseminationMessage::Pull(BlockRequest {
            missing,
            from: self.self_key,
        }))
    }

    /// Compute the delta to send based on frontier comparison.
    ///
    /// Given our frontier and a peer's frontier, determine what blocks
    /// the peer is missing.
    pub fn compute_delta_from_frontier(&self, their_frontier: &Frontier) -> DeltaGroup {
        // Their known set is the causal past of all their tips.
        let mut their_known = HashSet::new();
        for tip in their_frontier.tips.values() {
            if self.blocklace.contains(tip) {
                let past = self.blocklace.causal_past(tip);
                their_known.extend(past);
            }
        }

        let local_ids = self.blocklace.block_ids();
        let unknown_to_them: HashSet<BlockId> =
            local_ids.difference(&their_known).copied().collect();

        if unknown_to_them.is_empty() {
            return DeltaGroup::new();
        }

        let ordered = self.blocklace.topological_subset(&unknown_to_them);

        // Filter to causally-closed subset.
        let mut sendable = Vec::new();
        let mut they_will_know = their_known.clone();

        for block_id in &ordered {
            if let Some(block) = self.blocklace.get(block_id) {
                if block
                    .predecessors
                    .iter()
                    .all(|p| they_will_know.contains(p))
                {
                    sendable.push(block.clone());
                    they_will_know.insert(*block_id);
                }
            }
        }

        DeltaGroup::from_blocks(sendable)
    }

    /// Compute the delta to send based on frontier comparison, split into chunks.
    ///
    /// Like `compute_delta_from_frontier` but returns multiple causally-closed
    /// delta groups each bounded by `max_per_chunk` blocks.
    pub fn compute_delta_from_frontier_chunked(
        &self,
        their_frontier: &Frontier,
        max_per_chunk: usize,
    ) -> Vec<DeltaGroup> {
        let full_delta = self.compute_delta_from_frontier(their_frontier);
        if full_delta.is_empty() {
            return vec![];
        }
        chunk_delta_group(full_delta, max_per_chunk)
    }

    /// Get our current frontier as a message.
    pub fn frontier_message(&self) -> DisseminationMessage {
        DisseminationMessage::HaveFrontier(Frontier::from_blocklace(&self.blocklace))
    }

    /// Record that we successfully sent blocks to a peer.
    pub fn record_sent_to(&mut self, peer: &NodeKey, block_ids: &[BlockId]) {
        self.peer_knowledge.record_sent(peer, block_ids);
    }

    /// Try to flush pending blocks whose predecessors are now available.
    fn try_flush_pending(&mut self) {
        // Iterate until no more progress can be made.
        loop {
            let mut flushed = Vec::new();

            for (block_id, (block, missing)) in &self.pending {
                // Remove any predecessors that are now in the blocklace.
                let still_missing: HashSet<BlockId> = missing
                    .iter()
                    .filter(|p| !self.blocklace.contains(p))
                    .copied()
                    .collect();

                if still_missing.is_empty() {
                    // All predecessors present; try to insert.
                    flushed.push((*block_id, block.clone()));
                }
            }

            if flushed.is_empty() {
                break;
            }

            for (block_id, block) in flushed {
                self.pending.remove(&block_id);
                let _ = self.blocklace.insert(block);
            }
        }
    }
}

// =============================================================================
// Chunking Utilities
// =============================================================================

/// Split a causally-closed delta group into chunks of at most `max_per_chunk` blocks.
///
/// Each chunk is itself causally closed: within each chunk, blocks appear in
/// topological order and any block's predecessors are either in a prior chunk
/// (already sent) or earlier in the same chunk.
///
/// The input delta MUST already be in topological order (predecessors before
/// dependents). This is guaranteed by `blocks_to_send` and
/// `compute_delta_from_frontier`.
pub fn chunk_delta_group(delta: DeltaGroup, max_per_chunk: usize) -> Vec<DeltaGroup> {
    if delta.blocks.len() <= max_per_chunk {
        return vec![delta];
    }

    let mut chunks = Vec::new();
    let mut current_chunk = Vec::new();

    for block in delta.blocks {
        current_chunk.push(block);
        if current_chunk.len() >= max_per_chunk {
            chunks.push(DeltaGroup::from_blocks(std::mem::take(&mut current_chunk)));
        }
    }

    // Don't forget the trailing partial chunk.
    if !current_chunk.is_empty() {
        chunks.push(DeltaGroup::from_blocks(current_chunk));
    }

    chunks
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(id: u8) -> NodeKey {
        [id; 32]
    }

    fn make_block(creator: u8, seq: u64, preds: Vec<BlockId>, payload: &[u8]) -> Block {
        Block::new(make_key(creator), seq, preds, payload.to_vec())
    }

    // ─── Two nodes converge ──────────────────────────────────────────────────

    #[test]
    fn two_nodes_one_creates_blocks_other_receives() {
        let key_a = make_key(1);
        let key_b = make_key(2);

        let mut node_a = Disseminator::new(key_a);
        let mut node_b = Disseminator::new(key_b);

        // Node A creates some blocks.
        let b1 = node_a.create_block(b"block-1".to_vec());
        let b2 = node_a.create_block(b"block-2".to_vec());
        let b3 = node_a.create_block(b"block-3".to_vec());

        assert_eq!(node_a.blocklace().len(), 3);
        assert_eq!(node_b.blocklace().len(), 0);

        // Node A computes delta for Node B (thinks B has nothing).
        let delta = node_a.blocks_to_send(&key_b);
        assert_eq!(delta.len(), 3);
        assert!(delta.is_valid(&HashSet::new()));

        // Node B receives the delta.
        let (inserted, failed) = node_b.receive_delta(&key_a, &delta);
        assert_eq!(inserted.len(), 3);
        assert!(failed.is_empty());
        assert_eq!(node_b.blocklace().len(), 3);

        // Both nodes now have the same blocks.
        assert_eq!(
            node_a.blocklace().block_ids(),
            node_b.blocklace().block_ids()
        );

        // Verify the blocks were inserted correctly.
        assert!(node_b.blocklace().contains(&b1.id()));
        assert!(node_b.blocklace().contains(&b2.id()));
        assert!(node_b.blocklace().contains(&b3.id()));
    }

    // ─── Three nodes in chain ────────────────────────────────────────────────

    #[test]
    fn three_nodes_chain_propagation() {
        let key_a = make_key(1);
        let key_b = make_key(2);
        let key_c = make_key(3);

        let mut node_a = Disseminator::new(key_a);
        let mut node_b = Disseminator::new(key_b);
        let mut node_c = Disseminator::new(key_c);

        // A creates blocks.
        node_a.create_block(b"from-a-1".to_vec());
        node_a.create_block(b"from-a-2".to_vec());

        // A sends to B.
        let delta_ab = node_a.blocks_to_send(&key_b);
        node_b.receive_delta(&key_a, &delta_ab);
        node_a.record_sent_to(
            &key_b,
            &delta_ab.block_ids().into_iter().collect::<Vec<_>>(),
        );

        assert_eq!(node_b.blocklace().len(), 2);

        // B sends to C.
        let delta_bc = node_b.blocks_to_send(&key_c);
        node_c.receive_delta(&key_b, &delta_bc);

        assert_eq!(node_c.blocklace().len(), 2);

        // All three have the same blocks.
        assert_eq!(
            node_a.blocklace().block_ids(),
            node_c.blocklace().block_ids()
        );
    }

    // ─── Peer knowledge tracking ────────────────────────────────────────────

    #[test]
    fn peer_knowledge_updated_on_receive() {
        let key_a = make_key(1);
        let key_b = make_key(2);

        let mut node_b = Disseminator::new(key_b);

        // Create a genesis block from A.
        let b1 = make_block(1, 0, vec![], b"genesis");
        let b1_id = b1.id();

        // B receives b1 from A.
        node_b.received_from(&key_a, b1).unwrap();

        // B should now know that A has b1.
        let a_known = node_b.peer_knowledge().known_by(&key_a).unwrap();
        assert!(a_known.contains(&b1_id));
    }

    #[test]
    fn peer_knowledge_includes_transitive_predecessors() {
        let key_a = make_key(1);
        let key_b = make_key(2);

        let mut node_b = Disseminator::new(key_b);

        // Insert genesis first so b2 can reference it.
        let b1 = make_block(1, 0, vec![], b"first");
        let b1_id = b1.id();
        node_b.received_from(&key_a, b1).unwrap();

        let b2 = make_block(1, 1, vec![b1_id], b"second");
        let b2_id = b2.id();
        node_b.received_from(&key_a, b2).unwrap();

        // B should know that A has both b1 and b2.
        let a_known = node_b.peer_knowledge().known_by(&key_a).unwrap();
        assert!(a_known.contains(&b1_id));
        assert!(a_known.contains(&b2_id));
    }

    // ─── Delta group validation ─────────────────────────────────────────────

    #[test]
    fn delta_group_valid_when_causally_closed() {
        let b1 = make_block(1, 0, vec![], b"a");
        let b1_id = b1.id();
        let b2 = make_block(1, 1, vec![b1_id], b"b");

        // Delta with both blocks (in order) is valid against empty existing.
        let delta = DeltaGroup::from_blocks(vec![b1, b2]);
        assert!(delta.is_valid(&HashSet::new()));
    }

    #[test]
    fn delta_group_valid_when_predecessors_in_existing() {
        let b1 = make_block(1, 0, vec![], b"a");
        let b1_id = b1.id();
        let b2 = make_block(1, 1, vec![b1_id], b"b");

        // Delta with only b2, but b1 is in existing.
        let mut existing = HashSet::new();
        existing.insert(b1_id);
        let delta = DeltaGroup::from_blocks(vec![b2]);
        assert!(delta.is_valid(&existing));
    }

    #[test]
    fn delta_group_invalid_when_predecessors_missing() {
        let b1 = make_block(1, 0, vec![], b"a");
        let b1_id = b1.id();
        let b2 = make_block(1, 1, vec![b1_id], b"b");

        // Delta with only b2 and empty existing: not causally closed.
        let delta = DeltaGroup::from_blocks(vec![b2]);
        assert!(!delta.is_valid(&HashSet::new()));
    }

    #[test]
    fn delta_group_invalid_wrong_order() {
        let b1 = make_block(1, 0, vec![], b"a");
        let b1_id = b1.id();
        let b2 = make_block(1, 1, vec![b1_id], b"b");

        // Wrong order: b2 before b1.
        let delta = DeltaGroup::from_blocks(vec![b2, b1]);
        assert!(!delta.is_valid(&HashSet::new()));
    }

    // ─── Block request/response ─────────────────────────────────────────────

    #[test]
    fn pull_request_for_missing_predecessors() {
        let key_a = make_key(1);
        let key_b = make_key(2);

        let mut node_a = Disseminator::new(key_a);
        let mut node_b = Disseminator::new(key_b);

        // A creates a chain: b1 -> b2 -> b3.
        let b1 = node_a.create_block(b"one".to_vec());
        let b2 = node_a.create_block(b"two".to_vec());
        let _b3 = node_a.create_block(b"three".to_vec());

        // B only receives b3 (missing b1, b2 as predecessors).
        // Since b3 depends on b2 which depends on b1, insertion will fail.
        let b3_clone = node_a.blocklace().get(&_b3.id()).unwrap().clone();
        let result = node_b.received_from(&key_a, b3_clone);
        assert!(result.is_err());

        // B should have pending blocks and missing deps.
        let missing = node_b.missing_blocks();
        assert!(!missing.is_empty());

        // B generates a pull request.
        let pull = node_b.generate_pull_request().unwrap();
        match &pull {
            DisseminationMessage::Pull(req) => {
                assert!(!req.missing.is_empty());
            }
            _ => panic!("expected Pull message"),
        }

        // A handles the pull and sends response.
        let response = node_a.handle_message(&key_b, pull).unwrap();
        match &response {
            DisseminationMessage::PullResponse(resp) => {
                // Response should include b1 and b2 (and b3 if needed).
                assert!(!resp.delta.is_empty());
            }
            _ => panic!("expected PullResponse"),
        }

        // B receives the response.
        node_b.handle_message(&key_a, response);

        // Now B should have all blocks.
        assert!(node_b.blocklace().contains(&b1.id()));
        assert!(node_b.blocklace().contains(&b2.id()));
    }

    // ─── Frontier exchange ──────────────────────────────────────────────────

    #[test]
    fn frontier_exchange_determines_delta() {
        let key_a = make_key(1);
        let key_b = make_key(2);

        let mut node_a = Disseminator::new(key_a);
        let mut node_b = Disseminator::new(key_b);

        // A creates blocks.
        node_a.create_block(b"a1".to_vec());
        node_a.create_block(b"a2".to_vec());
        node_a.create_block(b"a3".to_vec());

        // B has nothing. B announces its (empty) frontier.
        let b_frontier = Frontier::from_blocklace(node_b.blocklace());

        // A computes delta based on B's frontier.
        let delta = node_a.compute_delta_from_frontier(&b_frontier);
        assert_eq!(delta.len(), 3);
        assert!(delta.is_valid(&HashSet::new()));

        // Now give B the first block and re-exchange.
        let first_block = node_a
            .blocklace()
            .get(&node_a.blocklace().topological_order()[0])
            .unwrap()
            .clone();
        node_b.received_from(&key_a, first_block.clone()).unwrap();

        let b_frontier_2 = Frontier::from_blocklace(node_b.blocklace());
        let delta_2 = node_a.compute_delta_from_frontier(&b_frontier_2);
        // Should only need 2 more blocks now.
        assert_eq!(delta_2.len(), 2);
    }

    // ─── Equivocator blocks ─────────────────────────────────────────────────

    #[test]
    fn equivocator_blocks_propagated() {
        let key_a = make_key(1);
        let key_b = make_key(2);

        let mut node_a = Disseminator::new(key_a);
        let mut node_b = Disseminator::new(key_b);

        // Create two conflicting blocks from the same creator at the same
        // sequence (equivocation). Both should be propagated as evidence.
        let equivocator_key = make_key(99);
        let b1 = Block::new(equivocator_key, 0, vec![], b"version-A".to_vec());
        let b2 = Block::new(equivocator_key, 0, vec![], b"version-B".to_vec());

        // Both are valid blocks (different payload -> different ID).
        assert_ne!(b1.id(), b2.id());

        // A has both equivocating blocks.
        node_a.blocklace_mut().insert(b1.clone()).unwrap();
        node_a.blocklace_mut().insert(b2.clone()).unwrap();

        // A sends delta to B.
        let delta = node_a.blocks_to_send(&key_b);
        assert_eq!(delta.len(), 2);

        // B receives both.
        let (inserted, _) = node_b.receive_delta(&key_a, &delta);
        assert_eq!(inserted.len(), 2);

        // B has both equivocating blocks (for evidence).
        assert!(node_b.blocklace().contains(&b1.id()));
        assert!(node_b.blocklace().contains(&b2.id()));
    }

    // ─── Network partition and merge ────────────────────────────────────────

    #[test]
    fn partition_and_merge_via_delta_exchange() {
        let key_a = make_key(1);
        let key_b = make_key(2);

        let mut node_a = Disseminator::new(key_a);
        let mut node_b = Disseminator::new(key_b);

        // Both create blocks independently (partitioned).
        node_a.create_block(b"a-during-partition-1".to_vec());
        node_a.create_block(b"a-during-partition-2".to_vec());

        node_b.create_block(b"b-during-partition-1".to_vec());
        node_b.create_block(b"b-during-partition-2".to_vec());

        assert_eq!(node_a.blocklace().len(), 2);
        assert_eq!(node_b.blocklace().len(), 2);

        // Partition heals. Exchange frontiers.
        let a_frontier = Frontier::from_blocklace(node_a.blocklace());
        let b_frontier = Frontier::from_blocklace(node_b.blocklace());

        // A computes what B needs.
        let delta_for_b = node_a.compute_delta_from_frontier(&b_frontier);
        // B computes what A needs.
        let delta_for_a = node_b.compute_delta_from_frontier(&a_frontier);

        // Both should have 2 blocks to send (the other's blocks).
        assert_eq!(delta_for_b.len(), 2);
        assert_eq!(delta_for_a.len(), 2);

        // Exchange deltas.
        node_b.receive_delta(&key_a, &delta_for_b);
        node_a.receive_delta(&key_b, &delta_for_a);

        // After merge, both have all 4 blocks.
        assert_eq!(node_a.blocklace().len(), 4);
        assert_eq!(node_b.blocklace().len(), 4);
        assert_eq!(
            node_a.blocklace().block_ids(),
            node_b.blocklace().block_ids()
        );
    }

    // ─── HaveFrontier message handling ──────────────────────────────────────

    #[test]
    fn have_frontier_updates_peer_knowledge() {
        let key_a = make_key(1);
        let key_b = make_key(2);

        let mut node_a = Disseminator::new(key_a);
        let mut node_b = Disseminator::new(key_b);

        // A creates blocks.
        node_a.create_block(b"x".to_vec());
        node_a.create_block(b"y".to_vec());

        // Give B the same blocks (simulate prior sync).
        let delta = node_a.blocks_to_send(&key_b);
        node_b.receive_delta(&key_a, &delta);

        // B sends its frontier to A.
        let frontier_msg = node_b.frontier_message();
        let response = node_a.handle_message(&key_b, frontier_msg);

        // A should know B has these blocks now (from frontier).
        // Since frontiers are equal, no response needed.
        assert!(response.is_none());

        // After frontier exchange, A's knowledge of B should include B's blocks.
        // Now create a new block on A.
        node_a.create_block(b"z".to_vec());

        // A should only need to send the new block to B.
        let delta2 = node_a.blocks_to_send(&key_b);
        assert_eq!(delta2.len(), 1);
    }

    // ─── Causal closure enforcement ────────────────────────────────────────

    #[test]
    fn blocks_to_send_always_causally_closed() {
        let key_a = make_key(1);
        let key_b = make_key(2);

        let mut node_a = Disseminator::new(key_a);

        // Create a chain of blocks.
        node_a.create_block(b"1".to_vec());
        node_a.create_block(b"2".to_vec());
        node_a.create_block(b"3".to_vec());
        node_a.create_block(b"4".to_vec());
        node_a.create_block(b"5".to_vec());

        // Simulate B knowing only the first block.
        let first_id = node_a.blocklace().topological_order()[0];
        let mut b_known = HashSet::new();
        b_known.insert(first_id);
        node_a.peer_knowledge.record_has(&key_b, &b_known);

        let delta = node_a.blocks_to_send(&key_b);
        // Delta should be causally closed relative to what B knows.
        assert!(delta.is_valid(&b_known));
        // Should contain blocks 2-5 (4 blocks).
        assert_eq!(delta.len(), 4);
    }

    // ─── Pending block flush ────────────────────────────────────────────────

    #[test]
    fn pending_blocks_flushed_when_deps_arrive() {
        let key_a = make_key(1);
        let key_b = make_key(2);

        let mut node_b = Disseminator::new(key_b);

        // Create blocks in order on A's side.
        let b1 = make_block(1, 0, vec![], b"first");
        let b1_id = b1.id();
        let b2 = make_block(1, 1, vec![b1_id], b"second");
        let b2_id = b2.id();

        // B receives b2 first (out of order) - should be pending.
        let result = node_b.received_from(&key_a, b2.clone());
        assert!(result.is_err());
        assert_eq!(node_b.blocklace().len(), 0);

        // Now B receives b1 - should flush b2 from pending.
        let result = node_b.received_from(&key_a, b1);
        assert!(result.is_ok());

        // Both blocks should now be in the blocklace.
        assert_eq!(node_b.blocklace().len(), 2);
        assert!(node_b.blocklace().contains(&b1_id));
        assert!(node_b.blocklace().contains(&b2_id));
    }

    // ─── Serialization round-trip ───────────────────────────────────────────

    #[test]
    fn dissemination_message_roundtrip() {
        let b1 = make_block(1, 0, vec![], b"test");
        let delta = DeltaGroup::from_blocks(vec![b1]);
        let msg = DisseminationMessage::Push(delta.clone());

        let bytes = postcard::to_stdvec(&msg).unwrap();
        let decoded: DisseminationMessage = postcard::from_bytes(&bytes).unwrap();

        match decoded {
            DisseminationMessage::Push(d) => {
                assert_eq!(d.blocks.len(), 1);
                assert_eq!(d, delta);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn frontier_message_roundtrip() {
        let mut tips = HashMap::new();
        tips.insert(make_key(1), [0xAB; 32]);
        tips.insert(make_key(2), [0xCD; 32]);
        let frontier = Frontier { tips };
        let msg = DisseminationMessage::HaveFrontier(frontier.clone());

        let bytes = postcard::to_stdvec(&msg).unwrap();
        let decoded: DisseminationMessage = postcard::from_bytes(&bytes).unwrap();

        match decoded {
            DisseminationMessage::HaveFrontier(f) => {
                assert_eq!(f, frontier);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn pull_request_roundtrip() {
        let msg = DisseminationMessage::Pull(BlockRequest {
            missing: vec![[0x11; 32], [0x22; 32]],
            from: make_key(5),
        });

        let bytes = postcard::to_stdvec(&msg).unwrap();
        let decoded: DisseminationMessage = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    // ─── Chunking tests ────────────────────────────────────────────────────

    #[test]
    fn chunk_delta_group_single_chunk_when_small() {
        let key_a = make_key(1);
        let mut node = Disseminator::new(key_a);

        // Create 5 blocks (less than any reasonable chunk size).
        for i in 0..5 {
            node.create_block(format!("block-{i}").into_bytes());
        }

        let delta = node.blocks_to_send(&make_key(2));
        assert_eq!(delta.len(), 5);

        let chunks = chunk_delta_group(delta, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 5);
    }

    #[test]
    fn chunk_delta_group_splits_into_multiple() {
        let key_a = make_key(1);
        let mut node = Disseminator::new(key_a);

        // Create 10 blocks.
        for i in 0..10 {
            node.create_block(format!("block-{i}").into_bytes());
        }

        let delta = node.blocks_to_send(&make_key(2));
        assert_eq!(delta.len(), 10);

        // Chunk with max size 3.
        let chunks = chunk_delta_group(delta, 3);
        assert_eq!(chunks.len(), 4); // 3+3+3+1
        assert_eq!(chunks[0].len(), 3);
        assert_eq!(chunks[1].len(), 3);
        assert_eq!(chunks[2].len(), 3);
        assert_eq!(chunks[3].len(), 1);
    }

    #[test]
    fn chunk_delta_group_each_chunk_causally_closed() {
        let key_a = make_key(1);
        let mut node = Disseminator::new(key_a);

        // Create a chain of blocks.
        for i in 0..9 {
            node.create_block(format!("block-{i}").into_bytes());
        }

        let delta = node.blocks_to_send(&make_key(2));
        let chunks = chunk_delta_group(delta, 3);

        // Each chunk should be causally closed given all prior chunks.
        let mut accumulated_known: HashSet<BlockId> = HashSet::new();
        for chunk in &chunks {
            assert!(chunk.is_valid(&accumulated_known));
            accumulated_known.extend(chunk.block_ids());
        }
    }

    #[test]
    fn blocks_to_send_chunked_matches_full_delta() {
        let key_a = make_key(1);
        let key_b = make_key(2);
        let mut node = Disseminator::new(key_a);

        for i in 0..7 {
            node.create_block(format!("block-{i}").into_bytes());
        }

        let full_delta = node.blocks_to_send(&key_b);
        let chunks = node.blocks_to_send_chunked(&key_b, 3);

        // Concatenated chunks should equal the full delta.
        let mut all_blocks: Vec<Block> = Vec::new();
        for chunk in &chunks {
            all_blocks.extend(chunk.blocks.clone());
        }
        assert_eq!(all_blocks.len(), full_delta.len());
        assert_eq!(
            all_blocks.iter().map(|b| b.id()).collect::<Vec<_>>(),
            full_delta.blocks.iter().map(|b| b.id()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn blocks_since_returns_new_blocks_only() {
        let key_a = make_key(1);
        let mut node = Disseminator::new(key_a);

        // Create initial blocks.
        node.create_block(b"first".to_vec());
        node.create_block(b"second".to_vec());

        // Record tips at this point.
        let tips_snapshot: HashMap<NodeKey, BlockId> = node
            .blocklace()
            .tips()
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect();

        // Create more blocks.
        node.create_block(b"third".to_vec());
        node.create_block(b"fourth".to_vec());

        // blocks_since should only return the new blocks.
        let new_blocks = node.blocks_since(&tips_snapshot);
        assert_eq!(new_blocks.len(), 2);
    }
}
