# Conceptual Protocol Review: federation/, wire/, net/

Reviewer: Claude (automated review)
Date: 2026-05-20
Scope: Protocol design and distributed systems correctness

---

## 1. BFT Consensus Model

The consensus implementation in `federation/src/consensus.rs` uses a rotating-leader single-round voting protocol with threshold n - f where f = floor((n-1)/3). The threshold arithmetic is correct: for n=4, threshold=3, max_faults=1, satisfying the standard f < n/3 constraint.

**Safety concern:** The protocol does NOT implement a lock/unlock mechanism. In PBFT/HotStuff, a node that votes for a block at height h must not vote for a conflicting block at the same height without a view-change certificate proving the first block will never finalize. Here, `has_voted` is a soft local flag that resets on `advance_view()`. A Byzantine leader could craft two conflicting proposals in rapid succession (before the first QC forms), and honest nodes that receive them in different orders could split votes. The synchronous `ConsensusOrchestrator` masks this by sequencing all message delivery, but in an asynchronous deployment this is a safety violation.

**Liveness assumptions:** Strictly synchronous. The orchestrator tries at most two view changes if the leader is offline, then gives up. There is no timeout-driven view change, no exponential backoff, and no mechanism for nodes to propose view changes independently. Liveness requires a correct leader to be reachable synchronously.

## 2. Revocation Model

Revocation latency equals one consensus round (propose + collect threshold votes + finalize). During that round, a revoked token's non-membership proof remains valid against the previous attested root. There is no epoch-based expiry on attested roots, so a stale root with a valid non-membership proof remains usable indefinitely until the verifier fetches a newer root.

**Maximum exposure window:** Unbounded without a freshness check. The wire protocol checks `federation_root == current_root` (exact match) for token presentations, which helps, but a verifier that cached a root from before the revocation round would still accept the token. The `AttestedRoot` has a `timestamp` field, but no code enforces a maximum age. This needs a configurable staleness bound.

## 3. Causal DAG Ordering (net/)

The DAG in `net/src/causal.rs` provides a clean happened-before partial order. It correctly rejects entries with missing dependencies and produces deterministic topological orderings via Kahn's algorithm. The `merge_frontier()` function computes a sorted hash of frontier entries, enabling state comparison between peers.

**Network partition behavior:** During a partition, each partition's DAG grows independently. Reconnection requires a reconciliation protocol (pull missing entries by hash). The `RequestTurn` message exists for this, but there is no anti-entropy protocol that systematically synchronizes DAGs after partition healing. Nodes in the minority partition will accumulate entries whose deps are locally satisfied but globally incomplete. The DAG itself remains consistent (no conflicts possible due to hash-addressed content), but operational liveness of any consensus or coordination built atop it degrades to the majority partition only.

## 4. Gossip Protocol Reliability

The gossip layer (`net/src/gossip.rs`) implements eager-push: every new message is forwarded to all topic peers immediately. Deduplication prevents infinite rebroadcast.

**Failure probability:** In a fully-connected mesh of n nodes, a single message reaches all nodes with probability 1 (assuming no simultaneous failures). With k failed links, reliability degrades gracefully since every intermediate node re-forwards. However, the implementation has a bounded seen-set (100k entries with FIFO eviction). If a node evicts a message hash and then receives the same message again from a slow peer, it will re-deliver and re-forward, causing duplicates but not message loss.

**Critical gap:** There is no pull-based repair mechanism for gossip. If a node joins a topic after a revocation was gossiped, it will never receive that revocation unless it performs a full state sync via another mechanism. For revocation propagation, this is insufficient -- new or recovering nodes need a catch-up protocol.

## 5. Wire Protocol STARK Verification

The wire protocol binds proofs to the current federation root: `PresentToken` includes `federation_root` and the server rejects presentations with a stale root. The `AuthorizationRequest` includes a 16-byte random nonce and timestamp, preventing naive replay.

**Replay concern:** A man-in-the-middle could capture a valid `PresentToken` frame and replay it to the same server within the same federation epoch. The nonce is generated client-side and is not checked for uniqueness server-side. The server does not maintain a seen-nonce set. A replay would succeed if the federation root has not advanced. Mitigation: add server-side nonce tracking or require a server-issued challenge.

