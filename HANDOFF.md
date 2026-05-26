# HANDOFF.md — pyana session state, 2026-05-25 → weekend

For any agent (grok / codex / gemini / claude) picking up pyana work
through the weekend. Author: claude-opus session 130628b0.

## Where things are tracked

- **This file** is the cross-agent ledger of pending work.
- **`SILVER-DEBT.md`** is the canonical Silver-vs-Golden debt ledger
  (§0 "Recently Retired" tracks closed work; §1–3 track open).
- **`TOPLEVEL-MD-INDEX.md`** indexes every audit/study doc.
- **`SESSION-2026-05-25-SUMMARY.md`** is the orientation page for this
  session's landings.

Audit docs to skim depending on the work area:

| Area | Read first |
|---|---|
| Soundness gaps | `AIR-SOUNDNESS-AUDIT.md`, `EXECUTOR-VK-AUDIT.md` |
| Receipt architecture | `RECEIPT-ARCHITECTURE-STUDY.md`, `EFFECT-WIRING-COMPLETENESS-AUDIT.md` |
| Effect VM coverage | `EFFECT-VM-NOOP-AUDIT.md`, `EFFECT-WIRING-COMPLETENESS-AUDIT.md` |
| Test reality | `TEST-REALITY-AUDIT.md`, `IGNORED-TESTS-AUDIT.md`, `KIMI-DAMAGE-AUDIT.md`, `META-TEST-AUDIT.md` |
| Vision / philosophy | `NEW-WORLD.md`, `HOUYHNHNM-COMPARISON.md`, `HOUYHNHNM-DEEP-CRITIQUE.md`, `PROTOCOL-CATEGORICAL-ANALYSIS.md` |
| API ergonomics | `SDK-API-AUDIT.md` |
| Decomposition / structure | `APPLY-METHODS-NOTES.md`, `BLOCK1-BIND-CLOSURE-NOTES.md`, `VK-NEW-CALLER-AUDIT.md` |
| Demos | `demo/cross-app-e2e/REAL-VERSION.md`, `demo/multi-node-devnet/README.md`, `MULTI-NODE-DEVNET-RUN.md` |
| Grok learnings | `GROK-EXPERIMENT-LOG.md` |

## Discipline for any continuing agent

1. **Local cargo only.** `cargo check -p <crate>` is fine. Never
   `--workspace`. Persvati (the linux box) is reserved for ember.
2. **No `git stash` — ever.** Multiple agents share the tree; stash
   destroys work.
3. **No `--no-verify` on commits.** Hooks exist for a reason.
4. **Use `git -c commit.gpgsign=false commit -m "..."`** to bypass GPG.
5. **NEVER use the name "wallet"** — it is "cipherclerk." This rename
   was done with care; do not regress.
6. **Improve, don't degrade.** If you find a bug while doing other
   work, fix it. Don't ship sham fixes that make a test pass without
   solving the underlying issue. Honest deferral is fine; document
   precisely.
7. **Don't suppress warnings** with `#[allow(...)]`. Investigate.
8. Effect → VmEffect changes touch many crates. Adding a new variant
   triggers a workspace cascade. Plan for the match-arm fanout.
9. The `cell` crate cannot depend on `circuit` (cycle). Verifier
   registries are installed at host construction, not inside `cell`.
10. **Grok learnings (per GROK-EXPERIMENT-LOG.md):** `grok -p "prompt"`
    is a one-shot LLM call with NO tool use / NO file edits — useless
    for code work. The agentic form is `grok --permission-mode
    bypassPermissions --always-approve --max-turns N --prompt-file
    <file>`. Even then, ~30 turns per file deletion is typical;
    consider doing surgical work yourself.

## Session-wide landings (2026-05-25)

Substantial. See `SESSION-2026-05-25-SUMMARY.md` for the comprehensive
view. Headline metrics:

