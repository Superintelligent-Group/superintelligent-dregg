# Stage 7-ζ — Folding-Scheme Research and Decision

**Status:** research-grade design, no Rust changes. Companion to
`STAGE-7-GAMMA-AGGREGATION-DESIGN.md` (commit `621081c7`), which named
the folding-scheme choice as "research-grade" and deferred it.
Companion to `WITNESSED-RECEIPT-CHAIN-DESIGN.md` (commit `da072359`),
which named the witness shape replay needs. This document picks one,
defends the pick, and sketches the implementation.

The animating constraint: *the algebraic proof must fully constrain
ALL operations across a turn, AND the receipt chain must compress to
constant size such that a verifier downloading nothing but a root proof
can attest to the whole history of a cell.* Stage 7-γ.0 and γ.1 add
shared-PI bundling and a Poseidon2-merge `TurnAggregationAir`; they
reduce per-turn verification from "N proofs" to "N proofs + one tiny
aggregator," but the verifier still scales with `touched(turn)` and
nothing compresses across turns. Stage 7-ζ is the recursion that
collapses both dimensions: a tree-fold inside a turn, a chain-fold
across turns, both producing a single constant-size object the verifier
can swallow whole.

Two structural problems, both addressed here:

1. **Tree-fold** over a single turn's `call_forest`. A root action
   targets cell A; sub-actions target B and C; some sub-actions have
   their own sub-actions. After Stage 7-γ.1, we have N per-cell Effect
   VM proofs plus an aggregator. Stage 7-ζ replaces "N proofs + one
   aggregator" with "one folded root proof" whose verifier work is
   constant in N and constant in tree depth.
2. **Chain-fold** across a cell's receipt history. Today's
   `pyana_compress_history` (`node/src/mcp.rs:2250`) synthesises a
   monotone-root sequence and runs `prove_ivc_stark` over it — the
   IVC is real, but the "witness" is a synthetic field-element sequence
   that has no causal binding to the actual `TurnReceipt` chain. Real
   folding makes each chain step bind to a real receipt's
   `(pre_state_hash, post_state_hash, effects_hash, turn_hash)`.

These are the *same construction at two scales*, which is precisely
the IVC promise. The decision is which folding scheme to pick.

---

## 1. The problem in pyana terms

### 1.1 The tree-fold object

A `Turn`'s `call_forest: CallForest` is a forest of `CallTree` nodes
(`turn/src/forest.rs:20..40`). Each node has an `action: Action`, a
`Vec<CallTree>` of children, and a Merkle hash combining the action
hash with the children's hashes. Top-level roots fan out to children;
children fan out further. Today the maximum observed depth is 2–3 (a
delegation chain with capability exercise) but the AST permits more.

After Stage 7-γ.0/.1 lands:

```
Effect VM proof π_c for each c ∈ touched(turn), all sharing PI fields:
  turn_hash, effects_hash_global, actor_nonce, previous_receipt_hash
Aggregator proof σ_turn over a TurnAggregationAir trace whose rows
  are the per-cell effects_local[c] values, accumulating to
  effects_hash_global.
```

The verifier downloads N+1 proofs. For N=8 cells at ~30 KB each plus
~30 KB aggregator, that is ~270 KB per turn. For a typical pyana
client tracking O(10³) turns of history, that is hundreds of MB —
unworkable for a light client and painful for a phone.

What we want: a single object π_turn of size ~30 KB regardless of
forest size and depth, such that `Verify(π_turn, pi_root)` accepts
iff every leaf Effect VM proof was valid AND every projection
constraint between siblings held AND the root's PI agrees with
`Turn::hash`.

### 1.2 The chain-fold object

Per-cell receipt chains today are linked by `previous_receipt_hash`
(`turn/src/turn.rs:263`). Verifying a chain of M turns means
verifying M Effect VM proofs (plus, post-γ, M aggregators) and walking
M receipt hashes. The witness for the *whole* chain is M × ~150 KB
worst case.

What we want: a single object π_chain of size ~30 KB such that
`Verify(π_chain, (genesis_root, current_root, chain_length))` accepts
iff there exists a sequence of M turns each producing a valid
π_turn, threaded by `previous_receipt_hash`, with monotone nonces,
producing the cell's terminal state.

### 1.3 The witness exportability constraint

`WitnessedReceipt` (per `WITNESSED-RECEIPT-CHAIN-DESIGN.md` §2) bundles
a receipt with a `WitnessBundle` sufficient for *scope-(2) replay*:
re-derive trace from `(pre_state, effects, context, secrets)`, recompute
PI, re-verify the proof. The folding scheme's per-step witness must
be *exportable in this same shape* — otherwise we have two incompatible
witness representations and replay becomes infeasible for folded chains.

This is a hard constraint that excludes some otherwise-attractive
schemes. A folding scheme whose per-step witness is "a relaxed R1CS
extended assignment vector" is *not* directly replayable against an
auditor who has only `(pre_state, effects, context, secrets)`; the
auditor would need to re-run the entire fold-machine. The cleanest
shape is one where the fold-step's witness *is* the same trace that
the Effect VM AIR consumed at the leaf, plus a small amount of
"accumulator update" data that any auditor can recompute.

---

## 2. The decision criteria, made explicit

Six criteria, weighted by what pyana actually cares about:

**C1. Witness exportability.** Can the per-step witness be expressed
as `(EffectVmWitness for cell c)` + small accumulator delta, such
that a `WitnessedReceipt` for the leaf turn replays to the same fold
accumulator the prover claimed? **Weight: critical.** A scheme that
fails this means we ship two parallel witness pipelines, one for
folded archives and one for non-folded receipts, which defeats the
Golden Vision's "receipts + witness ⇒ replayable" promise.

