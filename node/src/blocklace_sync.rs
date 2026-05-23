//! Federation sync via the blocklace (Cordial Miners) consensus layer.
//!
//! Replaces the Morpheus BFT consensus with the blocklace DAG structure from the
//! Cordial Miners paper. The blocklace provides:
//! - Quiescent operation (no messages when idle)
//! - Efficient cordial dissemination (send peers blocks you think they need)
//! - Leaderless total ordering via the tau function
//! - Equivocation detection built into the data structure
//! - Constitutional membership amendments via voting
//!
//! The node participates in consensus by:
//! 1. Creating blocks when turns are submitted
//! 2. Disseminating blocks to peers via the existing QUIC gossip transport
//! 3. Running tau() ordering to produce the finalized total order
//! 4. Processing finalized turns through the TurnExecutor

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use pyana_blocklace::constitution::{Constitution, ConstitutionManager, MembershipVote};
use pyana_blocklace::finality::{Block, BlockError, BlockId, Blocklace, FinalityLevel, Payload};
use pyana_blocklace::ordering::tau;
use pyana_blocklace::pyana_bridge::PyanaBlocklaceBridge;
use pyana_net::gossip::{GossipEvent, GossipNetwork, TopicHandle};
use pyana_net::message::PeerMessage;
use pyana_net::node::{NodeId, PeerNode, PeerNodeConfig};
use tokio::sync::{Mutex, Notify, RwLock};
use tracing::{debug, error, info, warn};

use crate::state::{NodeEvent, NodeState};

// ─── Constants ──────────────────────────────────────────────────────────────

/// Gossip topic for blocklace dissemination messages.
pub const TOPIC_BLOCKLACE: &str = "pyana/blocklace";

/// Default COD budget for optimistic execution (number of outstanding turns).
const DEFAULT_COD_BUDGET: usize = 8;

/// Default timeout for constitutional waves (milliseconds).
const DEFAULT_CONSTITUTION_TIMEOUT_MS: u64 = 10_000;

// ─── Gossip Message Types ───────────────────────────────────────────────────

/// Wire-format message for blocklace gossip.
///
/// These replace the Morpheus consensus messages on the gossip network.
/// The protocol is quiescent: messages are only sent when a turn is submitted
/// or a new block arrives from a peer.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum BlocklaceGossipMessage {
    /// Push blocks I think you need (causally-closed delta).
    Push(Vec<Block>),
    /// Request blocks I'm missing.
    Pull(Vec<BlockId>),
    /// Response to a pull request.
    PullResponse(Vec<Block>),
    /// Lightweight frontier for efficient sync: creator -> tip block ID.
    Frontier(HashMap<[u8; 32], BlockId>),
}

// ─── Shared Blocklace State ─────────────────────────────────────────────────

/// Thread-safe handle to the blocklace consensus state.
///
/// Shared between the gossip receiver task and the HTTP API (for turn submission).
#[derive(Clone)]
pub struct BlocklaceHandle {
    /// The local blocklace (with signing key, equivocation detection, finality).
    pub lace: Arc<RwLock<Blocklace>>,
    /// The bridge for classifying turns and producing receipts.
    pub bridge: Arc<Mutex<PyanaBlocklaceBridge>>,
    /// Constitution manager tracking participants and membership amendments.
    pub constitution: Arc<RwLock<ConstitutionManager>>,
    /// The gossip network for broadcasting messages.
    pub gossip: Arc<GossipNetwork>,
    /// The blocklace gossip topic handle.
    pub topic: TopicHandle,
    /// Our own public key (node identity for the blocklace).
    pub self_key: [u8; 32],
    /// Index tracking which ordered blocks have already been executed.
    pub executed_up_to: Arc<RwLock<usize>>,
    /// Notify channel: signaled when new blocks arrive that may advance finality.
    /// This makes the executor truly quiescent -- no polling.
    pub finality_notify: Arc<Notify>,
}