- **~25 audit/study docs** written
- **~200 new integration tests** across cell/turn/circuit/verifier/intent/bridge/federation/captp/sdk/node/wire/starbridge-apps/storage-templates/credentials/app-framework
- **2 monolith decompositions:** `circuit/src/effect_vm.rs` (10k LOC → 12 files); `turn/src/executor.rs` (14k LOC → 12 files)
- **`apply.rs` decomposed into 52 per-Effect methods** (each independently testable)
- **All P0 audit findings closed:** VK integrity, atomic-path receipts, signature widening v3, cipherclerk strict-prev-hash, proof-carrying stub-receipt, trustless engine fail-closed defaults, NonMembership adjacency keyed-hash, temporal AIR boundary, predicate registry NotYetWiredVerifier, 4 MCP tools for starbridge-apps
- **5 P0 bugs discovered by apply.rs decomp:** ExerciseViaCapability cap-target bypass, FulfillObligation fail-open, CreateObligation condition unbound, Queue ACLs missing, NoteSpend/Create value_commitment unverified — **all closed**
- **8 lossy/synthetic AIR projections closed**
- **FRI single-row tamper gap closed** (Path A: min trace height 64 rows → miss probability ≤ 10⁻⁴⁸)
- **Silver Vision graph-of-cell-programs e2e integration test landed** (`teasting/tests/silver_vision_graph_e2e.rs` + multi-fed variant + cross-fed receipt-lift test)
- **Recursive Pickles bridge demonstrated** (`teasting/tests/golden_vision_pickles_bridge.rs`) — confirmed user's belief that Golden Vision via existing Pickles code is wirable today
- **Kimchi adversarial witness test landed** — completes Golden Vision soundness audit for the Merkle root binding layer (subject to one identified escape path: equality gate's w[1] copy-constraint to public input row)

## Lanes in flight when this was written (6 lanes)

These were dispatched as parallel subagents. Status may have updated
by the time you read this — check `git log --since="2026-05-26"
--oneline` for recent landings.

1. **#119**: 4 absent VmEffect variants (CellSeal, CellUnseal, ReceiptArchive, Refusal) — sonnet
2. **#122**: `pyana_exercise_handoff_cert` MCP tool — sonnet
3. **#125**: Stage 7-γ.2 multi-cell cross-fed binding (Seam 9, aggregator AIR) — opus
4. **Warning sweep: core proving stack** (turn/cell/circuit/verifier) — sonnet
5. **Warning sweep: network layer** (node/federation/wire/captp/intent/bridge) — sonnet
6. **Warning sweep: user-facing** (sdk/app-framework/credentials/4 starbridge-apps) — sonnet

## Pending tasks (priority-ordered for the weekend)

### Critical / Silver-Vision-closure

- **#125** Stage 7-γ.2 multi-cell cross-fed binding (in flight; if stuck, the agent's report names the design question)
- **#128** `cross_fed_receipt_lift.rs:437,474` AttestedRoot only signs 1-of-2-threshold (assertions panic when running). Fix: build threshold-many signatures OR use 1-of-1 committee.
- **#119** 4 absent VmEffect variants (in flight)
- **#129** Full head-pointer advancement on dequeue (currently fields[6] LAGS by one for multi-message queues after dequeue)

### Important / Cleanup

- **#105 / #118** `VerificationKey::new` raw BLAKE3 → `canonical_vk_v2`. **No exploitable forge today** (verifier independently re-derives). Recommendation: add `from_components(VkComponents)`, deprecate `new`. See `VK-NEW-CALLER-AUDIT.md` for caller table.
- **#122** `pyana_exercise_handoff_cert` MCP tool (in flight)
- **#123** Promote `cross_fed_receipt_cite` helper to shared `pyana_turn::cross_fed_cite` module
- **#127** Warning sweep across all crate domains (3 of 4 lanes in flight, infra/test lane completed and surfaced #128 as a bug-find)

### Nice-to-have / Documented gaps

- **#52** Golden Vision: full distributed-semantics algebraic constraint (long-term)
- **#62** Silver Vision: pre-algebraic integrated runtime that RUNS (meta — keep open until everyone agrees Silver is actually complete)

## "Verification means something" status

Per the user's framing:
> Silver Vision is to have the algebraically verified proofs of all
> leaves to be fully binding. Bridge until Golden Vision is shipping
> trees of proofs around that get verified. Verification is supposed
> to actually MEAN something.

Current state (as of this handoff):

- **Receipt layer:** Solid. Every successful turn produces a receipt;
  receipt_hash binds was_burn/was_encrypted/finality/effects_hash/
  derivation_records/prev_hash; signature (v3) covers full
  receipt_hash; cipherclerk strict-prev-hash; receipt_stream_root
  in AttestedRoot.
- **AIR projection (52 Effect variants):**
  - 37/52 sound (per `EFFECT-WIRING-COMPLETENESS-AUDIT.md`)
  - 8/52 lossy/synthetic — **CLOSED** by lossy-projections lane
  - 7/52 absent — 3 closed (Burn, CellDestroy, AttenuateCapability);
    4 in flight (#119: CellSeal, CellUnseal, ReceiptArchive, Refusal)
- **Silver→Golden bridge:** Wirable today via recursive Pickles. The
  5-leaf tree test in `teasting/tests/golden_vision_pickles_bridge.rs`
  composes through Pickles IVC and verifies the root proof.
- **Multi-cell cross-fed binding:** Per-cell proofs exist but no
  algebraic cross-cell binding (#125 in flight). Until then,
  cross-federation Transfer verification is executor-trusted at the
  source federation.
- **Final Kimchi soundness anchor:** The recursive Pickles bridge
  still calls `verify_poseidon` natively (step 1) as primary soundness
  anchor. The adversarial witness test (#126, just landed) closes the
  Merkle-root-binding audit; one escape path documented (equality
  gate's w[1] copy-constraint to public input row — fix is straightforward).

## Known traps / things that surprise newcomers

1. **`grok -p "prompt"` does nothing** — see Discipline #10.
2. **Two AIRs, easy to confuse:**
   - Effect VM AIR (`circuit/src/effect_vm/`) — general-purpose row-per-effect; bridge in `turn/src/executor/effect_vm_bridge.rs`
   - Effect Action AIR (`circuit/src/effect_action_air.rs`) — per-effect schemas (SCHEMA_BURN etc.) dispatched via `effect_binding_proofs`
   Adding a constraint at one doesn't help the other.
3. **`fields[4]` is the queue tail, `fields[6]` is the head.** The
   prior assumption that `fields[4]` was the head produced silently
   wrong FIFO behavior (caught by #124).
4. **Several "REAL-BUG-HIDDEN" findings from `IGNORED-TESTS-AUDIT.md`
   turned out to be stale-test misdiagnoses** (#101 SlotChanged, #102
   revocation replay, #116 Renounced bricked — all were correct
   protocol behavior with outdated tests). When investigating audit
   findings, verify the underlying claim before assuming there's a bug.
5. **The `cell` crate cannot depend on `circuit`** (cycle). Verifier
   registries are installed at host construction. Hence
   `NotYetWiredVerifier` for the predicates that need circuit-side
   verifiers.
6. **`cross-app-e2e/verify.py` is structural-only** (Python re-derives
   commitments via BLAKE3 and compares to Rust). The proof-carrying
   demo is `two-ai-handoff/` (real Ed25519 + STARK + replay-chain).
7. **`apply.rs` is now 52 methods, not a mega-match.** Bugs are
   per-method-grep-able now. Several P0s were uncovered by the decomp
   (#111–#115, all closed).
8. **`multi-node-devnet` 5/5 scenarios pass — but some are bash
   theater** (write fixture, assert `cp` preserves it). The
   `synthetic_warnings` field in result.json now distinguishes
   synthetic vs live passes. Don't trust "5/5" without checking
   `jq '.synthetic_warnings'`.

## Parked design questions

- **#105**: `VerificationKey::new` raw BLAKE3 — recommended Option A
  (`from_components` + deprecate `new`) per `VK-NEW-CALLER-AUDIT.md`.
  Not yet implemented (#118).
- **Burn AIR layering**: `SCHEMA_BURN` (effect_action_air, snapshot-aware)
  + `VmEffect::Burn` (Effect VM AIR) are sibling layers covering
  complementary concerns. Keep both unless someone has a unified design.
- **CapTpDelivered through executor for Bob's MCP exercise**: needs
  `pyana_exercise_handoff_cert` MCP tool (#122, in flight) to switch
  from `Authorization::Bearer` to `Authorization::CapTpDelivered`.

## Quick start for picking up the lane

```bash
cd /Users/ember/dev/breadstuffs
git log --oneline -30                  # see what just landed
git status -uno                        # any uncommitted work
cat HANDOFF.md                         # this file
cat SILVER-DEBT.md | head -60          # canonical debt
cat SESSION-2026-05-25-SUMMARY.md      # orientation
ls *.md | head -40                     # audit docs at root
```

To pick a task:
1. Look at the "Pending tasks" section above
2. Or `git log` for in-flight work that might need follow-up
3. Or grep for `TODO`, `FIXME`, `REVIEW[..]` markers
4. Or check `SILVER-DEBT.md` for tier 1/2/3 items

Run cargo locally:
```bash
cargo check -p <crate>                 # single crate
cargo test -p <crate> --lib            # single-crate lib tests
# DO NOT: cargo check --workspace      # too slow on Mac without persvati
```

Hand back at end-of-session with what landed + what's still open.

Good luck. 🍞