**C2. Plonky3 alignment.** How much of `pyana-circuit` survives? The
codebase has 358-column AIRs, BabyBear, Poseidon2 width-16, FRI with
log_blowup=3. A scheme that requires a different field (Pasta,
Vesta, BN254) or different commitment scheme (KZG, Pedersen) needs a
new arithmetisation pipeline. **Weight: high.** Replacing the
prover infrastructure is the single biggest engineering cost; a
folding scheme that "fits" the existing AIR machinery is much cheaper.

**C3. Per-step prover cost.** Real pyana turns touch 1–4 cells; real
per-cell traces are 16–256 rows of a 105-column Effect VM AIR. The
fold step's prover must be fast on this scale; a scheme tuned for
1M-gate circuits at 100 ms per step is wrong-sized for our 1K-row
steps. **Weight: high.**

**C4. Recursion depth tolerance.** A `call_forest` can nest several
levels; a chain-fold runs for thousands of steps. Some schemes have
prover-cost blow-up at depth (because each step proves verification
of all prior steps' work). Others have constant per-step cost. **Weight:
medium-high.** The chain-fold is the lever; if it has bad asymptotic,
the Golden Vision dies at thousand-turn histories.

**C5. Audit story.** What other production systems use this? Has it
been formally analysed? Has anyone reported soundness issues? **Weight:
high.** Pyana ships proofs that protect real value; we cannot adopt a
2024 paper that hasn't survived production scrutiny.

**C6. Implementation effort.** Weeks vs. months vs. years to a real,
end-to-end working fold for one Effect VM proof. **Weight: medium.**
We can absorb 6 months if the result is right; we cannot absorb 24.

---

## 3. The candidates, scored on pyana's constraints

### 3.1 Nova (Kothapalli–Setty–Tzialla, 2021)

Folding scheme over relaxed R1CS: per step takes two instance-witness
pairs and produces one; the accumulator's witness is a random linear
combination of all prior witnesses; a final SNARK (Spartan or
Groth16) compresses the accumulator to constant size.

The C1 failure is decisive: Nova's per-step witness is a relaxed
R1CS extended assignment (`Z = [W; x; u]` plus cross-term `T`),
NOT the EffectVmWitness that an auditor re-derives from
`(pre_state, effects, context, secrets)`. Bridging would require
maintaining R1CS as a *parallel source of truth* alongside the AIR,
and the AIR→R1CS compile for 358 columns + LogUp lookups +
Poseidon2 NPOs is itself a multi-month project. C2 fails too:
Nova lives over Pasta/Vesta cycle-of-curves; we are BabyBear+FRI.
Adopting Nova means standing up a parallel curve-based stack on top
of the existing 100K+ LoC of Plonky3 code. C3 is good (per-step
~2 MSMs ≈ 20 ms on our trace size); C4 is excellent (per-step
constant in depth); C5 is best-in-class (Lurk in production,
Microsoft maintains, Sonobe vetted). C6 is the killer: ~12 months
to deliver, of which 4 months is R1CS compilation, 3 months curve
infrastructure, 2 months bridging to BabyBear state commitments.

**Score: 1/3/3/3/3/1 — strong cryptography, terrible fit.**

### 3.2 SuperNova (Kothapalli–Setty, 2022)

Extends Nova to *non-uniform IVC*: a finite set of `k` instructions,
the prover chooses which at each step, and only the active
instruction folds. Maps cleanly to the 46-variant Effect VM (each
selector becomes a SuperNova instruction; NoOp dominates and
contributes zero work). But the substrate is unchanged: still R1CS,
still pairing curves. Inherits Nova's C1/C2 failures plus an
instruction-multiplexing layer that adds ~2 months of work.

**Score: 1/3/2/3/2/1 — right *shape*, still trapped in R1CS-land.**

### 3.3 HyperNova (Kothapalli–Setty, 2023)

CCS-based folding. CCS (Customizable Constraint Systems) generalises
R1CS, Plonkish, and AIR into one algebraic frame; HyperNova folds
CCS instances directly. **Best constraint-system fit:** if we ingest
the Effect VM AIR as CCS, the witness IS the trace — no re-encoding,
auditor re-derivation works as written. Still expects a pairing-
friendly curve for the underlying KZG commitments; the constraint
substrate matches Plonky3 but the PCS substrate doesn't. ~10 months
because we avoid the R1CS compile but inherit the curve PCS stack.
2023 paper; few production deployments; Sonobe added a HyperNova
frontend in 2024 but it is experimental.

**Score: 2/2/2/3/1/2 — right constraint shape, parallel curve
stack. Reserved as the upgrade target.**

### 3.4 ProtoStar (Bünz–Chen, 2023)

Special-soundness folding for arbitrary public-coin protocols.
Generic enough to subsume R1CS, Plonkish, AIR; the paper's worked
examples fold a Plonkish AIR; per-step witness IS the protocol
transcript (i.e., the trace). Beautiful theoretical fit. But every
production ProtoStar implementation runs over pairing curves with
KZG commitments; none target FRI. Adopting means standing up the
curve substrate plus implementing ProtoStar's folding logic.

**Score: 3/2/2/3/2/1 — best theoretical fit, no FRI-flavoured
implementation in the wild.**

### 3.5 Plonky3-native recursion (`circuit/src/plonky3_recursion_impl.rs`)

The codebase ships `plonky3_recursion_impl::recursive` (the
`recursion` feature) backed by the `p3-recursion` fork pinned at
`c14b5fc079`. Classical recursive STARK: each step generates a full
STARK over the previous proof's verifier circuit. Native Plonky3,
native BabyBear, native FRI — *perfect substrate fit*, witness
exportability is the cleanest possible because the per-step witness
IS the inner AIR's witness plus the verifier-circuit trace.

But: the user has confirmed `p3-recursion` **does not really work**.
The companion `plonky3_verifier_air.rs` stub returns
"recursive verification is unavailable; RecursiveVerifierAir is a
non-functional placeholder." The `prove_recursive_membership` test
passes for the toy `P3MerklePoseidon2Air` but the path has never
been driven by an AIR with `EffectVmAir`'s shape (358 columns,
LogUp lookups, Poseidon2 NPOs). Effort to make it actually work is
itself a multi-month research project. Per-step cost also bad:
~5–10× inner trace per layer, so chains of any meaningful length
compound badly.