impl BlocklaceHandle {
    /// Submit a turn to the blocklace.
    ///
    /// Creates a new block with the turn payload, adds it to the local blocklace,
    /// and pushes it to all known peers.
    ///
    /// Returns the block ID (used as a receipt handle) and the initial finality level.
    pub async fn submit_turn(&self, turn_data: Vec<u8>) -> (BlockId, FinalityLevel) {
        // Create the block in our local blocklace.
        let block = {
            let mut lace = self.lace.write().await;
            lace.add_block(Payload::Turn(turn_data))
        };
        let block_id = block.id();

        // Determine initial finality based on participant count.
        let constitution = self.constitution.read().await;
        let initial_finality = if constitution.current.participant_count() <= 1 {
            // Solo mode: immediately ordered (we're the only participant).
            // tau() with n=1 trivially finalizes every block.
            FinalityLevel::Ordered
        } else {
            FinalityLevel::Local
        };
        drop(constitution);

        // Notify the finality executor that new blocks are available.
        self.finality_notify.notify_one();

        // Disseminate to all peers via gossip.
        self.push_new_blocks().await;

        (block_id, initial_finality)
    }

    /// Push new blocks to peers via the gossip topic.
    ///
    /// Broadcasts all blocks from our local blocklace that peers may not have.
    /// In practice, since we broadcast on a topic, all subscribed peers see it.
    /// The protocol is quiescent: this is only called when we create a new block.
    async fn push_new_blocks(&self) {
        let lace = self.lace.read().await;

        // Get our latest block (just the one we created).
        let our_tip = match lace.tips().get(&self.self_key) {
            Some(tip) => *tip,
            None => return,
        };

        // Send the block (and its immediate context) to peers.
        if let Some(block) = lace.get(&our_tip) {
            let msg = BlocklaceGossipMessage::Push(vec![block.clone()]);
            self.broadcast_gossip_message(&msg).await;
        }
    }

    /// Broadcast a blocklace gossip message to the topic.
    async fn broadcast_gossip_message(&self, msg: &BlocklaceGossipMessage) {
        let encoded = match postcard::to_stdvec(msg) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!(error = %e, "failed to encode blocklace gossip message");
                return;
            }
        };

        let msg_hash = *blake3::hash(&encoded).as_bytes();
        let peer_msg = PeerMessage::PublishTurn {
            turn_hash: msg_hash,
            turn_data: encoded,
            causal_deps: vec![],
        };

        if let Err(e) = self.gossip.publish(&self.topic, &peer_msg).await {
            debug!(error = %e, "failed to publish blocklace message");
        }
    }

    /// Run the tau ordering function and return newly finalized turn payloads.
    ///
    /// This is the core consensus function: it computes the deterministic total
    /// order from the blocklace DAG using the Cordial Miners tau function,
    /// then returns any turns that have been newly ordered since the last call.
    pub async fn poll_finalized_turns(&self) -> Vec<(BlockId, Vec<u8>)> {
        let lace = self.lace.read().await;
        let constitution = self.constitution.read().await;
        let participants = constitution.current.participants.clone();
        drop(constitution);

        let mut executed_up_to = self.executed_up_to.write().await;

        // For solo mode (n=1): every block with a Turn payload is immediately
        // finalized in topological order. tau() handles this correctly because
        // with a single participant, every block trivially has supermajority.
        let ordered = if participants.len() <= 1 {
            // Solo: all blocks are ordered by sequence.
            let mut all_turn_blocks: Vec<(u64, BlockId)> = lace
                .iter()
                .filter_map(|(id, block)| match &block.payload {
                    Payload::Turn(_) => Some((block.seq, *id)),
                    _ => None,
                })
                .collect();
            all_turn_blocks.sort_by_key(|(seq, _)| *seq);
            all_turn_blocks
                .into_iter()
                .map(|(_, id)| id)
                .collect::<Vec<_>>()
        } else {
            // Multi-party: run the full Cordial Miners tau ordering.
            // We build an ordering-compatible blocklace and maintain a mapping
            // between the two BlockId types (they use different hash schemes).
            let (ordering_lace, id_map) = build_ordering_blocklace(&lace);
            let raw_order = tau(&ordering_lace, &participants);
            // Map ordering BlockIds back to finality BlockIds.
            raw_order
                .into_iter()
                .filter_map(|ordering_id| id_map.get(&ordering_id).copied())
                .collect::<Vec<_>>()
        };

        // Skip already-executed blocks.
        if ordered.len() <= *executed_up_to {
            return vec![];
        }

        let new_blocks = &ordered[*executed_up_to..];
        let mut turns = Vec::new();

        for block_id in new_blocks {
            if let Some(block) = lace.get(block_id) {
                if let Payload::Turn(ref data) = block.payload {
                    turns.push((*block_id, data.clone()));
                }
            }
        }

        *executed_up_to = ordered.len();
        turns
    }
}

