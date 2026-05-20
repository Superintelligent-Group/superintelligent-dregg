# Bridge Pipeline Efficiency Review

## 1. Macaroon Verification Cost

Verification replays the HMAC-SHA256 chain: one HMAC call per caveat plus one for the nonce. For N caveats, that is N+1 HMAC-SHA256 operations plus a constant-time 32-byte comparison. HMAC-SHA256 on short messages (caveat bodies are typically 8-64 bytes) runs in roughly 200-400ns on modern hardware. A 5-caveat token costs approximately 1.2-2us total. Scaling is strictly O(N) and the constant is tiny because HMAC-SHA256 is hardware-accelerated on most platforms. Third-party caveats add one XChaCha20-Poly1305 decryption per discharge but this is still sub-microsecond. This is not a bottleneck for any plausible N.

## 2. Biscuit Datalog Evaluation Complexity

The `Evaluator::derive_one_round` performs nested iteration: for each rule R with B body atoms, it enumerates all substitutions by scanning the fact set F once per body atom, pruning at each step. The worst-case complexity per round is O(R * F^B). However, the standard policy has 7 rules with at most 4-5 body atoms, and the fact set after bridge conversion is small (typically 2-10 facts plus 6-8 request facts, so F < 20). Evaluation reaches fixpoint in 1-2 rounds because the only derived predicate is `allow`. In practice this means roughly 7 * 20^4 = 1.1M unification attempts in the absolute worst case, but predicate filtering short-circuits most of them immediately (O(1) comparison on 32-byte symbols). Real-world evaluation is well under 100us for typical token states.

## 3. Bridge Conversion Overhead (macaroon_to_factset)

Conversion iterates the caveat list once. Each caveat is decoded (`decode_grant` is MessagePack deserialization of a small struct) then mapped through `grant_to_facts` which performs 1-3 `symbols.intern()` calls. Symbol interning is BLAKE3(string) + HashMap insert -- roughly 100-200ns per symbol. For a token with 5 caveats producing 5-8 facts, total conversion cost is approximately 2-5us. The SymbolTable uses a `HashMap<[u8; 32], String>` so lookups are O(1) amortized with 32-byte key hashing (already a hash, so distribution is perfect). No expensive work here.

## 4. Poseidon2 Hashing in Bridge

The bridge calls Poseidon2 in two places: (a) `bytes_to_babybear` which does `hash_many` on 8 limbs (2 permutation calls: one absorb of 4 + one absorb of 4), and (b) `hash_fact` which does 1 permutation call. Each Poseidon2 permutation is 30 rounds (8 external + 22 internal) of field arithmetic over BabyBear (u32 modular operations). A single permutation involves roughly 8*30 = 240 S-box evaluations (x^7 = 3 multiplications) plus linear layers. Estimated wall-clock: 2-5us per permutation in this pure-Rust reference implementation (no SIMD, no lookup tables for round constants -- `round_constants()` recomputes from BLAKE3 on every call, which is expensive).

Per proof, Poseidon2 calls include: one `bytes_to_babybear` for the issuer key (2 permutations), one per fold step for old/new roots (2 each), one per removed fact, one per body fact hash in the derivation witness, plus 8 Merkle levels for issuer membership. For a typical 2-step chain with 2 removed facts and 3 body facts: roughly 2 + 4 + 2 + 3 + 8 = 19 permutations. At 3-5us each (dominated by BLAKE3 round constant regeneration), that is 57-95us of Poseidon2 time per proof.

**Critical inefficiency**: `round_constants()` and `internal_diag()` are recomputed from scratch (30 BLAKE3 hashes for round constants, 8 for diag) on every single permutation call. These should be lazy-static constants. This likely doubles or triples the Poseidon2 wall-clock time.

## 5. MockProof vs Real STARK

MockProof calls `generate_trace()` then iterates constraints row-by-row -- O(rows * constraints). For the issuer membership AIR (8-level Merkle, padded to 8 rows, width 6), this is roughly 8 * 3 = 24 constraint evaluations. Total: well under 10us.

Real STARK (`prove()` in stark.rs) performs Lagrange interpolation O(n^2) per column (n = trace length, padded to power of 2), evaluates on 4x blowup domain, builds two Merkle trees over the domain, and runs FRI. For an 8-row trace (padded to 8): interpolation is 8^2 * 6 cols = 384 field mults, domain evaluation is 32 * 6 = 192 poly evals, Merkle trees over 32 leaves, 50 query proofs with paths. Estimated: 1-5ms for the real STARK vs <10us for mock. That is a 100-500x difference. Mock is absolutely sufficient for development iteration.

## 6. Memory Allocation in the Bridge Pipeline

Several allocation patterns are notable:

- **Vec<Fact> in grant_to_facts**: Each match arm allocates a 1-3 element Vec. These are small but numerous. Could use SmallVec<[Fact; 2]> or return an ArrayVec.
- **Clone in attenuation_to_delta**: `old_symbols.clone()` copies the entire HashMap. The `FoldDeltaBuilder::new(old_state.clone())` clones the TokenState (which contains a BTreeSet + MerkleTree). These are unavoidable given the current API but notable for hot paths.
- **String formatting in delta.rs**: `format!("{}_{}", pred_name, i)` and `format!("bridge_check_{}", i)` allocate per-restriction. Minor but present.
- **reconstruct_evaluator_facts**: Builds a new `Vec<TraceFact>` with cloned facts on every proof generation. The Vec grows dynamically but is typically < 30 elements, so at most 1-2 reallocations.
- **TokenState cloning in prove()**: `final_state.clone()` in the prove path is required because `root()` takes `&mut self`. This clone copies the Merkle tree.

None of these are catastrophic, but the TokenState/MerkleTree clones are the heaviest at O(n) where n is fact count.

## 7. Can It Handle 1000 Presentations/Second?

Budget per presentation at 1000/s: 1ms.

Breakdown of the mock-proof path:
- Macaroon verification: ~2us (5 caveats)
- macaroon_to_factset: ~3us
- Attenuation delta computation: ~10us (Merkle recomputation)
- Datalog evaluation: ~50-100us
- Poseidon2 witness building: ~100us (with the round-constant recomputation bug)
- MockProof generation: ~10us
- Total: ~175us

At 175us per presentation, the system can handle approximately 5,700 presentations/second on a single core with mock proofs. 1000/s is achievable with comfortable headroom. With real STARK proofs (1-5ms for issuer membership alone), throughput drops to 200-1000/s, making it marginal. For production at 1000/s with real proofs, the STARK prover would need to be parallelized or batched.

**Key optimizations to unlock more headroom**: (1) memoize Poseidon2 round constants as static data, (2) avoid cloning TokenState in prove() by making root() take &self with interior mutability, (3) use SmallVec for grant_to_facts return values.