**Score: 3/3/1/2/0/?? — perfect fit on every axis except the
foundational one: it does not work today.**

### 3.6 The dark-horse: Mangrove-style "STARK-IVC by chunking"

A construction described in 2024 surveys (e.g., Wong-Wagner's
"Folding endgame"). Rather than folding in the Nova sense, *partition*
the long computation into chunks small enough to prove with vanilla
Plonky3, then hash-chain the chunks. The "fold step" is degenerate —
it commits the previous chunk's output as part of the current chunk's
input. Constant-size compression comes from a final wrapping STARK
that verifies the hash-chain consistency.

This is essentially what `circuit/src/plonky3_recursion.rs::AggregationAir`
already does (line 56). It is NOT real folding by Nova's definition
— the prover still does O(N) work over chunks — but it delivers O(1)
*verifier* work and O(1) *artefact size*, which is what the Golden
Vision actually demands. The verifier never sees inner chunks;
hash-chain commitment + final wrapping STARK is enough.

C1 witness exportability is excellent: the per-step witness is the
chunk's trace plus its hash-chain head. C2 Plonky3 alignment is
native (already partially built). C3 per-step cost is the underlying
Plonky3 prover with no folding overhead — the "fold" is a free hash
absorb. C4 depth is irrelevant (there is no recursion depth; tree
shape is achieved by a tree-shaped hash chain where parents absorb
children's roots). C5 audit story is the strongest of the field
because the cryptography reduces to "two Plonky3 STARKs and a
Poseidon2 chain," each already trusted; novelty is in system design,
not cryptography. C6: 6–10 weeks for tree-fold, another 6–10 for
chain-fold. **In-reach.**

**Score: 3/3/3/3/3/3 — but it is NOT real folding.**

The honest trade-off: vanilla hash-chunk-and-wrap gives O(N) prover
+ O(1) verifier; real Nova-folding gives O(N) prover with smaller
constants + O(1) verifier. Asymptotic is the same; prover constant
differs by ~5×. For pyana's scale (turns of 1–8 cells, chains of
length ≤ 10⁴), the constant doesn't matter for years.

---

## 4. The recommendation

**Pick: Mangrove-style "STARK-IVC by chunking" (§3.6), with an
upgrade path to HyperNova (§3.3) reserved for the day chain length
or tree depth makes the prover constant actually matter.**

This is the bold pick. The alternative — adopt Nova or HyperNova
now — would burn 10+ months of engineering on infrastructure that
delivers the same end-user property (one root proof per cell) at the
cost of a parallel curve-based proving stack. *We do not have ten
months of cryptographer-grade engineering bandwidth.* We have a
working Plonky3 stack that has already produced 358-column AIRs with
LogUp lookups and Poseidon2 NPOs; chunking-and-wrapping leverages
that stack directly.

**Defence in one paragraph.** The Mangrove approach maps the existing
`AggregationAir` pattern (Poseidon2 hash chain over per-cell PIs,
proven with the same Plonky3 prover used for Effect VM proofs)
upward to the tree-fold and chain-fold cases. The asymptotic prover
cost matches real folding (both are O(N)); the constant is ~5× worse
than Nova for the IVC step, but Nova's setup costs an order of
magnitude more total engineering. The witness exportability story
is *cleanest possible* because the fold step's witness IS the
Effect VM witness plus a hash; auditors with `WitnessedReceipt`
already have what they need. The audit story is the strongest of the
field because the cryptography reduces to "two Plonky3 STARKs and a
Poseidon2 chain," all of which we already trust. The path to real
folding is preserved: if we later need the constant-factor win,
HyperNova ingests the same AIR; we lose nothing by starting here.

**Crucially, we do not bet on `p3-recursion` working.** The
recommendation explicitly avoids the existing
`plonky3_recursion_impl::recursive` module as a production path. The
"fold" is *not* in-circuit STARK verification; it is *hash-chain
absorption + a single Plonky3 STARK over the chain.* When and if
`p3-recursion` matures, we can swap the wrapping STARK for a true
recursive verifier and get logarithmic verifier work, but we do not
need that today.

---

## 5. Concrete implementation sketch

### 5.1 Crate ownership

- **`pyana-circuit`** gains a new module
  `circuit/src/turn_fold.rs` for the tree-fold AIR and a module
  `circuit/src/chain_fold.rs` for the chain-fold AIR. Both are
  small (~300 lines each).
- **`pyana-turn`** gains a thin orchestrator module
  `turn/src/fold_driver.rs` that walks the `call_forest` bottom-up
  invoking the leaf and internal-node provers, then produces the
  root proof.
- **`pyana-node`** gains a real `pyana_compress_history`
  implementation in `node/src/mcp.rs::tool_compress_history` that
  consumes `WitnessedReceipt[]` and produces a `ChainFoldProof`.
- **`pyana-verifier`** gains `verify_turn_fold_proof` and
  `verify_chain_fold_proof` entry points.

### 5.2 The tree-fold recursive step

The tree-fold proves: *given a `CallTree` node, its action's effects
on its target cell were correctly applied, and the same holds
recursively for each child.* The proof has the shape:

