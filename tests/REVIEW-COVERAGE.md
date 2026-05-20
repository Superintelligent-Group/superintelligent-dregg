# Adversarial Test Suite Coverage Review

## Overall Assessment

The test suite is well-structured across six modules (soundness, byzantine, commitment, trace_attacks, budget, fuzz) and covers the primary cryptographic and consensus attack surfaces. It catches real attacks rather than merely validating happy paths. However, several critical attack classes remain untested.

## Strengths

**Soundness (soundness.rs):** Byte-level proof tampering at multiple offsets, replay attacks across distinct statements, and IVC chain-break detection are all genuine attacks a real adversary would attempt. The `tamper_random_positions_in_serialized_proof` test is particularly good -- it acknowledges that not all byte positions are checked and only asserts >50% detection, which is honest.

**Trace attacks (trace_attacks.rs):** Tests the full taxonomy of Datalog trace forgery: skipped steps, injected facts, reordered derivations, substitution tampering. The two-step derivation trace is a strong choice because it exercises dependency ordering.

**Commitment (commitment.rs):** Second preimage resistance, non-membership proof invalidation after insertion, fold-chain reordering, and manual delta construction to bypass the builder API all represent realistic attack vectors.

**Budget (budget.rs):** Receipt forgery, count manipulation, windowed budget boundary conditions, and multi-verifier interleaving cover the auditing surface well.

## Gaps and Missing Attack Classes

**1. Timing side channels:** No test measures whether proof verification or hash comparison runs in constant time. An adversary with timing oracle access could distinguish valid from invalid proofs byte-by-byte. The `verify_membership` and `verify_trace` functions should be tested for timing invariance.

**2. Resource exhaustion / DoS:** The flood test (`flood_1000_revocations`) verifies correctness but not resource bounds. Missing: maximum proof size limits, maximum trace depth, deeply nested fold chains (100+ steps), and memory consumption during verification of adversarially-large inputs.

**3. True concurrency / race conditions:** `budget.rs` tests interleaved sequential access, not actual multi-threaded races. There is no `#[test]` using `std::thread::spawn` to race `record_use` calls against a shared `BudgetEnforcer`. The windowed budget boundary at exactly the window edge under concurrent access is the canonical race.

**4. State corruption / partial writes:** No test crashes a node mid-write (e.g., between updating the Merkle tree and persisting the new root) to verify recovery. The Byzantine tests crash nodes before consensus, not during state mutation.

**5. Signature verification:** The Byzantine module uses `Signature([authority as u8; 64])` -- fixed byte arrays with no actual cryptographic verification. `QuorumCertificate::is_valid()` only checks vote count, not signature validity. A test that forges a vote with a valid count but invalid signatures is missing.

**6. Malleability:** No test checks whether two semantically-equivalent but byte-different representations of the same fact produce the same hash. If `Fact::to_bytes()` has multiple valid encodings (e.g., trailing zeros, field element normalization), an attacker could create "shadow" facts.

**7. Nonce/replay across sessions:** The trace verifier and budget system have no test for cross-session replay. Can a valid receipt from one enforcer instance be accepted by a fresh instance with the same token?

## Fuzz Test Diversity

The fuzz module (fuzz.rs) uses xorshift64 with fixed seeds, generating 10,000 iterations per property. This is adequate for field axiom verification but weak for collision resistance testing (10K samples in a 2^31 field is statistically meaningless). The trace fuzzer tests only 200 random requests against 5 apps and 5 actions -- the combinatorial space is tiny. Missing: adversarially structured inputs (all-zero fields, p-1 values, repeated elements), variable-length fact sets (0, 1, max), and empty/singleton edge cases in fold chains.

## Potentially Tautological Tests

- `poseidon2_hash_4_to_1_no_collision_10k`: With a 31-bit field output, 10K samples has ~0.002% collision probability by birthday bound. This test would pass even with a broken hash that merely returned its first input unchanged.
- `view_change_when_leader_offline`: Explicitly asserts "either outcome is acceptable" -- this test cannot fail unless the code panics, making it a crash-test rather than a correctness test.
- `reorder_valid_if_deps_satisfied`: Verifies a valid trace still verifies after the evaluator produces it. It does not actually test reordering; it tests the evaluator's output, which is already tested elsewhere.

## Recommended Additions

1. Constant-time comparison test (statistical timing analysis over 10K iterations)
2. Thread-pool race on BudgetEnforcer with a budget of 1 (must never allow 2 uses)
3. Signature verification tests with actual ed25519 (or replace Signature stubs)
4. Adversarial input fuzzing: all-zero fact, max-field-element fact, duplicate facts in a FactSet
5. Cross-epoch replay: present a valid proof/receipt to a verifier at a later epoch
6. Stack/memory bomb: fold chain of depth 10,000, trace with 10,000 derivation steps
7. Partial-write recovery: simulate crash between root update and persistence
