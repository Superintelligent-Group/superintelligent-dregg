# Efficiency & Implementation Review: circuit/, commit/, trace/

## 1. Poseidon2 Permutation: Multiplication Cost

The Poseidon2 in `circuit/src/poseidon2.rs` uses width=8, alpha=7, 8 external rounds, 22 internal rounds.

**Per permutation:**
- S-box (x^7): computed via `pow(7)` which is 4 multiplications per element (squaring chain: x->x^2->x^4->x^3=x^2*x->x^7=x^4*x^3). External rounds apply 8 S-boxes = 32 muls. Internal rounds apply 1 S-box = 4 muls.
- External linear layer: 3 butterfly stages (12 adds) + 8 scalar multiplications = 8 muls.
- Internal linear layer: 1 sum (7 adds) + 8 multiply-accumulates = 8 muls.
- External rounds total: 4*(32+8) = 160 muls. Internal rounds total: 22*(4+8) = 264 muls.
- **Grand total: ~424 field multiplications per permutation.**

This is within the expected range for Poseidon2 over BabyBear. However, `round_constants()` and `internal_diag()` are recomputed from scratch (30 BLAKE3 hashes each!) on EVERY call to `permute()`. This is the single largest performance bug: these should be `LazyLock` statics.

**SIMD potential:** The butterfly-structured external linear layer is trivially vectorizable (4 independent pairs, then 2 independent quads). On ARM NEON or x86 AVX2, 8-wide BabyBear state fits in 1-2 256-bit registers. The S-box is per-element and parallelizes perfectly. Expected speedup: 3-5x with proper SIMD.

## 2. STARK Proof Generation: Dominant Costs

For a 4-row trace (typical: 4-level Merkle path), domain_size = 16:

1. **Lagrange interpolation: O(n^2) per column.** 6 columns, each interpolated in O(n^2) = O(16). This is fine for n<=16 but does NOT scale. At n=64, this is already 24K muls. NTT is impossible here because p=2^31-1 is Mersenne, not NTT-friendly. This is a critical architectural problem.
2. **Polynomial evaluation on extended domain: O(n * domain_size) per column.** 6 * 4 * 16 = 384 evaluations via Horner.
3. **Constraint evaluation: O(domain_size * trace_len)** for vanishing polynomial Z(x). The inner loop at line 525 computes Z(x) = prod(x - tp) for all trace points, costing O(trace_len) per domain point. Total: O(domain_size * trace_len) = O(n^2 * BLOWUP).
4. **Merkle tree building: O(domain_size * BLAKE3)** - 16 leaves plus 15 internal nodes.
5. **FRI folding: O(domain_size * log(domain_size))** - halving steps with BLAKE3 leaf hashing.

**Dominant cost for n=4:** The BLAKE3 Merkle trees (50 queries, each needing a proof with log2(16)=4 hashes) dominate wall-clock time. For n=64+, interpolation becomes the bottleneck.

**Parallelism:** Column interpolation is embarrassingly parallel. FRI folding is sequential. Merkle leaf hashing is parallelizable. No parallelism is currently exploited.

## 3. STARK Proof Size Scaling

With BLOWUP=4, 50 queries, and a binary Merkle tree of depth log2(4n):
- Each query: 6 trace values (24B) + Merkle path (log2(4n) * 32B) + next-row values/path + constraint value/path + FRI paths.
- For 4-row trace (depth-4 tree): each query ~= 24 + 128 + 24 + 128 + 4 + 128 + FRI ~= 600-800B.
- 50 queries ~= 30-40 KiB total proof.
- Scaling: O(NUM_QUERIES * log(n) * 32) bytes per query for Merkle paths. Proof grows logarithmically with trace length.

**Is 4x blowup optimal?** For 100-bit security with 50 queries, the standard formula is security = NUM_QUERIES * log2(1/rho) where rho = 1/BLOWUP. So 50 * log2(4) = 100 bits. This is correctly matched. Reducing to 2x blowup would require 100 queries (doubling proof size) or halve security.

## 4. Merkle Tree in commit/: 4-ary Analysis