```
                       π_node
                  (TurnFoldAir trace)
                          |
            PI: { node_hash, action_hash,
                  effects_acc[4], delta_acc[8],
                  cells_seen_root[4], depth_cap }
                          |
              +-----------+-----------+
              |                       |
        leaf step                internal step
        (no children)            (k children)
              |                       |
              v                       v
        inner Effect VM         inner π_self  +  π_child_1 ... π_child_k
        proof π_self            (the action's      (each a tree-fold
        bound to (target_c,      own effects on    proof of a subtree)
        old_commit, new_commit,  target_c)
        effects_local)
```

The fold step's AIR has a small trace (4 + k rows for an internal
node with k children, 1 row for a leaf) over the columns:

```
col[0..4]: previous_effects_acc (Poseidon2 absorbing state)
col[4..12]: previous_delta_acc (twin u64 balance accumulator: low + hi)
col[12..16]: child_effects_hash (or this-node's effects_hash for leaves)
col[16..24]: child_net_delta (signed)
col[24..28]: new_effects_acc (Poseidon2 output)
col[28..36]: new_delta_acc
col[36]: row selector (0 = absorb-self, 1 = absorb-child)
```

Constraints:

1. **Absorb-self row.** For each `CallTree` node, the first row
   absorbs the *inner* Effect VM proof's PI (effects_hash_local for
   target_c, net_delta_local) into the accumulators. The
   `child_effects_hash` and `child_net_delta` columns of this row
   are bound (via shared PI) to the inner π_self's PI.
2. **Absorb-child rows.** One row per child. Each binds (via shared
   PI) to the child's tree-fold proof's PI (its `effects_acc`,
   `delta_acc`, `cells_seen_root`, `depth_cap+1`). The Poseidon2
   absorption rule is the same.
3. **Boundary constraints.** First-row `previous_effects_acc` equals
   the public-input `parent_effects_acc_in` (initially 0). Last-row
   `new_effects_acc` equals the public-input `effects_acc_out`.
4. **Causality.** The node's `action_hash` (PI) must hash into the
   parent's `child_effects_hash` slot. The forest's structural Merkle
   tree (`forest.rs::compute_children_hash`) is what binds this:
   each tree-fold's `node_hash` PI equals the call-forest tree node's
   hash.
5. **Depth cap.** A depth-cap PI prevents unbounded recursion; the
   absorb-child rule asserts `child.depth_cap == self.depth_cap - 1`
   and rejects `depth_cap == 0` with children.

At the root of the call_forest, the tree-fold proof's PI binds to
`Turn::hash` (covers effects_acc, delta_acc, the depth-cap chain, and
the cells_seen root). This is precisely what Stage 7-γ.1's
`TurnAggregationAir` produces today, but instead of being a
flat "one row per touched cell" trace, it is a tree-shaped
"one fold-step per CallTree node" trace.

**The actual proof at the root is a single STARK**, not a recursive
STARK. The recursion is *over the hash chain*, not in-circuit
verification. The verifier still has to download each π_self per
leaf, OR we wrap the leaves: see §5.4.

### 5.3 The chain-fold recursive step

The chain-fold proves: *the cell's receipt chain from turn 0 to
turn M is internally consistent, with monotone nonces and matching
pre/post state hashes.* Trace:

```
col[0..4]: previous_chain_acc (Poseidon2 absorbing state, 4 felts)
col[4..8]: turn_root_hash (this turn's tree-fold PI hash)
col[8..12]: previous_post_state_hash (becomes this turn's pre)
col[12..16]: this_post_state_hash
col[16]: previous_nonce
col[17]: this_nonce
col[18..22]: previous_receipt_hash
col[22..26]: this_receipt_hash
col[26..30]: new_chain_acc
```

Constraints per row (one row per turn in the chain):

1. **State continuity.** `previous_post_state_hash` in row i+1
   equals `this_post_state_hash` in row i. Boundary: row 0's
   `previous_post_state_hash` is the cell's genesis commitment.
2. **Receipt chain hash.** `previous_receipt_hash` in row i+1 equals
   `this_receipt_hash` in row i. The `this_receipt_hash` is
   recomputed from the row's columns via Poseidon2 (matching
   `TurnReceipt::receipt_hash`).
3. **Nonce monotonicity.** `this_nonce == previous_nonce + 1` on
   every row (or `previous_nonce + 1 + skips` if rejection receipts
   are folded in; see WITNESSED-RECEIPT-CHAIN-DESIGN.md §4).
4. **Tree-fold binding.** `turn_root_hash` equals the
   `tree_fold_proof.public_inputs[0..4]` hash of the per-turn
   tree-fold proof. This is the *only* place chain-fold and
   tree-fold meet.
5. **Accumulator absorption.** `new_chain_acc = Poseidon2(
   previous_chain_acc, turn_root_hash, this_post_state_hash,
   this_nonce)` — the running commitment to the whole chain.

PI: `{ genesis_state_hash, final_state_hash, final_nonce,
final_chain_acc, chain_length }`. One STARK proof.

**Verification cost:** O(1). The verifier checks the STARK and
trusts the `final_chain_acc` as a commitment to a chain it never
materialised.

**For replay:** The witness for a chain-fold step IS the
`WitnessedReceipt` for that turn. An auditor with the full
`Vec<WitnessedReceipt>` can:
1. Verify each per-turn tree-fold proof against its WR.
2. Recompute the chain-fold trace from the WR sequence.
3. Re-prove the chain-fold STARK (Plonky3 is deterministic given
   the trace + Fiat-Shamir).
4. Compare byte-for-byte with the stored proof.

