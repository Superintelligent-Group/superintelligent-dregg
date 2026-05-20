# Conceptual Review: pyana-demo and pyana-demo-agent

## Summary Verdict

Both demos are **substantially honest** about the system's capabilities. They use real crate code (pyana-commit, pyana-circuit, pyana-federation, pyana-cell, pyana-turn) rather than mocking the crypto. However, there are meaningful gaps between what a viewer infers and what is actually proven.

## 1. STARK Proof: Real but Misleading in Scope

The STARK proof IS real -- `pyana_circuit::stark` implements a genuine FRI-based proof system with Merkle commitments, Fiat-Shamir, and 50 queries. It actually verifies. However, the demo proves a **toy algebraic constraint** (parent = current + sib0 + sib1 + sib2 + position) that does NOT correspond to actual Merkle tree membership as understood by cryptographers. The "membership" being proved is membership in a constructed algebraic structure where the prover chooses the siblings. An adversary who fabricates siblings can produce a valid proof for any "leaf." The STARK proves computational integrity of the trace, but the PUBLIC INPUTS do not bind to any external federation root that the verifier independently holds. The verifier only checks that the proof is internally consistent, not that it connects to reality. This is acknowledged in the TODO comments but invisible to a demo viewer.

## 2. Security Properties: Partially Enforced by Real Code

**Correctly enforced by real crates:**
- Revocation via Merkle non-membership (pyana-commit's 4-ary tree): genuine -- once revoked, proofs cannot be generated, and stale proofs fail verification against the updated root.
- Ed25519 signatures (ed25519-dalek): real asymmetric crypto throughout. Derivation trace uses actual signature verification.
- Fold delta verification (pyana-commit): real Merkle-based proof that attenuation removed specific leaves.

**Enforced only by demo logic:**
- Federation membership: the demo uses a HashMap lookup (`federation.is_member()`), not the Morpheus consensus or attested roots that `pyana_federation::node` implements. The TODO admits this.
- Authorization (facts/rules/checks): entirely standalone logic in `demo/src/token.rs`, with a parallel pyana-commit Merkle root computed but NOT used for the actual authorization decision.
- Cross-silo "without contacting the issuer" claim: technically true in-process, but the demo shares a single `RevocationRegistry` object. Real cross-silo would need gossip/broadcast, which is not demonstrated.

## 3. Agent Demo: Realistic but Oversimplified

The agent demo exercises real pyana-cell and pyana-turn code. The TurnExecutor genuinely checks capability possession, enforces balance limits, rolls back on failure, and chains receipt hashes. These are real security properties enforced by production code.

**What is NOT realistic:**
- All cells use `AuthRequired::None` for every permission. The demo explicitly skips Ed25519/ZK signature verification. A real agent system would require cryptographic authorization for every action. This makes the "capability attenuation" claim weaker -- it is enforced by c-list membership, but NOT by cryptographic proof that the agent is who it claims.
- The "search_api" and "filesystem" capabilities are breadstuff tokens stored as hashes on the cell. The demo never actually calls an API or touches a filesystem. The effects are just `SetField` operations on the agent's own state. There is no external I/O boundary enforcement.
- The "isolated cells" claim is true at the ledger level (no direct cross-cell reads), but the demo does not demonstrate what happens when a malicious agent constructs a turn outside the executor -- the isolation is runtime-enforced, not hardware-enforced.

## 4. Hidden Edge Cases

- **Clock skew / expiration**: The demo hardcodes timestamps and never tests expiration.
- **Concurrent revocation**: What if a non-membership proof is generated, and the token is revoked before verification? The demo acknowledges this implicitly (stale proof path) but frames it as "propagation" when it is really a TOCTOU race.
- **Multiple delegation hops**: The agent demo only does one level of delegation (Alice -> Bob/Carol). The call-forest recursive execution is tested in `pyana-turn/src/tests.rs`, not in the demo.
- **Malformed inputs**: No demo shows what happens with invalid proofs, corrupted tokens, or out-of-order trace steps. The unit tests cover some of this, but a demo viewer sees only the happy path plus two expected rejections.

## 5. Investor/User Mislead Risk

**Moderate.** A sophisticated viewer reading the source would find it honest (TODOs are explicit). A non-technical viewer watching the terminal output would believe: (a) the STARK proves real federation membership (it proves a simplified algebraic statement), (b) cross-silo verification works without any shared state (it requires a shared revocation accumulator), and (c) agents are cryptographically isolated (they are only capability-isolated with auth disabled).

## 6. What the Demos Should Show but Do Not

- A negative case where a forged STARK proof fails verification (tampered witness).
- Real Ed25519 authorization in the agent demo (not `AuthRequired::None` everywhere).
- The actual Morpheus consensus from `pyana_federation::node` for membership attestation.
- A timing/race scenario where revocation and verification interleave.
- The ZK proof verifier trait (`ProofVerifier`) exercised with a real circuit proof as authorization for an agent action, connecting pyana-circuit to pyana-turn.