/// Build a `pyana_blocklace::Blocklace` (the ordering-compatible type) from
/// the finality-layer blocklace. The ordering module's `tau()` function
/// operates on the simpler `Blocklace` from `lib.rs`.
///
/// Returns the ordering blocklace and a mapping from ordering BlockIds to
/// finality BlockIds (needed because the two types use different hash schemes).
fn build_ordering_blocklace(
    finality_lace: &Blocklace,
) -> (
    pyana_blocklace::Blocklace,
    HashMap<pyana_blocklace::BlockId, BlockId>,
) {
    let mut ordering_lace = pyana_blocklace::Blocklace::new();
    // Mapping from finality block ID -> ordering block ID (for predecessor translation)
    let mut finality_to_ordering: HashMap<BlockId, pyana_blocklace::BlockId> = HashMap::new();
    // Reverse mapping: ordering block ID -> finality block ID (for result translation)
    let mut ordering_to_finality: HashMap<pyana_blocklace::BlockId, BlockId> = HashMap::new();

    // Insert blocks in topological order (by sequence, then by creator for ties).
    let mut blocks: Vec<(&BlockId, &Block)> = finality_lace.iter().collect();
    blocks.sort_by(|(_, a), (_, b)| a.seq.cmp(&b.seq).then_with(|| a.creator.cmp(&b.creator)));

    for (finality_id, block) in blocks {
        // Translate predecessors from finality IDs to ordering IDs.
        let predecessors: Vec<pyana_blocklace::BlockId> = block
            .predecessors
            .iter()
            .filter_map(|p| finality_to_ordering.get(p).copied())
            .collect();
        let payload = match &block.payload {
            Payload::Turn(data) => data.clone(),
            Payload::Ack => vec![],
            Payload::Checkpoint { root, height } => {
                let mut buf = Vec::with_capacity(40);
                buf.extend_from_slice(root);
                buf.extend_from_slice(&height.to_le_bytes());
                buf
            }
            Payload::MembershipVote { .. } => vec![0x04],
            Payload::Data(data) => data.clone(),
        };
        let ordering_block =
            pyana_blocklace::Block::new(block.creator, block.seq, predecessors, payload);
        let ordering_id = ordering_block.id();
        let _ = ordering_lace.insert(ordering_block);

        // Record the bidirectional mapping.
        finality_to_ordering.insert(*finality_id, ordering_id);
        ordering_to_finality.insert(ordering_id, *finality_id);
    }
    (ordering_lace, ordering_to_finality)
}

// ─── Main Entry Point ───────────────────────────────────────────────────────