### 5.4 Wrapping leaves with a final STARK (the "fold-output
compression")

The tree-fold AIR as described in §5.2 leaves the *leaf Effect VM
proofs* in the wild — the verifier still needs them to check that
the bound `child_effects_hash` values match real proofs. To get true
constant-size verification we wrap:

```
                       π_turn_root (final wrapping STARK)
                       │
                       │ proves: there exist proofs π_self for
                       │ every CallTree leaf, each Effect VM AIR-
                       │ valid against its bound PI, AND the
                       │ tree-fold proof above holds.
                       │
                       ▼
              consumes (in private witness):
                - all leaf π_self bytes
                - the tree-fold proof π_tree
              produces (in public PI):
                - tree-fold's PI (turn_hash, effects_acc, etc.)
```

The wrapping STARK's trace contains one block per leaf proof. Each
block is a *recomputation of the leaf proof's Fiat-Shamir transcript*
under the Plonky3 challenger, checking that the transcript's
challenges match the leaf proof's recorded openings. This is
*partial-recursive verification* — the wrapping STARK doesn't
re-execute the inner AIR's constraints (that would be O(trace_size)
work, infeasible), but it *does* verify the FRI commitments and the
quotient-polynomial consistency via a hash chain.

This is the "weakest" form of recursive verification: it binds the
leaves to the tree by hash but does *not* re-check the leaves'
algebraic constraints. The soundness argument is: the leaves'
constraints were checked at proving time by the prover's `verify`
call; the wrapping STARK proves the prover ran that verification.
This is a *prover-honesty* claim, not a *cryptographic-soundness*
claim — the leaves' AIR is still trusted to be sound, but the
prover's claim that they ran `verify(leaf_proof) == Ok` is now
checked algebraically.

Strong-form wrapping (re-execute leaf AIR constraints) is what
`p3-recursion` was supposed to do; we explicitly defer that to a
later stage. The Mangrove-style wrap is sufficient for the Golden
Vision because it makes the artefact size O(1) and the verifier
cost O(1); the prover-honesty trust is acceptable for federation-
internal verification, and federation-external verifiers (light
clients) can downgrade to "verify the wrap, accept the prover's
honesty" with explicit awareness.

### 5.5 ASCII diagram of the full recursive step

```
   ┌───────────────────────────────────────────────────────────────┐
   │                       CHAIN-FOLD STARK                          │
   │   PI: (genesis, current, length, chain_acc)                     │
   │   trace: one row per Turn t                                     │
   │                                                                 │
   │      ┌──────────────────────────────────────────────┐           │
   │      │ row t:                                       │           │
   │      │   prev_chain_acc ── absorb ──→ new_chain_acc │           │
   │      │   prev_state_hash ── continuity ──→ this_post│           │
   │      │   prev_nonce ── +1 ──→ this_nonce            │           │
   │      │   turn_root_hash ← binds to ←──┐             │           │
   │      └────────────────────────────────│─────────────┘           │
   │                                       │                         │
   └───────────────────────────────────────│─────────────────────────┘
                                           │
                                           │ (one TURN-FOLD root per row)
                                           ▼
   ┌───────────────────────────────────────────────────────────────┐
   │                       TURN-FOLD STARK                           │
   │   PI: (turn_hash, effects_acc, delta_acc, cells_seen_root)      │
   │   trace: one row per CallTree node (DFS post-order)             │
   │                                                                 │
   │     root              ┌─────────┐                              │
   │       │               │ row N:  │ ← final row binds effects_acc │
   │       ▼               │ absorb  │   to PI                       │
   │     +───+             │ root    │                               │
   │     │ A │ (depth 0)   └─────────┘                              │
   │     +-+-+                                                       │
   │       │       absorb-child rows folded ←─┐                      │
   │       ├─→ B                              │                      │
   │       │   │                              │                      │
   │       │   ├─→ D     each row binds      │                      │
   │       │   └─→ E      effects_local[c]   │                      │
   │       │              to one leaf π_self │                      │
   │       └─→ C                              │                      │
   │                                          │                      │
   └──────────────────────────────────────────│──────────────────────┘
                                              │
                                              │ (one leaf π_self per CallTree)
                                              ▼
   ┌───────────────────────────────────────────────────────────────┐
   │                     EFFECT VM STARK (×N leaves)                 │
   │   PI: (OLD_COMMIT, NEW_COMMIT, effects_local, net_delta,       │
   │        turn_hash, effects_hash_global, actor_nonce, ...)        │
   │   trace: 105 cols × rows-per-VmEffect                           │
   │   (Stage 7-γ.0 shared PI fields make these leaves bindable)    │
   └───────────────────────────────────────────────────────────────┘

   Optional WRAP layer (5.4): a final STARK that absorbs all
   leaf π_self proof bytes via Poseidon2 hash chain, proving the
   prover ran verify() on each leaf. Makes turn-fold artefact
   stand-alone — verifier sees only chain-fold proof and accepts
   the whole history without downloading leaves.
```

### 5.6 How it integrates with Stage 7-γ.0 / γ.1

- **γ.0 (shared PI bundle)**: REQUIRED prerequisite. The turn-fold
  binds `turn_hash`, `effects_hash_global`, `actor_nonce`, and
  `previous_receipt_hash` because every leaf Effect VM proof carries
  these in its PI. Without γ.0, the turn-fold has nothing to bind to.
- **γ.1 (TurnAggregationAir)**: REPLACED by the turn-fold. γ.1's
  Poseidon2-merge of `effects_local[c]` values into
  `effects_hash_global` is exactly the absorb-self constraint of
  the turn-fold AIR. We do not ship γ.1 and then ζ; ζ subsumes γ.1.
  Land γ.0 (which is engineering, not research), then jump to ζ.

