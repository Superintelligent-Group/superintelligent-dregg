//! Channel-based networking between federation nodes.
//!
//! This module provides a simulated network layer using crossbeam channels.
//! Each node has an inbox (receiver) and the network maintains the routing
//! table. Messages can be unicast or broadcast.
//!
//! The network also supports fault injection:
//! - Dropping messages to/from specific nodes (simulating partitions)
//! - Adding latency (not used in synchronous demo, but API is ready)
//! - Node crash simulation (stop delivering messages)

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::types::{AddressedMessage, ConsensusMessage};

// =============================================================================
// Network Layer
// =============================================================================

/// A simulated network connecting federation nodes via channels.
pub struct Network {
    /// Senders for each node's inbox.
    senders: Vec<Sender<AddressedMessage>>,
    /// Receivers for each node's inbox.
    receivers: Vec<Receiver<AddressedMessage>>,
    /// Number of nodes.
    pub num_nodes: usize,
    /// Nodes that are "crashed" — messages to/from them are dropped.
    crashed_nodes: Vec<bool>,
    /// Total messages sent through the network.
    pub messages_sent: u64,
    /// Total messages dropped due to faults.
    pub messages_dropped: u64,
}

impl Network {
    /// Create a new network for n nodes.
    pub fn new(num_nodes: usize) -> Self {
        let mut senders = Vec::with_capacity(num_nodes);
        let mut receivers = Vec::with_capacity(num_nodes);

        for _ in 0..num_nodes {
            let (tx, rx) = bounded(256);
            senders.push(tx);
            receivers.push(rx);
        }

        Self {
            senders,
            receivers,
            num_nodes,
            crashed_nodes: vec![false; num_nodes],
            messages_sent: 0,
            messages_dropped: 0,
        }
    }

    /// Send a message through the network.
    pub fn send(&mut self, msg: AddressedMessage) {
        // Don't deliver messages from crashed nodes.
        if self.crashed_nodes[msg.from] {
            self.messages_dropped += 1;
            return;
        }

        if msg.is_broadcast() {
            // Deliver to all nodes except sender.
            for i in 0..self.num_nodes {
                if i == msg.from {
                    continue;
                }
                if self.crashed_nodes[i] {
                    self.messages_dropped += 1;
                    continue;
                }
                let _ = self.senders[i].try_send(msg.clone());
                self.messages_sent += 1;
            }
        } else {
            // Unicast.
            if self.crashed_nodes[msg.to] {
                self.messages_dropped += 1;
                return;
            }
            let _ = self.senders[msg.to].try_send(msg);
            self.messages_sent += 1;
        }
    }

    /// Receive all pending messages for a node (non-blocking drain).
    pub fn receive_all(&self, node_id: usize) -> Vec<AddressedMessage> {
        let mut messages = Vec::new();
        while let Ok(msg) = self.receivers[node_id].try_recv() {
            messages.push(msg);
        }
        messages
    }

    /// Receive a single message for a node (non-blocking).
    pub fn try_receive(&self, node_id: usize) -> Option<AddressedMessage> {
        self.receivers[node_id].try_recv().ok()
    }

    /// Simulate a node crash (stop delivering to/from this node).
    pub fn crash_node(&mut self, node_id: usize) {
        self.crashed_nodes[node_id] = true;
        // Drain any pending messages for this node.
        while self.receivers[node_id].try_recv().is_ok() {}
    }

    /// Recover a crashed node.
    pub fn recover_node(&mut self, node_id: usize) {
        self.crashed_nodes[node_id] = false;
    }

    /// Check if a node is crashed.
    pub fn is_crashed(&self, node_id: usize) -> bool {
        self.crashed_nodes[node_id]
    }

    /// Broadcast a consensus message from a node.
    pub fn broadcast(&mut self, from: usize, message: ConsensusMessage) {
        self.send(AddressedMessage::broadcast(from, message));
    }

    /// Send a directed consensus message.
    pub fn unicast(&mut self, from: usize, to: usize, message: ConsensusMessage) {
        self.send(AddressedMessage::directed(from, to, message));
    }

    /// Get the number of online (non-crashed) nodes.
    pub fn online_count(&self) -> usize {
        self.crashed_nodes.iter().filter(|&&c| !c).count()
    }
}

// =============================================================================
// Network Statistics
// =============================================================================

/// Statistics about network usage.
#[derive(Clone, Debug)]
pub struct NetworkStats {
    pub total_messages_sent: u64,
    pub total_messages_dropped: u64,
    pub nodes_online: usize,
    pub nodes_crashed: usize,
}

impl Network {
    /// Get network statistics.
    pub fn stats(&self) -> NetworkStats {
        let online = self.online_count();
        NetworkStats {
            total_messages_sent: self.messages_sent,
            total_messages_dropped: self.messages_dropped,
            nodes_online: online,
            nodes_crashed: self.num_nodes - online,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ConsensusMessage;

    #[test]
    fn broadcast_delivery() {
        let mut net = Network::new(4);

        net.broadcast(0, ConsensusMessage::GetAttestedRoot);

        // All nodes except sender should receive the message.
        assert_eq!(net.receive_all(0).len(), 0);
        assert_eq!(net.receive_all(1).len(), 1);
        assert_eq!(net.receive_all(2).len(), 1);
        assert_eq!(net.receive_all(3).len(), 1);
        assert_eq!(net.messages_sent, 3);
    }

    #[test]
    fn unicast_delivery() {
        let mut net = Network::new(4);

        net.unicast(0, 2, ConsensusMessage::GetAttestedRoot);

        assert_eq!(net.receive_all(0).len(), 0);
        assert_eq!(net.receive_all(1).len(), 0);
        assert_eq!(net.receive_all(2).len(), 1);
        assert_eq!(net.receive_all(3).len(), 0);
        assert_eq!(net.messages_sent, 1);
    }

    #[test]
    fn crashed_node_no_delivery() {
        let mut net = Network::new(4);

        net.crash_node(2);
        net.broadcast(0, ConsensusMessage::GetAttestedRoot);

        // Node 2 should not receive the message.
        assert_eq!(net.receive_all(1).len(), 1);
        assert_eq!(net.receive_all(2).len(), 0);
        assert_eq!(net.receive_all(3).len(), 1);
        assert_eq!(net.messages_sent, 2);
        assert_eq!(net.messages_dropped, 1);
    }

    #[test]
    fn crashed_node_cannot_send() {
        let mut net = Network::new(4);

        net.crash_node(0);
        net.broadcast(0, ConsensusMessage::GetAttestedRoot);

        // Nobody should receive messages from crashed node.
        assert_eq!(net.receive_all(1).len(), 0);
        assert_eq!(net.receive_all(2).len(), 0);
        assert_eq!(net.receive_all(3).len(), 0);
        assert_eq!(net.messages_dropped, 1);
    }

    #[test]
    fn recover_node() {
        let mut net = Network::new(4);

        net.crash_node(2);
        assert!(net.is_crashed(2));
        assert_eq!(net.online_count(), 3);

        net.recover_node(2);
        assert!(!net.is_crashed(2));
        assert_eq!(net.online_count(), 4);

        net.broadcast(0, ConsensusMessage::GetAttestedRoot);
        assert_eq!(net.receive_all(2).len(), 1);
    }

    #[test]
    fn stats() {
        let mut net = Network::new(4);
        net.crash_node(3);

        let stats = net.stats();
        assert_eq!(stats.nodes_online, 3);
        assert_eq!(stats.nodes_crashed, 1);
    }
}