/// Run the blocklace-based federation sync as a background task.
///
/// This is the replacement for `federation_sync::run_federation_sync` when
/// `--consensus blocklace` is specified.
///
/// Key difference from Morpheus: QUIESCENT operation. No periodic timers for
/// consensus. Activity only when a turn is submitted or blocks arrive from peers.
pub async fn run_blocklace_sync(state: NodeState, gossip_port: u16) -> Option<BlocklaceHandle> {
    let peers = {
        let s = state.read().await;
        s.peers.clone()
    };

    // Get our signing key and derive the blocklace identity.
    let (gossip_signing_key, signing_key_bytes, our_public_key) = {
        let s = state.read().await;
        let sk = s.wallet.gossip_signing_key();
        let pk = s.wallet.public_key();
        (sk.clone(), sk.to_bytes(), pk)
    };

    // The finality::Blocklace uses ed25519_dalek::SigningKey directly.
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);
    let self_key: [u8; 32] = signing_key.verifying_key().to_bytes();

    // Determine participants: in solo mode, just ourselves.
    // In full mode, all known federation keys.
    let participants: Vec<[u8; 32]> = {
        let s = state.read().await;
        if s.known_federation_keys.is_empty() {
            // Solo mode or unconfigured: just ourselves.
            vec![self_key]
        } else {
            s.known_federation_keys.iter().map(|k| k.0).collect()
        }
    };

    let quorum_threshold = if participants.len() <= 1 {
        1
    } else {
        // 2f+1 where f = (n-1)/3
        (participants.len() * 2 / 3) + 1
    };

    info!(
        participants = participants.len(),
        quorum_threshold = quorum_threshold,
        solo = (participants.len() <= 1),
        "initializing blocklace consensus"
    );

    // Initialize the constitution with our participant set.
    let constitution = Constitution::new(participants.clone(), DEFAULT_CONSTITUTION_TIMEOUT_MS);
    let constitution_manager = ConstitutionManager::new(constitution);

    // Initialize the blocklace with our signing key and quorum threshold.
    let blocklace = Blocklace::new(signing_key.clone(), quorum_threshold);
    let bridge = PyanaBlocklaceBridge::new(DEFAULT_COD_BUDGET);

    // Create the PeerNode (QUIC endpoint) for gossip.
    let bind_addr_str = format!("0.0.0.0:{gossip_port}");
    let peer_node = match PeerNode::new(PeerNodeConfig {
        bind_addr: bind_addr_str.parse().unwrap(),
        ..PeerNodeConfig::default()
    })
    .await
    {
        Ok(node) => node,
        Err(e) => {
            error!(error = %e, "failed to create PeerNode for blocklace gossip");
            return None;
        }
    };

    let node_id: NodeId = peer_node.node_id();
    let endpoint = peer_node.endpoint().clone();

    info!(
        node_id = %pyana_net::node::fmt_node_id(&node_id),
        local_addr = %peer_node.local_addr(),
        "blocklace PeerNode ready"
    );

    // Build the signing key registry from known federation keys.
    let peer_keys_map = {
        let s = state.read().await;
        let mut peer_keys: std::collections::HashMap<NodeId, pyana_types::PublicKey> =
            std::collections::HashMap::new();
        for fed_key in &s.known_federation_keys {
            let peer_node_id = *blake3::hash(fed_key.as_bytes()).as_bytes();
            peer_keys.insert(peer_node_id, *fed_key);
        }
        peer_keys.insert(node_id, our_public_key);
        peer_keys
    };

    // Create the GossipNetwork with Ed25519 asymmetric signing.
    let gossip = Arc::new(GossipNetwork::new(
        endpoint,
        node_id,
        gossip_signing_key,
        peer_keys_map,
    ));

    // Parse peer addresses.
    let peer_addrs: Vec<SocketAddr> = peers
        .iter()
        .filter_map(|p| match p.parse::<SocketAddr>() {
            Ok(addr) => Some(addr),
            Err(e) => {
                warn!(peer = %p, error = %e, "invalid peer address, skipping");
                None
            }
        })
        .collect();

    // Join the blocklace gossip topic.
    let topic = match gossip.join_topic(TOPIC_BLOCKLACE, &peer_addrs).await {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, "failed to join blocklace topic");
            return None;
        }
    };

    // Subscribe to the blocklace topic for incoming messages.
    let mut blocklace_stream = match gossip.subscribe(&topic).await {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "failed to subscribe to blocklace topic");
            return None;
        }
    };

    // Also join the standard gossip topics so the node participates in
    // turn/revocation/intent data propagation (the blocklace handles ordering,
    // but existing topics handle non-consensus gossip).
    if !peer_addrs.is_empty() {
        let topic_turns = gossip
            .join_topic(crate::federation_sync::TOPIC_TURNS, &peer_addrs)
            .await;
        let topic_revocations = gossip
            .join_topic(crate::federation_sync::TOPIC_REVOCATIONS, &peer_addrs)
            .await;
        let topic_intents = gossip
            .join_topic(crate::federation_sync::TOPIC_INTENTS, &peer_addrs)
            .await;
        let topic_roots = gossip
            .join_topic(crate::federation_sync::TOPIC_ROOTS, &peer_addrs)
            .await;
        let topic_checkpoints = gossip
            .join_topic(crate::federation_sync::TOPIC_CHECKPOINTS, &peer_addrs)
            .await;
        let topic_decryption_shares = gossip
            .join_topic(crate::federation_sync::TOPIC_DECRYPTION_SHARES, &peer_addrs)
            .await;
        let topic_budget = gossip
            .join_topic(crate::federation_sync::TOPIC_BUDGET, &peer_addrs)
            .await;

        // If all topics joined successfully, build and store the GossipHandle.
        if let (Ok(tt), Ok(tr), Ok(ti), Ok(tro), Ok(tc), Ok(td), Ok(tb)) = (
            topic_turns,
            topic_revocations,
            topic_intents,
            topic_roots,
            topic_checkpoints,
            topic_decryption_shares,
            topic_budget,
        ) {
            let gossip_handle = crate::federation_sync::GossipHandle {
                network: gossip.clone(),
                topic_turns: tt,
                topic_revocations: tr,
                topic_intents: ti,
                topic_roots: tro,
                topic_checkpoints: tc,
                topic_decryption_shares: td,
                topic_budget: tb,
            };
            state.set_gossip(gossip_handle).await;
        }
    }

    // Record initial peer count metric.
    crate::metrics::set_federation_peers_connected(peer_addrs.len() as f64);

    // Build the shared handle.
    let lace = Arc::new(RwLock::new(blocklace));
    let bridge_handle = Arc::new(Mutex::new(bridge));
    let constitution_handle = Arc::new(RwLock::new(constitution_manager));
    let executed_up_to = Arc::new(RwLock::new(0usize));
    let finality_notify = Arc::new(Notify::new());

    let handle = BlocklaceHandle {
        lace: lace.clone(),
        bridge: bridge_handle,
        constitution: constitution_handle.clone(),
        gossip: gossip.clone(),
        topic: topic.clone(),
        self_key,
        executed_up_to,
        finality_notify: finality_notify.clone(),
    };

    info!("blocklace gossip layer initialized, processing messages");

    // ─── Spawn the Gossip Receiver Task ─────────────────────────────────────

    let handle_for_receiver = handle.clone();
    let state_for_receiver = state.clone();
    tokio::spawn(async move {
        loop {
            match blocklace_stream.recv().await {
                Some(GossipEvent::Message { from, message }) => {
                    handle_blocklace_message(
                        &handle_for_receiver,
                        &state_for_receiver,
                        from,
                        message,
                    )
                    .await;
                }
                Some(GossipEvent::PeerJoined(addr)) => {
                    info!(peer = %addr, "peer joined blocklace topic");
                    // When a new peer joins, send our frontier for efficient catch-up.
                    let lace = handle_for_receiver.lace.read().await;
                    let frontier_tips: HashMap<[u8; 32], BlockId> =
                        lace.tips().iter().map(|(k, v)| (*k, *v)).collect();
                    drop(lace);

                    let msg = BlocklaceGossipMessage::Frontier(frontier_tips);
                    handle_for_receiver.broadcast_gossip_message(&msg).await;
                }
                Some(GossipEvent::PeerLeft(addr)) => {
                    info!(peer = %addr, "peer left blocklace topic");
                }
                None => {
                    warn!("blocklace gossip stream ended");
                    break;
                }
            }
        }
    });

    // ─── Spawn the Finalized Turn Executor Task ─────────────────────────────

    spawn_finality_executor(state.clone(), handle.clone());

    Some(handle)
}