**STARK verification itself:** The `StarkVerifier` delegates to `pyana_circuit::stark::verify`, which runs a full STARK check. The proof is opaque bytes over the wire. No binding between the proof's public inputs and the `AuthorizationRequest` is visible in the wire server -- the server verifies the proof in isolation. If the STARK circuit does not internally commit to the request digest, a valid proof for one resource could be replayed for another.

## 6. Federation Membership Changes

There is no reconfiguration protocol. `ConsensusConfig` takes a fixed `num_nodes` at creation. The `Federation` struct uses a static `Vec<FederationNode>`. Adding or removing nodes requires reconstructing the entire federation, including new threshold calculations and new BLS committee setup (new `UniverseSetup` with hints).

This is a critical missing piece for any production deployment. HotStuff and Tendermint both define explicit reconfiguration transactions embedded in the finalized chain. Without one, the system cannot gracefully handle node failures, planned maintenance, or federation growth.

## 7. Comparison to Existing BFT Systems

| Property | This system | HotStuff | Tendermint | Narwhal |
|----------|------------|----------|------------|---------|
| Message complexity | O(n) per round | O(n) | O(n^2) | O(n^2) mempool |
| Pipelining | No | Yes (3-chain) | No | DAG-based |
| Responsiveness | No (sync) | Yes (optimistic) | No | Yes |
| View change | 2 retries | O(n) msgs | O(n^2) | N/A |
| Reconfiguration | None | Embedded | EndBlock | Epoch-based |

The system trades simplicity for capability. It is closest to a simplified single-shot Tendermint without the propose/prevote/precommit pipeline and without the round-based timeout structure that guarantees liveness under partial synchrony. The constant-size QC via BLS aggregation is a genuine advantage over naive multi-signature approaches.

## 8. Post-Quantum Claims

The `ThresholdQC` uses BLS12-381 via the `hints` crate (KZG commitments + BLS aggregate signatures). BLS12-381 is based on elliptic curve pairings and is broken by Shor's algorithm. The Ed25519 signatures used in the consensus voting layer are similarly vulnerable.

If any component of this system claims "post-quantum security," that claim is false. BLAKE3 hashing and STARK proofs (based on hash functions and FRI) are plausibly post-quantum, but the entire attestation and consensus layer -- the trust anchor for everything else -- is not. A quantum adversary could forge threshold QCs and attested roots.

## 9. Trust Model and Sybil Resistance

The `AllowlistVerifier` in `net/src/node.rs` provides the only Sybil resistance: a node can only connect if its blake3(cert_DER) is pre-authorized in the allowlist. This is a closed-membership model -- someone must have out-of-band knowledge of each node's certificate to add it.

**Who controls the allowlist?** It is a runtime-mutable `Arc<RwLock<HashSet>>`. Any code with a reference to the verifier can call `allow_node()`. There is no governance protocol, no multi-party authorization for membership changes, and no on-chain record of who authorized whom. In the gossip layer, the `GossipCertVerifier` accepts ANY certificate, relying purely on the explicit `join_topic`/`add_peer` call graph for isolation. A compromised node could add arbitrary peers to gossip topics.

**Federation-level Sybil:** Since the consensus threshold is n-f, an attacker who can add nodes (by getting their cert into the allowlist) can dilute the federation until they control > f nodes. There is no stake, no proof-of-work, and no identity binding beyond "someone called `allow_node()`."

---

## Summary of Critical Issues

1. **No safety under asynchrony** -- conflicting votes possible without lock mechanism
2. **Unbounded revocation exposure** -- no enforced staleness bound on attested roots
3. **No post-partition DAG reconciliation** -- missing anti-entropy protocol
4. **Replay vulnerability** -- server-side nonce deduplication absent
5. **No reconfiguration protocol** -- static membership only
6. **BLS12-381 not post-quantum** -- invalidates any PQ claims for the attestation layer
7. **Gossip has no catch-up** -- late-joining nodes miss historical revocations
