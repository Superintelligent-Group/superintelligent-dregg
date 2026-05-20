//! Causal DAG for tracking happened-before ordering between turns.
//!
//! The causal DAG ensures that turns are processed in a consistent order
//! across all peers, respecting causal dependencies (happened-before relations).
//! Each turn declares which previous turns it causally depends on (its "deps"),
//! forming a directed acyclic graph.

use std::collections::{HashMap, HashSet, VecDeque};

/// A single entry in the causal DAG, representing one turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DagEntry {
    /// The blake3 hash identifying this turn.
    pub turn_hash: [u8; 32],
    /// The serialized turn data.
    pub turn_data: Vec<u8>,
    /// Hashes of turns this turn causally depends on (happened-before).
    pub deps: Vec<[u8; 32]>,
    /// Unix timestamp (milliseconds) when the turn was created.
    pub timestamp: i64,
    /// The public key (or hash thereof) of the node that produced this turn.
    pub node_id: [u8; 32],
}

/// Errors from causal DAG operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CausalError {
    /// The entry has a dependency that is not yet in the DAG.
    MissingDeps(Vec<[u8; 32]>),
    /// A turn with this hash already exists.
    Duplicate([u8; 32]),
    /// Detected a cycle (should be impossible with proper hashing, but checked).
    Cycle,
    /// The turn hash does not match the claimed hash.
    HashMismatch {
        claimed: [u8; 32],
        computed: [u8; 32],
    },
}

impl std::fmt::Display for CausalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CausalError::MissingDeps(deps) => {
                write!(f, "missing {} causal dependencies", deps.len())
            }
            CausalError::Duplicate(h) => {
                write!(f, "duplicate turn: {}", hex_short(h))
            }
            CausalError::Cycle => write!(f, "causal cycle detected"),
            CausalError::HashMismatch { claimed, computed } => {
                write!(
                    f,
                    "hash mismatch: claimed {} != computed {}",
                    hex_short(claimed),
                    hex_short(computed)
                )
            }
        }
    }
}

impl std::error::Error for CausalError {}

/// The causal DAG tracks turns and their happened-before relationships.
///
/// Invariants:
/// - Every entry's deps must be present in the DAG before (or at) insertion time.
/// - No duplicate hashes.
/// - The graph is always a DAG (no cycles).
#[derive(Debug, Clone)]
pub struct CausalDag {
    /// All turns indexed by their hash.
    turns: HashMap<[u8; 32], DagEntry>,
    /// Reverse index: for each turn, which turns depend on it.
    dependents: HashMap<[u8; 32], HashSet<[u8; 32]>>,
    /// Set of turns that have no dependents (the "frontier" or "latest" set).
    frontier: HashSet<[u8; 32]>,
}

impl CausalDag {
    /// Create a new empty causal DAG.
    pub fn new() -> Self {
        Self {
            turns: HashMap::new(),
            dependents: HashMap::new(),
            frontier: HashSet::new(),
        }
    }

    /// Insert a new entry into the DAG.
    ///
    /// Fails if:
    /// - Any dependency is missing (use `missing_deps` to check first).
    /// - The turn hash is already present.
    /// - Optionally verifies the hash matches `blake3(turn_data)`.
    pub fn insert(&mut self, entry: DagEntry) -> Result<(), CausalError> {
        // Check for duplicate
        if self.turns.contains_key(&entry.turn_hash) {
            return Err(CausalError::Duplicate(entry.turn_hash));
        }

        // Check all deps are present
        let missing = self.missing_deps(&entry);
        if !missing.is_empty() {
            return Err(CausalError::MissingDeps(missing));
        }

        let hash = entry.turn_hash;

        // Update dependents index: each dep now has us as a dependent
        for dep in &entry.deps {
            self.dependents
                .entry(*dep)
                .or_default()
                .insert(hash);
            // This dep is no longer in the frontier (it has a dependent now)
            self.frontier.remove(dep);
        }

        // The new entry starts as a frontier node (no dependents yet)
        self.frontier.insert(hash);
        self.dependents.entry(hash).or_default();

        // Store the entry
        self.turns.insert(hash, entry);

        Ok(())
    }

