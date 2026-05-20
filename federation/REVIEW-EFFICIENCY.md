# Efficiency Review: federation/, wire/, net/

Performance, scalability, and resource analysis of the pyana federation stack.

---

## 1. BFT Consensus Message Complexity

The consensus uses a rotating-leader protocol (Morpheus-shaped). Per round:
- 1 Propose broadcast: O(N) messages
- N-1 Vote unicasts to leader: O(N) messages
- 1 Finalize broadcast: O(N) messages

**Total: O(N) messages per round.** This is optimal for single-leader BFT (compare Tendermint's O(N^2) all-to-all voting). View changes add O(N) per failed leader. Dominant latency is 2 network RTTs (propose + vote collection + finalize broadcast), approximately 3-10ms on LAN, 100-300ms on WAN.

At N=7 (max for Ethereum KZG setup): 19 messages per round. At N=63 (max domain size): ~189 messages per round. The protocol is strongly leader-bottlenecked -- the leader must process all votes sequentially (linear scan for duplicates in `collect_vote`).

## 2. ThresholdQC (hints/BLS12-381)

The `hints` crate uses BLS12-381 + KZG polynomial commitments with SNARK-assisted aggregation. Estimated costs:

- **Signing (per share):** BLS pairing computation, ~1-2ms per partial signature
- **Aggregation:** KZG opening + SNARK proof generation. For N=7, expect ~50-200ms (dominated by the SNARK proving step)
- **Verification:** Single pairing check + SNARK verification, ~3-5ms regardless of committee size

Compared to N individual Ed25519 verifications (each ~60us): at N=7, Ed25519 costs ~420us total. ThresholdQC verification is ~10x slower per-check, but the QC is constant-size (~288 bytes compressed) vs N*64 bytes for individual signatures. The tradeoff favors ThresholdQC when proofs are stored/transmitted many times (amortized bandwidth savings).

## 3. Wire Protocol Overhead

Framing: 4-byte LE length prefix per message. Postcard encoding overhead is minimal -- varint-encoded enum discriminants, no schema metadata, no field tags. For a typical `PresentToken` message with a 24 KiB STARK proof:

- Postcard overhead: ~10-20 bytes (enum tag + varint lengths + AuthorizationRequest fields)
- Framing overhead: 4 bytes
- **Total overhead: <0.1% of payload**

Compared to Protobuf: Protobuf would add ~50-100 bytes of field tags/wire types for the same message. Cap'n Proto has zero-copy reads but alignment padding adds 10-20% size overhead on variable-length data. Postcard is the right choice here -- minimal overhead, fast decode, no allocation for fixed-size fields.

The 16 MiB max message size is reasonable; no fragmentation or streaming needed for the expected payloads.

## 4. QUIC Connection Cost

Each QUIC connection requires:
- **TLS 1.3 handshake:** 1-RTT (ECDSA P-256 cert generation + signature verification). Self-signed certs avoid CA chain validation. ~2-5ms on LAN.
- **Cert generation:** `rcgen` generates a fresh ECDSA-P256 cert per node startup. This is a one-time cost (~1ms).
- **Cert verification:** The `GossipCertVerifier` accepts ANY cert (zero verification cost for gossip). The `AllowlistVerifier` does blake3 hash + HashSet lookup (~200ns). Neither verifies the actual signature chain -- this is intentional but means TLS only provides encryption, not authentication at the TLS layer.

**Connection pooling:** Partially implemented. `GossipNetwork.state.peers` maintains a `HashMap<SocketAddr, Connection>` cache. Connections are reused for gossip forwarding. However, the `wire` crate's `SiloServer.present_token()` opens a NEW TCP connection for every request (no pooling). This is the critical gap -- each cross-silo token presentation pays full TCP+TLS handshake cost (~5-10ms).

## 5. Gossip Scalability

The gossip layer uses **eager-push** (flood): every message is forwarded to ALL topic peers. With N nodes and M messages/second:

- **Bandwidth amplification factor: N-1** (each message is sent to every peer)
- **Total network bandwidth: M * (N-1) * message_size per node**
- **Deduplication:** Bounded seen-set (100k entries, ~3.2 MB). At 1000 msg/s, this covers 100 seconds of history before eviction.

For N=10, M=100 msg/s, 1 KiB messages: each node sends 900 KiB/s. For N=100: 9.9 MB/s per node. This does not scale beyond ~50 nodes without switching to lazy-pull gossip (e.g., PlumTree/epidemic broadcast trees). The current design is appropriate for small federations (4-20 nodes).

## 6. Revocation Tree Operations

The tree is a 4-ary sparse Merkle tree with depth 16 (4^16 = ~4 billion slots), backed by a BTreeMap:

- **Insert:** O(log K) BTreeMap insert + root recomputation. Root recomputation walks 16 levels, hashing 4 children at each level = 16 blake3 hashes. At K=100k: ~insert takes <10us (BTreeMap insert) + ~5us (16 hashes). Total: ~15us.
- **Non-membership proof:** O(log K) BTreeMap range query to find neighbors + 2 membership proofs (each 16 levels of sibling computation). Total: ~30-50us.
- **Proof size:** Each membership proof contains 16 * 3 * 32 = 1,536 bytes of siblings + 16 bytes of indices. Non-membership proof: up to 2 * 1,536 + 32 = ~3.1 KiB.

These costs are negligible compared to network RTT and STARK verification.

## 7. STARK Verification

The STARK system uses BabyBear field (p = 2^31 - 1), O(n^2) Lagrange interpolation (acceptable for small traces of 4-16 rows), and BLAKE3 Merkle commitments. For a ~32 KiB proof with 16 queries:

- Each query: 3 Merkle path verifications (trace + constraint + next-row) at ~log2(domain_size) hashes each
- FRI layer verification: additional hash checks per folding layer
- **Estimated verification time: 0.5-2ms** for a 32 KiB proof (dominated by ~100-200 blake3 hashes)

For per-request verification: at 2ms per verification, a single core handles ~500 verifications/second. This is the tightest bottleneck in the system. Multi-core parallelism (one tokio task per request) scales linearly with cores.

## 8. Memory Per Federation Node

Per node:
- `RevocationTree`: BTreeMap with K entries * 36 bytes (u32 key + [u8;32] value) + HashSet<String> for quick lookup. At K=100k revocations: ~7 MB.
- `ConsensusState`: Fixed size, ~1 KB (pending events + collected votes, cleared each round).
- `finalized_blocks` history: Grows unbounded. Each block is ~200 bytes + events. At 1 block/second for a day: ~17 MB. **No pruning is implemented.**
- `GossipNetwork.seen`: 100k * 32 bytes = 3.2 MB (bounded).
- QUIC connections: ~50 KB per active connection (quinn buffers).

**Total at steady state with K=100k revocations, 10 peers:** ~15-25 MB. Growth concern: `finalized_blocks` and `revoked_tokens` (Vec<String> in SiloState) grow unboundedly.

## 9. Network Partition Recovery

When a partitioned node rejoins:
- **No sync protocol exists.** The `recover_node` function simply sets `is_online = true`. The node's `last_finalized_hash` will mismatch the chain, causing all future proposals to be rejected by `validate_block` (prev_hash check fails).
- **Required fix:** A rejoining node must receive all blocks it missed (from `last_finalized_hash` to current tip), apply them in order, and update its revocation tree. This is a gap in the current implementation.
- **Data volume:** Each missed block is ~200 bytes + events. Missing 1000 blocks = ~200 KB. Manageable, but the machinery to fetch and replay them does not exist.

## 10. Can This Handle 10k Requests/Second?

**Bottleneck analysis for 10k req/s of cross-silo token presentations:**

1. **STARK verification:** At ~2ms each, need 20 cores dedicated to verification. Achievable on modern hardware.
2. **TCP connections (wire crate):** No connection pooling for `present_token`. At 10k/s with 5ms handshake each: need 50 concurrent connection establishments. This saturates easily but is fixable.
3. **Consensus throughput:** At 1 block/100ms with 100 revocations/block = 1000 revocations/second max. Consensus is not on the hot path for reads (presentations), only for writes (revocations).
4. **Gossip bandwidth:** At 10k messages/s * 10 peers * 1 KiB = 100 MB/s per node. Exceeds reasonable bandwidth. Gossip is not needed for presentations (they are point-to-point).

**Verdict:** 10k token presentations/second is achievable with: (a) connection pooling in the wire layer, (b) multi-core STARK verification, (c) separating the gossip layer from the presentation hot path. The current code cannot do it without these fixes, primarily due to the missing connection pool and single-threaded verification path.