// ─── Message Handling ───────────────────────────────────────────────────────

/// Process an incoming blocklace gossip message.
async fn handle_blocklace_message(
    handle: &BlocklaceHandle,
    _state: &NodeState,
    from: SocketAddr,
    message: PeerMessage,
) {
    let turn_data = match message {
        PeerMessage::PublishTurn { turn_data, .. } => turn_data,
        _ => return,
    };

    let gossip_msg: BlocklaceGossipMessage = match postcard::from_bytes(&turn_data) {
        Ok(msg) => msg,
        Err(e) => {
            debug!(from = %from, error = %e, "failed to decode blocklace gossip message");
            return;
        }
    };

    match gossip_msg {
        BlocklaceGossipMessage::Push(blocks) => {
            handle_push(handle, from, blocks).await;
        }
        BlocklaceGossipMessage::Pull(missing_ids) => {
            handle_pull(handle, from, missing_ids).await;
        }
        BlocklaceGossipMessage::PullResponse(blocks) => {
            handle_push(handle, from, blocks).await;
        }
        BlocklaceGossipMessage::Frontier(their_tips) => {
            handle_frontier(handle, from, their_tips).await;
        }
    }
}

/// Handle a Push (or PullResponse) message: receive blocks into our blocklace.
async fn handle_push(handle: &BlocklaceHandle, from: SocketAddr, blocks: Vec<Block>) {
    if blocks.is_empty() {
        return;
    }

    let block_count = blocks.len();
    let mut lace = handle.lace.write().await;
    let mut inserted = 0usize;
    let mut missing_deps: Vec<BlockId> = Vec::new();

    for block in blocks {
        match lace.receive_block(block) {
            Ok(()) => {
                inserted += 1;
            }
            Err(BlockError::MissingPredecessor { missing, .. }) => {
                missing_deps.push(missing);
            }
            Err(BlockError::Equivocation {
                creator,
                seq,
                proof,
            }) => {
                let creator_hex: String = creator[..4].iter().map(|b| format!("{b:02x}")).collect();
                warn!(
                    from = %from,
                    creator = %creator_hex,
                    seq = seq,
                    "equivocation detected from peer"
                );
                // Auto-evict equivocator from the constitution.
                drop(lace);
                let mut constitution = handle.constitution.write().await;
                constitution.auto_evict(&proof);
                drop(constitution);
                lace = handle.lace.write().await;
                inserted += 1;
            }
            Err(BlockError::InvalidSignature { creator, seq }) => {
                let creator_hex: String = creator[..4].iter().map(|b| format!("{b:02x}")).collect();
                warn!(
                    from = %from,
                    creator = %creator_hex,
                    seq = seq,
                    "invalid signature on block from peer"
                );
            }
        }
    }
    drop(lace);

    if inserted > 0 {
        info!(
            from = %from,
            inserted = inserted,
            total_received = block_count,
            "received blocks from peer"
        );
        // Signal the finality executor that new blocks may advance ordering.
        handle.finality_notify.notify_one();
    }

    // If we have missing dependencies, request them.
    if !missing_deps.is_empty() {
        missing_deps.dedup();
        let pull_msg = BlocklaceGossipMessage::Pull(missing_deps);
        handle.broadcast_gossip_message(&pull_msg).await;
    }
}