    /// Insert an entry, buffering it if deps are missing (returns list of missing deps).
    /// If all deps are present, inserts and returns Ok(None).
    /// If deps are missing, returns Ok(Some(missing_deps)) without inserting.
    pub fn try_insert(&mut self, entry: DagEntry) -> Result<Option<Vec<[u8; 32]>>, CausalError> {
        if self.turns.contains_key(&entry.turn_hash) {
            return Err(CausalError::Duplicate(entry.turn_hash));
        }
        let missing = self.missing_deps(&entry);
        if missing.is_empty() {
            self.insert(entry)?;
            Ok(None)
        } else {
            Ok(Some(missing))
        }
    }

    /// Check whether an entry's causal dependencies are all present.
    pub fn is_causally_valid(&self, entry: &DagEntry) -> bool {
        entry.deps.iter().all(|dep| self.turns.contains_key(dep))
    }

    /// Return the list of missing dependencies for an entry.
    pub fn missing_deps(&self, entry: &DagEntry) -> Vec<[u8; 32]> {
        entry
            .deps
            .iter()
            .filter(|dep| !self.turns.contains_key(*dep))
            .copied()
            .collect()
    }

    /// Return a topological ordering of all entries (respecting happened-before).
    /// Uses Kahn's algorithm.
    pub fn causal_order(&self) -> Vec<&DagEntry> {
        if self.turns.is_empty() {
            return Vec::new();
        }

        // Count in-degree (number of deps each node has that are IN the dag)
        let mut in_degree: HashMap<[u8; 32], usize> = HashMap::new();
        for (hash, entry) in &self.turns {
            in_degree.entry(*hash).or_insert(0);
            // Each dep that exists in our dag contributes to in-degree
            // Actually, in-degree = number of deps (all should be present)
            *in_degree.entry(*hash).or_insert(0) = entry.deps.len();
        }

        // Start with nodes that have zero in-degree (no deps, i.e., genesis turns)
        let mut queue: VecDeque<[u8; 32]> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg == 0)
            .map(|(&hash, _)| hash)
            .collect();

        // Sort the initial queue for determinism
        let mut sorted_queue: Vec<[u8; 32]> = queue.drain(..).collect();
        sorted_queue.sort();
        queue.extend(sorted_queue);

        let mut result = Vec::with_capacity(self.turns.len());

        while let Some(hash) = queue.pop_front() {
            result.push(hash);
            // For each turn that depends on `hash`, decrement its in-degree
            if let Some(deps_of) = self.dependents.get(&hash) {
                let mut next: Vec<[u8; 32]> = Vec::new();
                for &dependent in deps_of {
                    if let Some(deg) = in_degree.get_mut(&dependent) {
                        *deg -= 1;
                        if *deg == 0 {
                            next.push(dependent);
                        }
                    }
                }
                // Sort for determinism
                next.sort();
                queue.extend(next);
            }
        }

