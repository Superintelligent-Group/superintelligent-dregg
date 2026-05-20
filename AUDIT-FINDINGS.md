# Audit Findings (2026-05-20)

## Summary

4 audit agents reviewed the full 57k LOC codebase. Results:
- **8 CRITICAL** (security model broken)
- **9 HIGH** (protocol gaps / soundness failures)  
- **12 MEDIUM** (correctness issues / missing integration)
- **8 LOW** (design smells / dead code)

---

## CRITICAL — Security Model Broken

| # | Issue | Location | Fix |
|---|-------|----------|-----|
| 1 | **Turn executor: signatures never verified** — accepts any 64 bytes | turn/src/executor.rs:498-500 | Verify Ed25519 sig against cell.public_key |
| 2 | **Turn executor: ZK proofs never validated** — accepts any bytes | turn/src/executor.rs:527-528 | Verify against cell.verification_key using circuit::stark |
| 3 | **Coordinator commits without verifying vote signatures** | coord/src/atomic.rs:397-410 | Verify each Vote::Yes signature |
| 4 | **Atomic turns bypass all gas metering** (ComputronCosts::zero()) | coord/src/atomic.rs:391 | Pass real costs, enforce budget |
| 5 | **Wire signatures are 32 bytes** (Ed25519 needs 64) — truncation | wire/src/message.rs:114-126 | Fix to [u8;64] |
| 6 | **Wire SubmitRevocation sig is 32 bytes** — unverifiable | wire/src/message.rs:133-137 | Fix to [u8;64] |
| 7 | **Cell ledger apply_delta not truly atomic** — partial commits | cell/src/ledger.rs:228-239 | Clone-and-swap pattern |
| 8 | **Bridge 32→4 byte truncation** destroys collision resistance | bridge/src/present.rs:549 | Use multi-limb BabyBear encoding |

---

## HIGH — Protocol Gaps / Soundness

| # | Issue | Location | Fix |
|---|-------|----------|-----|
| 9 | **Dual Merkle system not reconciled** (BLAKE3 vs Poseidon2) | commit/ vs circuit/ | Unify: Poseidon2 end-to-end for provable path |
| 10 | **Net gossip is one-hop only** — TODO at line 409 | net/src/gossip.rs:409 | Implement re-forwarding |
| 11 | **Net: zero authentication** — SkipCertVerification | net/src/node.rs:412-457 | Pin certs or use node ID allowlist |
| 12 | **Store uses 32-byte sigs, no ThresholdQC** | store/src/federation.rs:17-29 | Fix to [u8;64], add QC field |
| 13 | **Store recovery can't restore consensus state** | store/src/recovery.rs | Add height/view/last_finalized |
| 14 | **Synthetic issuer membership proof** (tautological) | bridge/src/present.rs:518-545 | Query real federation tree |
| 15 | **Net gossip: no delivery guarantee** — no retry/ack/anti-entropy | net/src/gossip.rs | Add pull-based reconciliation |
| 16 | **Turn executor: only first effect's permission checked** | turn/src/executor.rs:574-597 | Check ALL effects' permissions |
| 17 | **Turn executor: Receive permission never checked** | turn/src/executor.rs | Check Receive on destination cell |

---

## MEDIUM — Correctness / Missing Integration

| # | Issue | Location |
|---|-------|----------|
| 18 | Bridge uses MockProof not real STARK in prove() | bridge/src/present.rs:310 |
| 19 | IVC not plumbed through bridge API | bridge/ (missing prove_ivc) |
| 20 | Placeholder body_fact_hashes in derivation witness | bridge/src/present.rs:446 |
| 21 | ThresholdQC never produced during consensus rounds | federation/src/consensus.rs:196 |
| 22 | Wire revocation is standalone (ignores federation) | wire/src/server.rs:193-227 |
| 23 | Wire cannot produce real non-membership proofs | wire/src/server.rs:729 |
| 24 | No shared types crate — 3 incompatible copies | wire/ vs federation/ vs store/ |
| 25 | Net: unbounded seen HashSet (memory leak) | net/src/gossip.rs:67 |
| 26 | Net: dead connections never cleaned up | net/src/gossip.rs:372 |
| 27 | Two diverging CausalDag implementations | net/ vs coord/ |
| 28 | Coordinator: no 2PC timeout | coord/src/atomic.rs |
| 29 | AtomicForest hash doesn't include precondition values | coord/src/atomic.rs:72-83 |

---

## Priority Fix Order

### Phase 1: Make the auth model real (CRITICAL 1-4)
1. Turn executor verifies Ed25519 signatures against cell.public_key
2. Turn executor verifies ZK proofs against cell.verification_key  
3. Coordinator verifies vote signatures before accepting
4. Atomic turns use real ComputronCosts

### Phase 2: Fix wire format (CRITICAL 5-6, HIGH 12)
5. Create shared `pyana-types` crate with canonical Signature([u8;64]), AttestedRoot, etc.
6. Wire and store depend on pyana-types

### Phase 3: Unify Merkle / fix bridge (CRITICAL 8, HIGH 9, 14)
7. Multi-limb BabyBear encoding (4×u32 from 32 bytes, or use full-width field)
8. Decide: Poseidon2 end-to-end for provable path, BLAKE3 for fast non-ZK path
9. Bridge queries real federation Merkle tree for issuer membership

### Phase 4: Network hardening (HIGH 10-11, 15)
10. Gossip re-forwarding + anti-entropy protocol
11. Node authentication (cert pinning or allowlist)

### Phase 5: Integration wiring (MEDIUM 18-24)
12. Wire SiloServer delegates to federation for revocations
13. ThresholdQC produced during consensus, stored, transmitted
14. Bridge exposes IVC path + real STARK path
