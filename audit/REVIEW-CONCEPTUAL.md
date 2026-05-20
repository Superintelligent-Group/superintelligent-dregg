# Conceptual Review: pyana-audit and pyana-store

Reviewer: Claude (structural/conceptual review, 2026-05-20)

## 1. Append-Only Guarantees (audit/)

The in-memory `AuditLog` is structurally append-only: events are pushed to a `Vec<UsageEvent>`, positions are assigned by `events.len()`, and there is no `remove` or `replace` method exposed. However, there is **no cryptographic enforcement** preventing the operator from reconstructing the log from scratch with entries omitted. The Merkle tree is recomputed from the current leaf set on every root query---it has no chaining (each leaf does not commit to the previous root). A dishonest operator could drop intermediate events and regenerate a valid tree with a different root. The only detection mechanism is external: a token holder who retained a receipt can compare their stored root against the operator's claimed historical root. If the operator garbage-collects `historical_roots`, there is no recourse. This is weaker than a hash-chain (like Certificate Transparency) where truncation is detectable by any party holding a signed tree head.

## 2. Budget Enforcement and Concurrency

`BudgetEnforcer` is a single-threaded struct (`&mut self` on `record_use`). It is not `Sync`, and there is no internal locking. In a concurrent deployment (e.g., multiple async tasks sharing the enforcer via `Arc<Mutex<_>>`), correctness depends entirely on the caller holding the lock across the check-and-append sequence. The current design is correct for single-threaded use, but the code does not document this constraint. A distributed deployment sharing budget state across nodes would require a consensus round to prevent races---nothing in the current architecture addresses this.

## 3. Consistency Proofs

`ConsistencyProof::verify_structure()` only checks that `new_size >= old_size` and that bridge hashes have plausible depth/position values. It does **not** reconstruct the old root from the bridge hashes + prefix leaves. This means the proof is structurally well-formed but not cryptographically convincing to an external verifier who lacks the leaf data. A full consistency proof (RFC 6962 style) should let a verifier with only the two roots and the bridge hashes confirm that the old tree is a prefix. As written, the proof is only verifiable by a party that trusts the prover or has the full log.

## 4. Count Proof Privacy

The `CountProof` includes `event_proofs: Vec<(u64, [u8; 32], InclusionProof)>`. Each tuple contains the **global index** of the event and its leaf hash. An auditor receiving this proof learns exactly which positions in the global log belong to the queried token. If the auditor also has access to the leaves at those positions (or can observe timing/ordering), the proof leaks the event schedule. The `index_commitment` hides nothing from the auditor---it merely provides a binding for the prover. This is not privacy-preserving in the ZK sense: a proper solution would use a zero-knowledge set membership proof or a Pedersen commitment scheme that hides the indices entirely.

## 5. Key Encryption Scheme (store/keys.rs)

The scheme is BLAKE3-derive-key used as a stream cipher (XOR) plus a separate BLAKE3-keyed MAC. This is sound given BLAKE3's XOF properties:

- **Nonce uniqueness**: Each encryption generates a fresh 16-byte random nonce via `getrandom`. Collision probability is ~2^{-64} after 2^32 encryptions---adequate for 32-byte keys stored in a single DB.
- **Key reuse**: Same master key + different nonce yields independent keystreams. Correct.
- **MAC**: Encrypt-then-MAC with separate domain separation (`key-enc v1` vs `key-mac v1`). Authentication is verified before decryption. Constant-time comparison is implemented manually (correct).
- **Limitation**: The scheme is fixed to 32-byte plaintexts and uses only 32 bytes of BLAKE3 output. It would not generalize to variable-length secrets without redesign. The comment acknowledges this is a simplified stand-in for XChaCha20-Poly1305.

One concern: `derive_keystream` hashes `master_key || nonce`, not `nonce || master_key`. With BLAKE3's Merkle-Damgard-like compression, this is fine (no length-extension), but the conceptual cleaner pattern is to use the key as the keyed-hash key rather than as input data. Currently the code uses `new_derive_key` (context string) then feeds master+nonce as data. This is safe per BLAKE3's spec but is an unusual usage---the master key is not being used as a BLAKE3 key parameter.

## 6. Crash Recovery Correctness (store/)

The store delegates durability to redb's write-ahead log. Each mutation is wrapped in `begin_write()` ... `commit()`. State that can be lost between the last successful `commit()` and a crash:

- Any write transaction that was begun but not yet committed is rolled back. This includes both the audit event and its sequence counter increment, which live in the same transaction---so partial updates (event without counter, or counter without event) cannot occur.
- The `append_fold_step` method performs a load-modify-store pattern in two separate transactions (read in `load_token_chain`, write in `store_token_chain`). If the process crashes between the load and the write, no data is lost (the step was never committed). If two threads attempt concurrent appends to the same chain, both could read the same `current_root`, and the second committer would succeed with a stale base---the continuity check only runs in-process, not at the DB constraint level.

## 7. redb Suitability

redb is a reasonable choice for this workload: embedded, single-writer ACID, MVCC for concurrent readers, and zero-copy reads. Considerations:

- **Write amplification**: redb uses a copy-on-write B-tree. For append-heavy workloads (audit log), leaf pages will fill sequentially, yielding ~1x amplification. Random updates (e.g., metadata counter bumps) trigger page copies but at negligible volume.
- **Compaction**: redb does not do background compaction; the `compact()` method must be called manually. For a growing audit log that is never deleted, fragmentation is minimal. The revocation table (write-once per token) is similarly benign.
- **Concurrent readers**: redb supports multiple concurrent read transactions via MVCC snapshots. This is fine for the read-heavy query patterns (loading chains, checking revocations). The single-writer constraint means all writes serialize, which matches the append-only audit semantics but could bottleneck bulk ingestion.
- **Scalability ceiling**: The 16M-leaf Merkle tree in audit/ is in-memory only; the persistent audit in store/ is flat (keyed by sequence number) with no Merkle commitment. These are separate systems that are not yet integrated---persistence of the Merkle state would require serializing the tree or replaying all events on startup.