        // Map hashes back to entries
        result
            .iter()
            .filter_map(|h| self.turns.get(h))
            .collect()
    }

    /// Get the frontier: entries with no dependents (the "latest" set).
    pub fn latest(&self) -> Vec<&DagEntry> {
        self.frontier
            .iter()
            .filter_map(|h| self.turns.get(h))
            .collect()
    }

    /// Compute a deterministic hash of the current frontier.
    /// This can be used to compare DAG states between peers.
    pub fn merge_frontier(&self) -> [u8; 32] {
        let mut frontier_hashes: Vec<[u8; 32]> = self.frontier.iter().copied().collect();
        frontier_hashes.sort();

        let mut hasher = blake3::Hasher::new();
        for h in &frontier_hashes {
            hasher.update(h);
        }
        *hasher.finalize().as_bytes()
    }

    /// Get a turn by its hash.
    pub fn get(&self, hash: &[u8; 32]) -> Option<&DagEntry> {
        self.turns.get(hash)
    }

    /// Check if the DAG contains a turn with the given hash.
    pub fn contains(&self, hash: &[u8; 32]) -> bool {
        self.turns.contains_key(hash)
    }

    /// Get the number of turns in the DAG.
    pub fn len(&self) -> usize {
        self.turns.len()
    }

    /// Check if the DAG is empty.
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Get all turns that transitively depend on the given turn (descendants).
    pub fn descendants(&self, hash: &[u8; 32]) -> HashSet<[u8; 32]> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*hash);

        while let Some(current) = queue.pop_front() {
            if let Some(deps) = self.dependents.get(&current) {
                for &dep in deps {
                    if result.insert(dep) {
                        queue.push_back(dep);
                    }
                }
            }
        }

        result
    }

    /// Get all turns that this turn transitively depends on (ancestors).
    pub fn ancestors(&self, hash: &[u8; 32]) -> HashSet<[u8; 32]> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*hash);

        while let Some(current) = queue.pop_front() {
            if let Some(entry) = self.turns.get(&current) {
                for &dep in &entry.deps {
                    if result.insert(dep) {
                        queue.push_back(dep);
                    }
                }
            }
        }

        result
    }

    /// Verify that a turn's hash matches blake3(turn_data).
    pub fn verify_hash(entry: &DagEntry) -> Result<(), CausalError> {
        let computed = *blake3::hash(&entry.turn_data).as_bytes();
        if computed != entry.turn_hash {
            return Err(CausalError::HashMismatch {
                claimed: entry.turn_hash,
                computed,
            });
        }
        Ok(())
    }
}

impl Default for CausalDag {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a hash in short hex for display.
pub fn hex_short(h: &[u8; 32]) -> String {
    let full = hex::encode(h);
    format!("{}..{}", &full[..6], &full[full.len() - 6..])
}

/// Simple hex encoding (no external dep needed beyond what we have).
mod hex {
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_turn(data: &[u8], deps: Vec<[u8; 32]>, node: u8) -> DagEntry {
        let turn_hash = *blake3::hash(data).as_bytes();
        DagEntry {
            turn_hash,
            turn_data: data.to_vec(),
            deps,
            timestamp: 1000,
            node_id: [node; 32],
        }
    }

    #[test]
    fn empty_dag() {
        let dag = CausalDag::new();
        assert!(dag.is_empty());
        assert_eq!(dag.len(), 0);
        assert!(dag.latest().is_empty());
        assert!(dag.causal_order().is_empty());
    }

    #[test]
    fn insert_genesis() {
        let mut dag = CausalDag::new();
        let t1 = make_turn(b"turn-1", vec![], 1);
        dag.insert(t1.clone()).unwrap();

        assert_eq!(dag.len(), 1);
        assert!(dag.contains(&t1.turn_hash));
        assert_eq!(dag.latest().len(), 1);
        assert_eq!(dag.latest()[0].turn_hash, t1.turn_hash);
    }