The tree is sparse (BTreeMap-backed), depth=16, 4-ary branching.

- **Insert/Remove:** O(depth * 4) = O(64) hash operations in the worst case (recomputes all 4 children at each level via `compute_subtree_hash`). Actually worse: each level calls `has_leaves_in_range` (O(log N) BTreeMap lookup) per child, so total is O(depth * 4 * log N). For N=50 leaves: ~16 * 4 * 6 = ~384 BTreeMap lookups per root computation.
- **Root computation invalidates on every mutation** and is recomputed fully from scratch. No incremental update. This is O(N * depth) in the worst case for a dense tree.
- **Membership proof:** O(depth * 4) subtree hash computations for siblings. Each sibling requires recursing into its subtree if leaves exist there.
- **4-ary vs binary:** Proof size is 3 * 16 = 48 sibling hashes (48 * 32 = 1536 bytes) vs binary 1 * 32 = 32 sibling hashes (1024 bytes). 4-ary is 50% larger proofs but requires fewer levels (16 vs 32). For ZK circuits, 4-ary is better because it reduces the number of hash evaluations inside the circuit (16 vs 32 Poseidon2 calls per membership proof). Good choice for this use case.
- **Missing optimization:** `empty_hash_at_depth()` is recomputed from scratch every time (O(depth) hashes). Should be cached or precomputed in a const table.

## 5. Trace Evaluator: Complexity of Bottom-Up Evaluation

The evaluator in `trace/src/eval.rs` runs a naive semi-naive Datalog fixpoint:

- **Per round:** For each rule, find all substitutions by iterating all facts for each body atom. With R rules, B body atoms per rule, and F facts: O(R * F^B) per round.
- **Fixpoint termination:** Guaranteed because the Herbrand base is finite (facts are ground, no function symbols). Each round either derives at least one new fact or terminates. Maximum rounds = size of Herbrand base. For 50 facts and 4 rules with 2-body atoms: worst case ~50^2 = 2500 candidate substitutions per round, with at most ~100-200 derivable facts total.
- **Infinite derivation is impossible** by Datalog semantics (no negation, no function symbols, finite base).
- **Duplicate detection** uses linear scan (`facts.contains(&derived_fact)`) which is O(F) per check. A HashSet would be O(1).

## 6. FoldDelta Verification: Scope of Recomputation

`FoldDelta::verify()` in `commit/src/fold.rs` does NOT recompute the tree. It:
1. Verifies each removed fact's Merkle proof against `old_root` (O(depth * 4) hashes per fact).
2. Checks structural consistency (roots match).
3. Does NOT independently verify the survival witness beyond root equality checks.

The `reconstruct_new_state()` method DOES rebuild from scratch: clones all facts, removes, reinserts. This is O(N * depth) but is only used for testing/recovery, not verification.

## 7. Field Arithmetic: BabyBear Reduction

`BabyBear::mul` at line 198 uses **direct modular reduction**: `(a as u64 * b as u64) % P`. This computes a 64-bit product then uses the hardware `%` operator. On modern CPUs, `u64 % u32` compiles to a single `div` instruction (or `udiv` on ARM). This is approximately 3-5 cycles.

**Montgomery form would save nothing here** because BabyBear's modulus is a Mersenne prime (2^31-1), which allows a faster reduction: `let t = prod; let r = (t & P as u64) + (t >> 31); if r >= P { r - P }`. This would eliminate the division entirely. The current implementation leaves ~2-3x performance on the table for field arithmetic.

**Inversion:** Uses Fermat's little theorem (a^(p-2) mod p) via square-and-multiply. This takes 30 multiplications (for a 31-bit exponent). Adequate for occasional use; would be a problem if called in a hot loop (it is not currently).

## 8. Memory Allocation Patterns

