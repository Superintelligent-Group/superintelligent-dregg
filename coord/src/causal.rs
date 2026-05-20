//! Layer 1: Causal Chaining.
//!
//! Every turn a node produces includes hash-pointers to the latest turns it has seen.
//! This creates a DAG of happened-before relationships. Any node can verify
//! "turn T2 happened after turn T1" by following the hash links.
//!
//! No global ordering needed — just local causal consistency.

use std::collections::{HashMap, HashSet, VecDeque};

use pyana_cell::Ledger;
use pyana_turn::{Turn, TurnReceipt, TurnResult};
use serde::{Deserialize, Serialize};

use crate::error::CoordError;

// ─── CausalTurn ────────────────────────────────────────────────────────────────

/// A turn with causal metadata: hash-pointers to dependencies,
/// producing node identity, and a per-node sequence number.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CausalTurn {
    /// The actual turn (from pyana-turn).
    pub turn: Turn,
    /// Hashes of turns this one causally depends on (happened-before).
    pub causal_deps: Vec<[u8; 32]>,
    /// Identity of the producing node.
    pub node_id: [u8; 32],
    /// Per-node monotonic sequence number (0-indexed).
    pub sequence: u64,
    /// BLAKE3 hash of (turn_hash, deps, node_id, sequence).
    /// Serves as the unique identity of this causal turn.
    pub hash: [u8; 32],
}

impl CausalTurn {
    /// Create a new CausalTurn, computing its hash from the contents.
    pub fn new(
        mut turn: Turn,
        causal_deps: Vec<[u8; 32]>,
        node_id: [u8; 32],
        sequence: u64,
    ) -> Self {
        let turn_hash = turn.hash();
        let hash = Self::compute_hash(&turn_hash, &causal_deps, &node_id, sequence);
        CausalTurn {
            turn,
            causal_deps,
            node_id,
            sequence,
            hash,
        }
    }

    /// Compute the causal turn hash from its components.
    pub fn compute_hash(
        turn_hash: &[u8; 32],
        causal_deps: &[[u8; 32]],
        node_id: &[u8; 32],
        sequence: u64,
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"pyana-coord:causal-turn");
        hasher.update(turn_hash);
        for dep in causal_deps {
            hasher.update(dep);
        }
        hasher.update(node_id);
        hasher.update(&sequence.to_le_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Verify that the claimed hash matches the actual content hash.
    pub fn verify_hash(&mut self) -> bool {
        let turn_hash = self.turn.hash();
        let computed =
            Self::compute_hash(&turn_hash, &self.causal_deps, &self.node_id, self.sequence);
        computed == self.hash
    }
}

// ─── CausalDag ─────────────────────────────────────────────────────────────────

/// The causal DAG: a directed acyclic graph of happened-before relationships.
///
/// Nodes are turn hashes, edges point from a turn to its causal dependencies.
/// This lets us answer queries like:
/// - "Did T1 happen before T2?" (T1 is an ancestor of T2)
/// - "Are T1 and T2 concurrent?" (neither is ancestor of the other)
/// - "What is the current frontier?" (turns with no successors)
#[derive(Clone, Debug, Default)]
pub struct CausalDag {
    /// Forward edges: turn_hash -> set of turns that depend on it (successors).
    successors: HashMap<[u8; 32], HashSet<[u8; 32]>>,
    /// Backward edges: turn_hash -> set of turns it depends on (dependencies).
    dependencies: HashMap<[u8; 32], HashSet<[u8; 32]>>,
    /// All turn hashes in the DAG.
    all_turns: HashSet<[u8; 32]>,
    /// The current frontier: turns that have no successors yet.
    frontier: HashSet<[u8; 32]>,
}

impl CausalDag {
    /// Create a new empty causal DAG.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a turn into the DAG with its causal dependencies.
    ///
    /// Returns an error if:
    /// - The turn already exists (duplicate).
    /// - Any dependency is missing from the DAG (use `has_all_deps` to check first).
    pub fn insert(
        &mut self,
        turn_hash: [u8; 32],
        deps: &[[u8; 32]],
    ) -> Result<(), CoordError> {
        if self.all_turns.contains(&turn_hash) {
            return Err(CoordError::DuplicateTurn { hash: turn_hash });
        }

        // Verify all dependencies are present.
        for dep in deps {
            if !self.all_turns.contains(dep) {
                return Err(CoordError::MissingDependency {
                    turn_hash,
                    dep_hash: *dep,
                });
            }
        }

        // Insert the turn.
        self.all_turns.insert(turn_hash);
        self.dependencies.insert(turn_hash, deps.iter().copied().collect());
        self.successors.entry(turn_hash).or_default();

        // Register as successor of each dependency.
        for dep in deps {
            self.successors.entry(*dep).or_default().insert(turn_hash);
            // Remove dep from frontier since it now has a successor.
            self.frontier.remove(dep);
        }

        // New turn is on the frontier (no successors yet).
        self.frontier.insert(turn_hash);

        Ok(())
    }