/// Handle a Pull request: respond with requested blocks.
async fn handle_pull(handle: &BlocklaceHandle, from: SocketAddr, missing_ids: Vec<BlockId>) {
    if missing_ids.is_empty() {
        return;
    }

    let lace = handle.lace.read().await;

    // Collect requested blocks. For causal closure, also include their
    // predecessors that the requester may be missing.
    let mut to_send: Vec<Block> = Vec::new();
    let mut sent_ids = std::collections::HashSet::new();

    for block_id in &missing_ids {
        // Include the causal past of the requested block.
        let past = lace.causal_past(block_id);
        for past_id in &past {
            if !sent_ids.contains(past_id) {
                if let Some(block) = lace.get(past_id) {
                    to_send.push(block.clone());
                    sent_ids.insert(*past_id);
                }
            }
        }
        // Include the block itself.
        if !sent_ids.contains(block_id) {
            if let Some(block) = lace.get(block_id) {
                to_send.push(block.clone());
                sent_ids.insert(*block_id);
            }
        }
    }
    drop(lace);

    if !to_send.is_empty() {
        let response = BlocklaceGossipMessage::PullResponse(to_send);
        handle.broadcast_gossip_message(&response).await;
        debug!(from = %from, blocks = sent_ids.len(), "sent pull response");
    }
}