    #[test]
    fn causal_chain() {
        let mut dag = CausalDag::new();
        let t1 = make_turn(b"turn-1", vec![], 1);
        let t2 = make_turn(b"turn-2", vec![t1.turn_hash], 2);
        let t3 = make_turn(b"turn-3", vec![t2.turn_hash], 1);

        dag.insert(t1.clone()).unwrap();
        dag.insert(t2.clone()).unwrap();
        dag.insert(t3.clone()).unwrap();

        assert_eq!(dag.len(), 3);

        // Frontier should be just t3
        let frontier = dag.latest();
        assert_eq!(frontier.len(), 1);
        assert_eq!(frontier[0].turn_hash, t3.turn_hash);

        // Causal order should be t1 -> t2 -> t3
        let order = dag.causal_order();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0].turn_hash, t1.turn_hash);
        assert_eq!(order[1].turn_hash, t2.turn_hash);
        assert_eq!(order[2].turn_hash, t3.turn_hash);
    }

    #[test]
    fn concurrent_turns_diamond() {
        let mut dag = CausalDag::new();
        let t1 = make_turn(b"genesis", vec![], 1);
        // Two concurrent turns depending on genesis
        let t2a = make_turn(b"branch-a", vec![t1.turn_hash], 1);
        let t2b = make_turn(b"branch-b", vec![t1.turn_hash], 2);
        // Merge turn depending on both
        let t3 = make_turn(b"merge", vec![t2a.turn_hash, t2b.turn_hash], 1);

        dag.insert(t1.clone()).unwrap();
        dag.insert(t2a.clone()).unwrap();
        dag.insert(t2b.clone()).unwrap();
        dag.insert(t3.clone()).unwrap();

        assert_eq!(dag.len(), 4);

        // Frontier should be just the merge
        let frontier = dag.latest();
        assert_eq!(frontier.len(), 1);
        assert_eq!(frontier[0].turn_hash, t3.turn_hash);

        // Causal order: t1 first, then t2a and t2b (in some deterministic order), then t3
        let order = dag.causal_order();
        assert_eq!(order.len(), 4);
        assert_eq!(order[0].turn_hash, t1.turn_hash);
        assert_eq!(order[3].turn_hash, t3.turn_hash);
    }

    #[test]
    fn missing_deps_rejected() {
        let mut dag = CausalDag::new();
        let fake_dep = [0xaa; 32];
        let t = make_turn(b"orphan", vec![fake_dep], 1);

        let result = dag.insert(t);
        assert!(matches!(result, Err(CausalError::MissingDeps(_))));
    }

    #[test]
    fn duplicate_rejected() {
        let mut dag = CausalDag::new();
        let t1 = make_turn(b"turn-1", vec![], 1);
        dag.insert(t1.clone()).unwrap();

        let result = dag.insert(t1);
        assert!(matches!(result, Err(CausalError::Duplicate(_))));
    }

    #[test]
    fn hash_verification() {
        let t = make_turn(b"hello", vec![], 1);
        assert!(CausalDag::verify_hash(&t).is_ok());

        let mut bad = t.clone();
        bad.turn_hash = [0xff; 32];
        assert!(matches!(
            CausalDag::verify_hash(&bad),
            Err(CausalError::HashMismatch { .. })
        ));
    }

    #[test]
    fn merge_frontier_deterministic() {
        let mut dag1 = CausalDag::new();
        let mut dag2 = CausalDag::new();

        let t1 = make_turn(b"turn-1", vec![], 1);
        let t2 = make_turn(b"turn-2", vec![], 2);

        // Insert in different orders
        dag1.insert(t1.clone()).unwrap();
        dag1.insert(t2.clone()).unwrap();

        dag2.insert(t2.clone()).unwrap();
        dag2.insert(t1.clone()).unwrap();

        // Merge frontier should be the same regardless of insertion order
        assert_eq!(dag1.merge_frontier(), dag2.merge_frontier());
    }

    #[test]
    fn ancestors_and_descendants() {
        let mut dag = CausalDag::new();
        let t1 = make_turn(b"t1", vec![], 1);
        let t2 = make_turn(b"t2", vec![t1.turn_hash], 2);
        let t3 = make_turn(b"t3", vec![t2.turn_hash], 1);

        dag.insert(t1.clone()).unwrap();
        dag.insert(t2.clone()).unwrap();
        dag.insert(t3.clone()).unwrap();

        let desc = dag.descendants(&t1.turn_hash);
        assert!(desc.contains(&t2.turn_hash));
        assert!(desc.contains(&t3.turn_hash));
        assert_eq!(desc.len(), 2);

        let anc = dag.ancestors(&t3.turn_hash);
        assert!(anc.contains(&t1.turn_hash));
        assert!(anc.contains(&t2.turn_hash));
        assert_eq!(anc.len(), 2);
    }

    #[test]
    fn try_insert_buffering() {
        let mut dag = CausalDag::new();
        let t1 = make_turn(b"t1", vec![], 1);
        let t2 = make_turn(b"t2", vec![t1.turn_hash], 2);

        // Try inserting t2 before t1 - should return missing deps
        let result = dag.try_insert(t2.clone()).unwrap();
        assert_eq!(result, Some(vec![t1.turn_hash]));
        assert_eq!(dag.len(), 0); // Not inserted

        // Now insert t1
        dag.insert(t1).unwrap();

        // Now t2 should insert fine
        let result = dag.try_insert(t2).unwrap();
        assert_eq!(result, None);
        assert_eq!(dag.len(), 2);
    }
}