    /// Insert a genesis turn (no dependencies required).
    pub fn insert_genesis(&mut self, turn_hash: [u8; 32]) -> Result<(), CoordError> {
        self.insert(turn_hash, &[])
    }

    /// Check if all dependencies for a turn are present in the DAG.
    pub fn has_all_deps(&self, deps: &[[u8; 32]]) -> bool {
        deps.iter().all(|d| self.all_turns.contains(d))
    }


    /// Get the missing dependencies for a set of required deps.
    pub fn missing_deps(&self, deps: &[[u8; 32]]) -> Vec<[u8; 32]> {
        deps.iter()
            .filter(|d| !self.all_turns.contains(*d))
            .copied()
            .collect()
    }

    /// Check if `ancestor` happened before `descendant` (ancestor is reachable from descendant
    /// by following dependency edges backward).
    pub fn happened_before(&self, ancestor: &[u8; 32], descendant: &[u8; 32]) -> bool {
        if ancestor == descendant {
            return false; // A turn does not happen before itself.
        }
        if !self.all_turns.contains(ancestor) || !self.all_turns.contains(descendant) {
            return false;
        }

        // BFS backward from descendant through dependencies.
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*descendant);
        visited.insert(*descendant);

        while let Some(current) = queue.pop_front() {
            if let Some(deps) = self.dependencies.get(&current) {
                for dep in deps {
                    if dep == ancestor {
                        return true;
                    }
                    if visited.insert(*dep) {
                        queue.push_back(*dep);
                    }
                }
            }
        }

        false
    }

    /// Check if two turns are concurrent (neither happened before the other).
    pub fn are_concurrent(&self, a: &[u8; 32], b: &[u8; 32]) -> bool {
        if a == b {
            return false;
        }
        !self.happened_before(a, b) && !self.happened_before(b, a)
    }

    /// Get the current causal frontier: turns with no successors.
    pub fn frontier(&self) -> Vec<[u8; 32]> {
        self.frontier.iter().copied().collect()
    }

    /// Get all direct dependencies of a turn.
    pub fn deps_of(&self, turn_hash: &[u8; 32]) -> Option<&HashSet<[u8; 32]>> {
        self.dependencies.get(turn_hash)
    }

    /// Get all direct successors of a turn.
    pub fn successors_of(&self, turn_hash: &[u8; 32]) -> Option<&HashSet<[u8; 32]>> {
        self.successors.get(turn_hash)
    }

    /// Number of turns in the DAG.
    pub fn len(&self) -> usize {
        self.all_turns.len()
    }

    /// Whether the DAG is empty.
    pub fn is_empty(&self) -> bool {
        self.all_turns.is_empty()
    }

    /// Whether a turn exists in the DAG.
    pub fn contains(&self, turn_hash: &[u8; 32]) -> bool {
        self.all_turns.contains(turn_hash)
    }

    /// Get the causal depth of a turn (longest path from any genesis turn to this one).
    pub fn depth(&self, turn_hash: &[u8; 32]) -> Option<usize> {
        if !self.all_turns.contains(turn_hash) {
            return None;
        }

        let mut max_depth = 0;
        let mut stack: Vec<([u8; 32], usize)> = vec![(*turn_hash, 0)];
        let mut visited = HashSet::new();

        while let Some((current, depth)) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            if let Some(deps) = self.dependencies.get(&current) {
                if deps.is_empty() {
                    max_depth = max_depth.max(depth);
                } else {
                    for dep in deps {
                        stack.push((*dep, depth + 1));
                    }
                }
            } else {
                max_depth = max_depth.max(depth);
            }
        }

        Some(max_depth)
    }

    /// Get all turns in causal (topological) order.
    /// Returns turns such that if T1 happened before T2, T1 appears first.
    pub fn topological_order(&self) -> Vec<[u8; 32]> {
        let mut in_degree: HashMap<[u8; 32], usize> = HashMap::new();
        for turn in &self.all_turns {
            in_degree.entry(*turn).or_insert(0);
            if let Some(succs) = self.successors.get(turn) {
                for succ in succs {
                    *in_degree.entry(*succ).or_insert(0) += 1;
                }
            }
        }

        // Note: we're treating "dependencies" as incoming edges.
        // A turn with no deps has in_degree 0 in the dependency sense.
        let mut in_deg: HashMap<[u8; 32], usize> = HashMap::new();
        for turn in &self.all_turns {
            let dep_count = self
                .dependencies
                .get(turn)
                .map(|d| d.len())
                .unwrap_or(0);
            in_deg.insert(*turn, dep_count);
        }

        let mut queue: VecDeque<[u8; 32]> = VecDeque::new();
        for (turn, &deg) in &in_deg {
            if deg == 0 {
                queue.push_back(*turn);
            }
        }

        let mut result = Vec::with_capacity(self.all_turns.len());
        while let Some(turn) = queue.pop_front() {
            result.push(turn);
            if let Some(succs) = self.successors.get(&turn) {
                for succ in succs {
                    if let Some(deg) = in_deg.get_mut(succ) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(*succ);
                        }
                    }
                }
            }
        }

        result
    }
}