/// Handle a Frontier announcement: determine what the peer needs and push it.
async fn handle_frontier(
    handle: &BlocklaceHandle,
    from: SocketAddr,
    their_tips: HashMap<[u8; 32], BlockId>,
) {
    let lace = handle.lace.read().await;

    // Determine which blocks we have that the peer doesn't.
    // A peer with a given tip has all blocks in that tip's causal past.
    let mut their_known: std::collections::HashSet<BlockId> = std::collections::HashSet::new();
    for (_, tip_id) in &their_tips {
        if lace.contains(tip_id) {
            let past = lace.causal_past(tip_id);
            their_known.extend(past);
            their_known.insert(*tip_id);
        }
    }

    // Collect blocks they don't have.
    let mut to_send: Vec<Block> = Vec::new();
    for (id, block) in lace.iter() {
        if !their_known.contains(id) {
            to_send.push(block.clone());
        }
    }
    drop(lace);

    if !to_send.is_empty() {
        let msg = BlocklaceGossipMessage::Push(to_send.clone());
        handle.broadcast_gossip_message(&msg).await;
        debug!(from = %from, blocks = to_send.len(), "pushed delta after frontier exchange");
    }
}

// ─── Finalized Turn Executor ────────────────────────────────────────────────

/// Spawn a background task that waits for finalized blocks and executes their turns.
///
/// This task is QUIESCENT: it uses `Notify` to sleep until new blocks arrive.
/// No polling interval. Zero CPU when idle.
fn spawn_finality_executor(state: NodeState, handle: BlocklaceHandle) {
    tokio::spawn(async move {
        loop {
            // QUIESCENT: sleep until signaled that new blocks have arrived.
            handle.finality_notify.notified().await;

            // Process all newly finalized turns.
            let finalized_turns = handle.poll_finalized_turns().await;

            if finalized_turns.is_empty() {
                continue;
            }

            info!(
                turns = finalized_turns.len(),
                "executing finalized blocklace turns"
            );

            for (block_id, turn_data) in &finalized_turns {
                execute_finalized_turn(&state, &handle, *block_id, turn_data).await;
            }
        }
    });
}