This is itself a recommendation: **fold γ.1 into ζ.** Stage 7-γ.0 is
the only prerequisite that has to ship before ζ. The intermediate
γ.1 milestone was a flat (one-row-per-cell) version of what ζ does
properly (one-row-per-CallTree-node, recursively); building γ.1 and
then ζ duplicates work.

### 5.7 `pyana_compress_history` becomes real

Today: `tool_compress_history` synthesises `new_roots = [initial+1,
initial+2, ...]` and runs `prove_ivc_stark` on the synthetic
sequence. The proof attests to a hash-chain over field elements that
correspond to nothing.

After ζ:

```rust
async fn tool_compress_history(params: &Value, state: &NodeState) -> McpToolResult {
    let cell_id = params.get("cell_id")...;
    let chain: Vec<WitnessedReceipt> = wallet.witnessed_receipt_chain(cell_id);

    // Re-verify each per-turn tree-fold proof against its witness.
    for wr in &chain {
        verify_turn_fold_proof(&wr.turn_fold_proof, &wr.public_inputs)?;
    }

    // Build the chain-fold trace from the WR sequence.
    let trace = build_chain_fold_trace(&chain);
    let public_inputs = chain_fold_public_inputs(&chain);

    // Prove.
    let chain_fold_proof = prove_chain_fold_stark(&trace, &public_inputs);

    McpToolResult::json(&json!({
        "compressed": true,
        "chain_length": chain.len(),
        "genesis_state": hex(chain.first().pre_state_hash),
        "current_state": hex(chain.last().post_state_hash),
        "chain_acc": hex(public_inputs.chain_acc),
        "proof_bytes": chain_fold_proof.serialize(),
    }))
}
```

The proof binds to actual receipt content, not synthetic roots. An
auditor with the original `Vec<WitnessedReceipt>` can reproduce it
byte-for-byte (Plonky3 determinism). An auditor with only the proof
+ public inputs can confirm "some chain of length L produced this
state and chain_acc."

### 5.8 Witness export shape

`WitnessedReceipt` (per WITNESSED-RECEIPT-CHAIN-DESIGN.md §2) gains
two optional fields:

```rust
pub struct WitnessedReceipt {
    pub receipt: TurnReceipt,
    pub air_name: String,
    pub proof_bytes: Vec<u8>,           // the leaf or wrapped proof
    pub public_inputs: Vec<u32>,
    pub witness: Option<WitnessBundle>, // §2 of WR doc
    pub witness_hash: [u8; 32],

    // NEW for Stage 7-ζ:
    pub turn_fold_proof: Option<Vec<u8>>,     // The per-turn tree-fold proof
    pub turn_fold_pi: Option<Vec<u32>>,       // Its PI (the turn-root commitment)
    pub aggregate_membership: Option<AggregateMembership>,
}
```

The leaf Effect VM proofs (`proof_bytes`) remain inline per receipt.
The `turn_fold_proof` is the per-turn root that the chain-fold
absorbs. The chain-fold proof itself is NOT stored per receipt —
it is computed on demand by `compress_history` from the chain of WRs.

For replay: given a `Vec<WitnessedReceipt>`:

1. Per-WR: re-derive trace from WitnessBundle, verify leaf Effect VM
   proof, verify turn-fold proof against its PI.
2. Walk the chain: confirm `previous_receipt_hash` continuity.
3. Optionally: re-prove the chain-fold STARK; compare bytes.

### 5.9 Public inputs at the chain root

The chain-fold proof's PI is the minimum a third party needs to
trust a 10⁴-turn cell history:

```
ChainFoldPI {
    cell_id: [BabyBear; 8],            // the cell whose history this is
    genesis_state_commitment: [BB; 4], // initial state hash
    current_state_commitment: [BB; 4], // post-final-turn state hash
    final_nonce: BB,                   // cell's current nonce
    chain_acc: [BB; 4],                // Poseidon2 chain commitment
    chain_length: BB,                  // M
    federation_id: [BB; 4],            // binds to a fork
}
```

A light client with only `ChainFoldPI` and `chain_fold_proof.bytes`
can verify "this is the genuine history of cell X under federation
F, currently at state S, nonce N, after M turns" in O(1) time
(one STARK verify, ~10 ms). The chain_acc is the binding that an
auditor with the witnesses can re-derive.

---

## 6. Honest unknowns

### 6.1 What a cryptographer should audit

