# Efficiency Review: pyana-audit and pyana-store

## 1. Audit Log Append Cost

Each `append()` is **O(N)** in the worst case, not O(1) amortized. After pushing the leaf (O(1) amortized for the Vec), the code invalidates `cached_root` and immediately calls `self.root()`, which triggers `compute_root()`. This recursively walks the entire 4-ary Merkle tree via `compute_subtree_hash()` -- depth 12, branching factor 4 -- visiting every populated node. For N leaves, root computation is O(N) because it visits all populated subtrees without caching intermediate nodes.

The `prove_inclusion_at()` call within append is also O(N) due to `compute_siblings()`, which calls `compute_subtree_hash()` for each sibling at each tree level.

Combined: each append does two full tree traversals. At 1M events this is roughly 2M hash computations per append.

## 2. Count Proof Generation

`prove_count()` iterates over all K indices for the target token and generates an inclusion proof for each. Each inclusion proof is O(N) (see above). Total complexity: **O(K * N)**.

This is proportional only to the token's own event count (K), not all N events, for the loop -- but each inclusion proof internally scans the full tree. For a token with K=100 uses in a log of N=1M, that is 100 full tree traversals.

## 3. Range Queries in Store (redb)

`audit_events_for_token()` uses a secondary index (`AUDIT_TOKEN_INDEX`) with a string prefix range scan. The key format `"{hex(token_id)}:{sequence:020}"` enables redb's B-tree range iteration. This is **O(K * log N)** where K is the result count -- the range scan is O(log N) to seek plus O(K) to iterate. Each result requires a second lookup in `AUDIT_LOG` by sequence number (O(log N) each), making the total O(K * log N). This is well-indexed.

`audit_events_range()` is O(M * log N) for M results in the sequence range. No secondary index needed; the primary key is the sequence number.

## 4. Memory Usage

The in-memory `AuditLog` holds:
- `events: Vec<UsageEvent>` -- each event is 112 bytes (32+8+32+32+8). At 1M events: ~107 MB.
- `leaves: Vec<[u8; 32]>` -- 32 bytes each. At 1M: ~30 MB.
- `event_index: HashMap<[u8; 32], Vec<u64>>` -- one entry per unique event hash, overhead ~100 bytes each.
- `token_events: HashMap<[u8; 32], Vec<u64>>` -- one entry per token, plus 8 bytes per event index.
- `historical_roots: Vec<[u8; 32]>` -- 32 bytes per append. At 1M: ~30 MB.

Total at 1M events: approximately **200-250 MB**. This is entirely unbounded -- there is no eviction, compaction, or streaming. The log grows monotonically. At 10M events you are looking at 2+ GB of resident memory for the audit subsystem alone.

## 5. Merkle Tree Updates

**Full rebuild on every access.** There is no incremental update. The tree is not stored as a persistent data structure -- only the leaf array exists. Every call to `root()` when the cache is invalid recomputes from scratch via `compute_subtree_hash()`. There is no memoization of interior nodes.

The `cached_root` field provides a single-value cache that is invalidated on every append. Since `append()` immediately calls `root()`, the cache is rebuilt once per append. But subsequent proof generation (which also calls `compute_siblings`) does another full traversal because sibling hashes are not cached either.

## 6. redb Transaction Overhead

Each `append_audit_event()` opens a write transaction, opens 3 tables (METADATA, AUDIT_LOG, AUDIT_TOKEN_INDEX), performs 4 operations (read seq, write event, write index, write seq+1), then commits. redb uses WAL, so each commit is at least 1 fsync. This means **1 fsync per event** in the non-batched path.

The `append_audit_events_batch()` method amortizes this: one transaction, one commit (one fsync) for N events. This is the correct pattern for throughput but callers must opt into it.

## 7. Projected Performance at 10k Events/Second

At 10k events/sec with the current in-memory `AuditLog`:
- Each append does a full tree recomputation: ~1M hash operations at scale. Blake3 hashes at ~1 GB/s, but with 1M 32-byte nodes that is ~30ms per root computation. At 10k/sec you need appends under 100us each -- **the Merkle rebuild alone will saturate the CPU within minutes of operation** as N grows past ~10k leaves.
- Memory will grow at ~250 bytes/event = 2.5 MB/sec, reaching 1 GB in ~7 minutes.
- For `pyana-store`: redb can handle ~10k commits/sec with batching (one fsync per batch). Without batching, you are limited by disk fsync latency (typically 100-500 per second on SSD), making the non-batched path cap at ~200-500 events/sec.

## Recommendations

1. **Cache interior Merkle nodes.** Store a `Vec<[u8; 32]>` for each tree level. On append, only recompute the path from new leaf to root -- O(log N) instead of O(N).
2. **Separate proof generation from append.** Do not compute inclusion proofs eagerly in `append()` unless the caller needs the receipt immediately.
3. **Cap or shard the in-memory log.** Either use a sliding window over the persistent store, or implement memory-mapped access to the redb-backed events.
4. **Always batch in the store layer.** The single-event `append_audit_event()` should internally buffer and flush, or the API should strongly guide callers toward batch mode.
5. **Pre-compute sibling hashes.** Store them alongside the tree nodes so proof generation is O(depth) = O(12) instead of O(N).