Major issues:
- **`round_constants()` allocates a Vec<[BabyBear; 8]> every permutation call.** 30 iterations of format!+BLAKE3+allocation. Critical path; called thousands of times during proof generation.
- **`Substitution::extend()` clones the entire bindings Vec for every variable binding.** During evaluation with R rules and F facts, this produces O(R * F^B * B) clone operations. Should use persistent data structures or arena allocation.
- **`derive_one_round` clones `indices` vectors** for every candidate substitution. With 50 facts and 2-body rules, that's up to 2500 Vec clones per round.
- **`trace_evals` in stark.rs** allocates separate Vecs for each column evaluation. Could use a single contiguous matrix.
- **`find_unchanged_subtrees` recursively allocates** Vec<SubtreeRef> at each level and extends. Arena allocation would help.

## 9. Expected Proof Generation Time (5 attenuation steps, 50 facts, 4 rules)

Estimated breakdown for the realistic workload:
- Trace evaluation (Datalog fixpoint): 50 facts, 4 rules, 2-body atoms. ~5 rounds, ~10K substitution attempts. ~1ms.
- Merkle operations (5 fold steps, each removing 1-3 facts): 5 * 3 * 16 * 4 BLAKE3 hashes for proofs. ~5 * 200 = 1000 BLAKE3 calls. ~0.5ms.
- IVC proof generation (5 fold AIRs + IVC AIR): MockProver verification is pure constraint evaluation, negligible. But with real STARK: 5 trace interpolations (O(n^2) each for small n) + 5 FRI proofs + Merkle tree builds. Estimated: ~5-20ms with the current O(n^2) interpolation on small traces.
- Poseidon2 hashing (hash chain for IVC): 5 * 2 permutations = 10 permutations. With the round_constants() bug: 10 * 30 * BLAKE3 = 300 BLAKE3 calls wasted. ~0.1ms (dominated by the BLAKE3 waste).

**Total estimate: 10-30ms** on a modern machine (M-series or Zen4), dominated by STARK polynomial arithmetic and Merkle tree BLAKE3 hashing.

## 10. Comparison with Published Benchmarks

| System | Hash/Trace | 2^16 row proof | Notes |
|--------|-----------|---------------|-------|
| Plonky3 (BabyBear) | Poseidon2 | ~100ms | NTT-friendly field, vectorized |
| Miden (Goldilocks) | RPO | ~200ms | 64-bit field |
| RISC Zero (BabyBear) | Poseidon2 | ~500ms | Full RISC-V VM overhead |
| **This impl (4-row)** | Poseidon2+BLAKE3 | **~20ms est.** | Tiny traces, O(n^2) interp |

The comparison is misleading: this system operates on traces of 2-16 rows (authorization proofs), not 2^16+ rows (general computation). For its target workload, performance is adequate. However:

1. **The Mersenne prime choice (p=2^31-1) is architecturally wrong for STARKs.** It cannot support NTT, forcing O(n^2) interpolation. Real Plonky3 uses p=2^31-2^27+1 which has a 2^27-sized multiplicative subgroup for NTT. If traces ever grow beyond ~64 rows, this becomes untenable.
2. **No SIMD exploitation.** Plonky3 achieves its speed through AVX-512 vectorized BabyBear arithmetic (8-wide). This implementation is entirely scalar.
3. **For the target use case (small authorization proofs):** the O(n^2) approach is fine. Traces will never exceed ~32 rows in practice (max Merkle depth 16 + derivation steps).

## Summary of Actionable Improvements

1. **Critical:** Cache `round_constants()` and `internal_diag()` in `LazyLock` statics. ~50x Poseidon2 speedup.
2. **Critical:** Use Mersenne-specific reduction (`(t & P) + (t >> 31)`) instead of `%` operator. ~2x field arithmetic speedup.
3. **High:** Replace `Vec<(Variable, Term)>` substitution with a small fixed-size array or arena-backed structure.
4. **High:** Use `HashSet` for duplicate fact detection in evaluator instead of linear scan.
5. **Medium:** Precompute `empty_hash_at_depth()` table (16 entries).
6. **Medium:** Consider switching to real BabyBear (p=2^31-2^27+1) if traces may grow, enabling NTT.
7. **Low:** SIMD-vectorize the Poseidon2 external linear layer and S-box.