/// Execute a single finalized turn against the node's ledger.
///
/// The turn has been totally ordered by the blocklace consensus (tau function)
/// and is ready for deterministic execution.
async fn execute_finalized_turn(
    state: &NodeState,
    _handle: &BlocklaceHandle,
    block_id: BlockId,
    turn_data: &[u8],
) {
    // Deserialize the signed turn.
    let signed_turn: pyana_sdk::SignedTurn = match postcard::from_bytes(turn_data) {
        Ok(st) => st,
        Err(e) => {
            warn!(
                block_id = %block_id,
                error = %e,
                "failed to deserialize turn from finalized block"
            );
            return;
        }
    };

    // Verify the turn signature.
    let computed_hash = signed_turn.turn.hash();
    if !signed_turn
        .signer
        .verify(&computed_hash, &signed_turn.signature)
    {
        warn!(
            block_id = %block_id,
            "invalid signature on finalized turn, skipping"
        );
        return;
    }

    let turn_hash_hex: String = computed_hash.iter().map(|b| format!("{b:02x}")).collect();

    // Execute the turn against the local ledger.
    let mut s = state.write().await;
    let mut executor = pyana_turn::TurnExecutor::new(pyana_turn::ComputronCosts::default());

    // Configure the executor with current node state.
    let local_fed_id = *blake3::hash(s.wallet.public_key().as_bytes()).as_bytes();
    executor.set_local_federation_id(local_fed_id);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    executor.set_timestamp(now);

    let current_height = s
        .store
        .latest_attested_root()
        .ok()
        .flatten()
        .map(|r| r.height)
        .unwrap_or(0);
    executor.set_block_height(current_height);

    let exec_result = executor.execute(&signed_turn.turn, &mut s.ledger);

    match exec_result {
        pyana_turn::TurnResult::Committed { receipt, .. } => {
            let receipt_hash_hex: String = receipt
                .turn_hash
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();

            // Resolve any pending turns waiting on this receipt.
            s.pending_turns.resolve(
                computed_hash,
                pyana_turn::ResolutionOutcome::Resolved(receipt.clone()),
            );

            // Process note commitments from NoteCreate effects.
            for tree in &signed_turn.turn.call_forest.roots {
                for effect in &tree.action.effects {
                    if let pyana_turn::Effect::NoteCreate { commitment, .. } = effect {
                        s.note_tree_append_commitment(&commitment.0);
                        let _ = s.store.store_note_commitment(commitment);
                    }
                }
            }

            // Append receipt to wallet.
            s.wallet.append_receipt(receipt.clone());
            drop(s);

            // Emit to WS subscribers.
            state.emit(NodeEvent::Receipt {
                hash: receipt_hash_hex,
            });

            info!(
                turn_hash = %turn_hash_hex,
                block_id = %block_id,
                "finalized turn executed (blocklace consensus)"
            );
        }
        pyana_turn::TurnResult::Rejected { reason, .. } => {
            warn!(
                turn_hash = %turn_hash_hex,
                block_id = %block_id,
                reason = %reason,
                "finalized turn rejected"
            );
        }
        pyana_turn::TurnResult::Expired => {
            warn!(
                turn_hash = %turn_hash_hex,
                block_id = %block_id,
                "finalized turn expired"
            );
        }
        pyana_turn::TurnResult::Pending => {
            debug!(
                turn_hash = %turn_hash_hex,
                block_id = %block_id,
                "finalized turn pending"
            );
        }
    }
}

// ─── Membership Vote Processing ─────────────────────────────────────────────

/// Process a MembershipVote payload from a finalized block.
///
/// When a block with a MembershipVote payload reaches finality (appears in tau
/// output), we apply the vote to the ConstitutionManager. If the vote causes
/// a proposal to pass, the constitution is amended.
#[allow(dead_code)]
async fn process_membership_vote(
    handle: &BlocklaceHandle,
    _block_id: BlockId,
    voter: [u8; 32],
    vote: &MembershipVote,
) {
    let mut constitution = handle.constitution.write().await;

    // Record the vote.
    let passed = constitution.submit_vote(vote, voter);

    if let Some(proposal_block) = passed {
        // The proposal has reached threshold -- apply it.
        if constitution.apply_if_passed(&proposal_block) {
            let new_count = constitution.current.participant_count();
            let new_version = constitution.version();
            info!(
                proposal_block = %proposal_block,
                new_participant_count = new_count,
                constitution_version = new_version,
                "constitution amended via membership vote"
            );
        }
    }
}