// ─── CausalLedger ──────────────────────────────────────────────────────────────

/// A ledger that tracks causal ordering of turns.
///
/// Combines a standard `Ledger` (world state) with a `CausalDag` (ordering)
/// and per-node frontier tracking.
#[derive(Clone, Debug)]
pub struct CausalLedger {
    /// The underlying world-state ledger.
    pub ledger: Ledger,
    /// The causal DAG tracking happened-before relationships.
    pub dag: CausalDag,
    /// Per-node: the latest causal turn hashes (their local frontier).
    pub node_frontiers: HashMap<[u8; 32], Vec<[u8; 32]>>,
    /// Per-node: the next expected sequence number.
    node_sequences: HashMap<[u8; 32], u64>,
    /// Storage for turn receipts indexed by causal turn hash.
    receipts: HashMap<[u8; 32], TurnReceipt>,
    /// The TurnExecutor costs configuration.
    executor_costs: pyana_turn::ComputronCosts,
    /// Current timestamp for executor.
    current_timestamp: i64,
    /// Current block height for executor.
    block_height: u64,
}

impl CausalLedger {
    /// Create a new causal ledger with an empty ledger and DAG.
    pub fn new() -> Self {
        CausalLedger {
            ledger: Ledger::new(),
            dag: CausalDag::new(),
            node_frontiers: HashMap::new(),
            node_sequences: HashMap::new(),
            receipts: HashMap::new(),
            executor_costs: pyana_turn::ComputronCosts::zero(),
            current_timestamp: 0,
            block_height: 0,
        }
    }

    /// Create a causal ledger wrapping an existing ledger.
    pub fn with_ledger(ledger: Ledger) -> Self {
        CausalLedger {
            ledger,
            dag: CausalDag::new(),
            node_frontiers: HashMap::new(),
            node_sequences: HashMap::new(),
            receipts: HashMap::new(),
            executor_costs: pyana_turn::ComputronCosts::zero(),
            current_timestamp: 0,
            block_height: 0,
        }
    }

    /// Set the computron cost configuration.
    pub fn set_costs(&mut self, costs: pyana_turn::ComputronCosts) {
        self.executor_costs = costs;
    }

    /// Set the current timestamp for turn execution.
    pub fn set_timestamp(&mut self, ts: i64) {
        self.current_timestamp = ts;
    }

    /// Set the current block height for turn execution.
    pub fn set_block_height(&mut self, height: u64) {
        self.block_height = height;
    }

    /// Apply a causal turn to the ledger.
    ///
    /// This:
    /// 1. Verifies the hash is correct.
    /// 2. Checks all causal dependencies are present.
    /// 3. Validates the per-node sequence number.
    /// 4. Inserts the turn into the causal DAG.
    /// 5. Executes the turn against the ledger.
    /// 6. Updates the node's frontier.
    ///
    /// Returns the TurnResult on success.
    pub fn apply_causal_turn(&mut self, ct: &CausalTurn) -> Result<TurnResult, CoordError> {
        // Step 1: Verify hash integrity.
        let mut turn_copy = ct.turn.clone();
        let turn_hash = turn_copy.hash();
        let computed_hash =
            CausalTurn::compute_hash(&turn_hash, &ct.causal_deps, &ct.node_id, ct.sequence);
        if computed_hash != ct.hash {
            return Err(CoordError::HashMismatch {
                claimed: ct.hash,
                computed: computed_hash,
            });
        }

        // Step 2: Check causal readiness.
        if !self.is_causally_ready(ct) {
            let missing = self.missing_deps(ct);
            if let Some(first_missing) = missing.first() {
                return Err(CoordError::MissingDependency {
                    turn_hash: ct.hash,
                    dep_hash: *first_missing,
                });
            }
        }

        // Step 3: Check sequence number.
        let expected_seq = self.node_sequences.get(&ct.node_id).copied().unwrap_or(0);
        if ct.sequence != expected_seq {
            return Err(CoordError::SequenceGap {
                node_id: ct.node_id,
                expected: expected_seq,
                got: ct.sequence,
            });
        }

        // Step 4: Insert into the causal DAG.
        self.dag.insert(ct.hash, &ct.causal_deps)?;

        // Step 5: Execute the turn against the ledger.
        let executor = {
            let mut ex = pyana_turn::TurnExecutor::new(self.executor_costs.clone());
            ex.set_timestamp(self.current_timestamp);
            ex.set_block_height(self.block_height);
            ex
        };

        let result = executor.execute(&ct.turn, &mut self.ledger);

        // Step 6: Update node tracking regardless of turn result.
        // The turn is in the DAG even if it was rejected (it still happened causally).
        *self.node_sequences.entry(ct.node_id).or_insert(0) = ct.sequence + 1;

        // Update node frontier: replace old frontier entries with this new turn.
        let frontier = self.node_frontiers.entry(ct.node_id).or_default();
        // Remove any deps that were in this node's frontier.
        frontier.retain(|h| !ct.causal_deps.contains(h));
        frontier.push(ct.hash);

        // Store receipt if committed.
        if let TurnResult::Committed { ref receipt, .. } = result {
            self.receipts.insert(ct.hash, receipt.clone());
        }

        Ok(result)
    }