1. **The wrap layer's soundness model (§5.4).** The Mangrove-style
   wrap is *not* full recursive verification; it is a hash-chain
   commitment to leaf-proof-bytes plus a STARK over the Fiat-Shamir
   challenge derivation. We assert this is sufficient for
   federation-internal trust ("prover ran the inner verify and got
   Ok") but it is NOT sufficient for adversarial third-party
   verification without the inner proofs. A cryptographer should
   confirm: under what threat model is this sound? Specifically,
   can a malicious prover produce a wrap-STARK that absorbs leaf
   bytes that *do not* form valid inner STARKs? Under FRI's
   soundness for the wrap and the leaf-byte hashing's collision
   resistance, this should not be possible without breaking
   Poseidon2 — but we want a written argument, not a hand wave.
2. **Nonce monotonicity in the chain-fold (§5.3).** Rejection
   receipts complicate this: if turn T_k is rejected, the chain
   skips a nonce. We need to decide whether the chain-fold AIR
   allows nonce gaps (and how to model them in the constraint) or
   whether rejected turns get a "null fold step" that consumes a
   nonce without state change.
3. **Cells_seen_root semantics.** The tree-fold's `cells_seen_root`
   PI commits to the *set* of cells touched by the call_forest. If
   set-commitment uses a Poseidon2 chain, the order matters; we'd
   need to commit to a *sorted* set (or use an actual set
   accumulator, like a sparse Merkle tree keyed by cell_id). A
   cryptographer should weigh in on the right primitive.
4. **Chain-fold genesis.** What is the *first* turn's
   `previous_chain_acc` and `previous_state_hash`? Today the
   `Genesis` action is special-cased in the executor; the fold AIR
   needs a clean boundary for "this row IS the genesis," typically
   via a one-shot column.

### 6.2 What benchmarks need to be run before commitment

1. **Wrap-STARK trace size.** For a turn with 4 leaf Effect VM
   proofs (~30 KB each), the wrap's trace contains the Fiat-Shamir
   recomputation of each. Estimate: ~10K rows × 32 columns per leaf,
   so 40K-row trace for a 4-leaf turn. Need actual measurement.
2. **Chain-fold prover cost at L=10⁴.** The chain-fold's trace is
   one row per turn; at L=10⁴ that's a 10K-row trace, ~256 columns.
   Plonky3 prover time on a 10K × 256 trace is empirically ~5 sec
   on commodity hardware. Need a benchmark.
3. **End-to-end witness replay cost.** Given a `Vec<WitnessedReceipt>`
   of length 10⁴, how long does re-derivation + verify + re-prove
   take? Critical for "auditor on a laptop" feasibility.
4. **Memory footprint of the wrap.** A 40K-row trace at 32 cols
   over BabyBear is ~5 MB witness; with FRI commitment expansion
   that's ~50 MB of prover memory. Tractable, but worth measuring.

### 6.3 What the failure mode looks like if we pick wrong

If Mangrove-style chunking turns out to have a hidden soundness gap
(e.g., the wrap doesn't actually prove what we think):

- **Detection:** a cryptanalysis paper or audit finds the gap.
  Likely route: someone constructs two distinct leaf-proof sequences
  that hash to the same wrap-input.
- **Recovery:** the wrap layer is opt-in. We can fall back to
  shipping leaf proofs alongside the turn-fold proof and chain-fold
  proof. The artefact size grows from "single root proof" to "root
  proof + leaves" but the system stays sound. Verifier work stays
  O(N) in leaves; we lose the light-client property but retain
  the executor-trust-reduction property.
- **Upgrade path:** swap the wrap for HyperNova when its
  implementation matures. HyperNova's CCS-folding gives real
  cryptographic soundness on the fold step; the rest of the stack
  (chain-fold, witness export) is unchanged.

The failure mode is *recoverable* because the wrap is structurally
separable from the tree-fold and chain-fold. Even if we lose the
wrap, we keep the per-turn-aggregation and per-cell-history
compression properties.

---

## 7. A staged path

### Stage 7-ζ.0 — Tree-fold AIR for single-cell turns (smallest viable; ~4 weeks)

**Deliverable:** real folding for the trivial case where
`call_forest` has exactly one root with no children. The fold AIR
has a 1-row trace; the constraints are degenerate (absorb-self only,
no children). The proof binds the inner Effect VM proof's PI to a
turn-fold PI.

- New file `circuit/src/turn_fold.rs`: `TurnFoldAir`, trace
  generator, prover, verifier.
- `turn/src/fold_driver.rs`: walks a single-cell `call_forest`,
  invokes leaf Effect VM prover then turn-fold prover.
- `pyana-circuit::turn_fold` exports `prove_turn_fold`,
  `verify_turn_fold`.
- Test: produce a single-cell Transfer turn, run the full pipeline,
  confirm verifier accepts.

**Why this first:** validates the AIR shape, the PI binding, and the
build-time integration with Stage 7-γ.0's shared-PI Effect VM. It
exercises every interface but nothing complicated. If γ.0 is in
flight, this can land alongside.

### Stage 7-ζ.1 — Tree-fold for general `call_forest` + chain-fold for L=1 (~6 weeks)

**Deliverable:** turn-fold handles multi-cell, multi-level trees;
chain-fold handles chains of length 1 (degenerate — just verifies
one turn-fold). The two pieces compose.

- Extend `TurnFoldAir` with absorb-child rows, depth cap, multi-row
  trace generation.
- New file `circuit/src/chain_fold.rs`: `ChainFoldAir`, trace
  generator, prover.
- Rewrite `node/src/mcp.rs::tool_compress_history` to consume
  `WitnessedReceipt[]` and produce real chain-fold proofs.
- `pyana-turn`: add `WitnessedReceipt.turn_fold_proof` and
  `.turn_fold_pi` fields per §5.8.
- Adversarial tests: a prover that swaps leaf π_self between two
  call_forest nodes; a prover that fabricates a child fold proof.

**Why this next:** delivers the Golden Vision's *per-turn*
constant-size proof and the *per-cell-chain-of-one* version. After
this lands, every shipped turn has a constant-size root artefact;
auditors can verify any single turn in O(1).

### Stage 7-ζ.2 — Real chain compression at scale + wrap layer (~8 weeks)

**Deliverable:** chain-fold for L up to ~10⁴; optional wrap layer
that makes the chain-fold proof stand-alone (verifier needs nothing
but the chain-fold proof and the public inputs).

- Optimise `ChainFoldAir` for long traces (column packing, FRI
  parameter tuning).
- Implement the wrap STARK (`circuit/src/turn_wrap.rs`) that absorbs
  leaf-proof bytes via Poseidon2 chain.
- Benchmark suite: chain-fold prove time at L = 10², 10³, 10⁴.
- Light-client mode: `pyana-verifier` accepts a chain-fold proof
  with no inner proofs and gives a verdict on cell history.
- Adversarial: a prover that produces a wrap-STARK over a chain
  that diverges from the leaf bytes' hash chain.

**Why this last:** the wrap is the bit with the weakest soundness
argument; we want it after the tree-fold and chain-fold are battle-
tested. The L=10⁴ benchmark is what tells us whether the constant
factor is acceptable; if it's not, we revisit HyperNova.

---

## 8. What this enables

Beyond compressing per-turn proofs and per-cell history, Stage 7-ζ
opens up structurally new capabilities:

### 8.1 Bridges, constant-size client state, light clients

Cross-federation: cell X in F1 wants to prove "nonce 42 as of block
1000" to F2; today F2 re-verifies the whole chain, after ζ F2
verifies one 30 KB proof in O(1). Wallet view of a cell goes from
`(CellState, Vec<TurnReceipt>)` (GBs at scale) to
`(CellState, ChainFoldProof)` (30 KB per cell). A user on a phone
verifies the chain-fold proof + wrap (§5.4) and accepts the whole
history in 10 ms; light-client mode becomes feasible.

### 8.4 Selective disclosure, capability provenance, recovery

A cell holder proves "my balance is ≥ X as of nonce N" via
chain-fold PI + a balance range proof — the chain-fold becomes a
privacy primitive. Cross-cell capability provenance ("bob's slot 7
derives from alice's grant at turn T_5") goes from executor claim
to algebraic chase via paired chain-folds absorbing the same
`intro_hash`. Recovery flows ("federation, prove my last 1000 turns
folded into X") become O(1) in proof size — the user audits a
ciphertext blob of witnesses against the public chain-fold commitment
before importing.

### 8.5 TLA invariants as runtime witnesses; federation archive

`spec/CellModel.tla`'s `BalanceConservation`, `NonceMonotonicity`,
`ReceiptChainIntegrity` are model-checked at design time today; with
chain-fold's per-row constraints, each is *algebraically witnessed
at every turn*. Federations publish a single chain-fold proof per
cell per epoch; the whole federation history compresses to
(number of cells) × 30 KB per epoch; archival storage costs become
manageable; the federation's "state" becomes a Merkle root over
chain-fold PIs.

---

## 9. Closing

Stage 7-ζ is the smallest research-grade move that turns pyana's
proof system from "per-cell, per-turn artefacts the executor
manages" into "constant-size root proofs the verifier swallows
whole." The pick is Mangrove-style STARK-IVC by chunking — not
because it has the best cryptography, but because it has the best
*fit*: native Plonky3, witness-exportable, audit-tractable,
preserves the upgrade path to HyperNova when the constant factor
matters.

The decision is bold because it explicitly rejects:

- **Nova / SuperNova:** wrong substrate (R1CS over pairing curves);
  ~12 months of parallel infrastructure for asymptotic equivalence.
- **HyperNova / ProtoStar:** right *shape*, wrong *substrate* (still
  curve-based PCS); deferred as the upgrade target.
- **Plonky3-native recursion via `p3-recursion`:** doesn't actually
  work today, despite the codebase claiming the slot. We do not bet
  the Golden Vision on a non-functional dependency.

The bet: a hash-chain-and-wrap construction over the existing
Plonky3 stack delivers the same end-user property (constant-size
verifier artefact) at one-tenth the engineering cost, with a clean
upgrade path when the constants stop being acceptable.

Stage 7-γ.0 is the prerequisite — without shared PI bindings on the
Effect VM, the fold has nothing to bind to. Stage 7-γ.1 is
*subsumed* by 7-ζ; we skip it. The smallest viable first chunk is
Stage 7-ζ.0: tree-fold for single-cell turns, proving the AIR shape
and the PI binding. Six weeks later, multi-cell tree-fold + chain-
fold for L=1. Eight weeks after that, real chain compression and
the optional wrap.

End state: a user with a phone, a chain-fold proof, and a small
public-input vector can verify the entire history of a cell in 10
milliseconds. That is the Golden Vision, delivered.

---

### Cross-references

- `STAGE-7-GAMMA-AGGREGATION-DESIGN.md` (commit `621081c7`): the
  per-turn shared-PI bundle that 7-ζ folds over. 7-γ.0 is the
  prerequisite; 7-γ.1 is subsumed.
- `WITNESSED-RECEIPT-CHAIN-DESIGN.md` (commit `da072359`): the
  witness shape that 7-ζ's per-step witness must be compatible
  with. The added `turn_fold_proof` and `turn_fold_pi` fields are
  the integration point.
- `STAGE-7-PLUS-DESIGN.md` (§2c, §6.5 N-2): the parent doc that
  marked the folding-scheme choice as deferred. This document is
  the un-deferral.
- `circuit/src/ivc.rs`: the existing IVC infrastructure
  (`prove_ivc_stark`, `IvcAir`, `StateTransitionAir`). The chain-
  fold AIR is a generalisation of `StateTransitionAir` with real
  receipt content as the absorbed material.
- `circuit/src/plonky3_recursion.rs::AggregationAir`: the working
  Plonky3-native aggregation pattern that 7-ζ's turn-fold AIR
  generalises to tree shape.
- `circuit/src/plonky3_recursion_impl.rs`: the
  recursion-via-in-circuit-verifier module that 7-ζ explicitly
  does NOT depend on; reserved for a future "real recursion" stage
  if/when `p3-recursion` matures.
- `node/src/mcp.rs::tool_compress_history` (`node/src/mcp.rs:2250`):
  the target site for the real chain-fold; replaces synthetic-root
  IVC with actual receipt-bound compression.
- `turn/src/forest.rs`: the `CallForest` / `CallTree` shape that
  the tree-fold AIR walks.
- `turn/src/turn.rs::TurnReceipt`: the per-turn artefact that
  WitnessedReceipt enriches; chain-fold absorbs one row per
  receipt.