    /// Get the current causal frontier (all turns with no successors).
    pub fn frontier(&self) -> Vec<[u8; 32]> {
        self.dag.frontier()
    }

    /// Check if a causal turn has all its dependencies present in the DAG.
    pub fn is_causally_ready(&self, ct: &CausalTurn) -> bool {
        self.dag.has_all_deps(&ct.causal_deps)
    }

    /// Get the missing dependencies for a causal turn.
    pub fn missing_deps(&self, ct: &CausalTurn) -> Vec<[u8; 32]> {
        self.dag.missing_deps(&ct.causal_deps)
    }

    /// Get the frontier for a specific node.
    pub fn node_frontier(&self, node_id: &[u8; 32]) -> Option<&Vec<[u8; 32]>> {
        self.node_frontiers.get(node_id)
    }

    /// Get the next expected sequence number for a node.
    pub fn next_sequence(&self, node_id: &[u8; 32]) -> u64 {
        self.node_sequences.get(node_id).copied().unwrap_or(0)
    }

    /// Get a receipt by causal turn hash.
    pub fn receipt(&self, turn_hash: &[u8; 32]) -> Option<&TurnReceipt> {
        self.receipts.get(turn_hash)
    }

    /// Check if turn A happened before turn B in the causal ordering.
    pub fn happened_before(&self, a: &[u8; 32], b: &[u8; 32]) -> bool {
        self.dag.happened_before(a, b)
    }

    /// Check if two turns are concurrent (neither causally precedes the other).
    pub fn are_concurrent(&self, a: &[u8; 32], b: &[u8; 32]) -> bool {
        self.dag.are_concurrent(a, b)
    }

    /// Get a reference to the underlying ledger.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Get a mutable reference to the underlying ledger.
    pub fn ledger_mut(&mut self) -> &mut Ledger {
        &mut self.ledger
    }

    /// Get a reference to the causal DAG.
    pub fn dag(&self) -> &CausalDag {
        &self.dag
    }

    /// Total number of causal turns applied.
    pub fn turn_count(&self) -> usize {
        self.dag.len()
    }
}

impl Default for CausalLedger {
    fn default() -> Self {
        Self::new()
    }
}

// ─── CausalTurnBuilder ─────────────────────────────────────────────────────────

/// Helper for constructing causal turns with correct metadata.
pub struct CausalTurnBuilder {
    node_id: [u8; 32],
}

impl CausalTurnBuilder {
    /// Create a builder for a specific node.
    pub fn new(node_id: [u8; 32]) -> Self {
        Self { node_id }
    }

    /// Build a causal turn given the current causal ledger state.
    ///
    /// Automatically fills in:
    /// - causal_deps from the ledger's current frontier
    /// - sequence number from the node's current sequence
    pub fn build(&self, turn: Turn, ledger: &CausalLedger) -> CausalTurn {
        let causal_deps = ledger.frontier();
        let sequence = ledger.next_sequence(&self.node_id);
        CausalTurn::new(turn, causal_deps, self.node_id, sequence)
    }

    /// Build a causal turn with explicit dependencies (for when you want to
    /// depend on specific turns rather than the full frontier).
    pub fn build_with_deps(
        &self,
        turn: Turn,
        deps: Vec<[u8; 32]>,
        sequence: u64,
    ) -> CausalTurn {
        CausalTurn::new(turn, deps, self.node_id, sequence)
    }
}
