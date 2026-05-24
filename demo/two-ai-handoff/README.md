# Two-AI Capability Handoff Demo

This is the canonical end-to-end demo for pyana, per
[`dev-philosophy/06-the-real-demo.md`](../../dev-philosophy/06-the-real-demo.md).

## Purpose

Demonstrate that pyana works *end-to-end* by having two simulated AI processes
(`alice` and `bob`) exchange a capability via three-party handoff, with the
recipient exercising the capability to do something meaningful, and an
independent verifier (`charlie`) accepting the proofs — **with no shared
memory or state** between prover and verifier.

If this demo passes:

- The Effect VM is functioning end-to-end (not just unit-tested)
- The capability model is real (Bob's authority is bounded by Alice's
  delegation)
- The handoff protocol is real (offline cert + recipient sig + swiss
  consumption)
- The receipt chain is real (it links, it can be exported, it can be
  re-verified by a process that doesn't share the prover's state)
- Two AI processes can cooperate via the substrate without trusting each
  other

## What success looks like

`./run.sh` exits 0 and prints:

```
[demo] PASS — two-AI handoff complete
  alice balance change: -100 (expected -100)
  bob's cell credited:  +100 (expected +100)
  grant turn:           verified by charlie (independent process)
  exercise turn:        verified by charlie (independent process)
  receipt chain links:  grant -> exercise
```

If any step fails, the script prints which one and why.

## The ten steps

These mirror the steps in `06-the-real-demo.md` §"The canonical demo":

| # | Description                              | Driver       | Pyana primitive                     |
|---|------------------------------------------|--------------|-------------------------------------|
| 1 | Setup: node + alice + bob + charlie      | run.sh       | `pyana-node mcp` / `pyana-node run` |
| 2 | Alice becomes a cell                     | alice.py     | `pyana_create_agent`                |
| 3 | Alice grants Bob TRANSFER_ONLY cap       | alice.py     | `pyana_grant_capability`            |
| 4 | Charlie verifies grant turn proof        | charlie      | `pyana-verifier` (TBD — extracted)  |
| 5 | Alice creates bearer cap (sturdy ref)    | alice.py     | `pyana_create_bearer_cap`           |
| 6 | Bob enlivens via URI                     | bob.py       | `pyana_exercise_bearer_cap` (init)  |
| 7 | Bob exercises (Transfer)                 | bob.py       | `pyana_exercise_bearer_cap`         |
| 8 | Charlie verifies exercise turn proof     | charlie      | `pyana-verifier`                    |
| 9 | Receipt chain links grant -> exercise    | run.sh       | `pyana_get_receipt_chain`           |
| 10| Alice exports IVC-compressed state       | alice.py     | `pyana_compress_history`            |

## Files

- `run.sh` — orchestrator. Launches everything, drives the 10 steps,
  asserts post-conditions, prints PASS/FAIL.
- `alice.py` — simulated Alice. Speaks MCP over stdio to her own
  `pyana-node mcp` process.
- `bob.py` — simulated Bob. Speaks MCP over stdio to his own
  `pyana-node mcp` process. Reads the handoff URI Alice produced from
  a file (the "out-of-band channel" — in real life this would be
  email/QR/etc.).
- `charlie.py` — verifier driver. Today, calls `pyana_verify_sovereign_proof`
  over MCP on a *separate* `pyana-node mcp` process. Once `pyana-verifier`
  is extracted as a standalone binary (see Blockers), this will shell out
  to that instead, which is structurally stronger.
- `expected.json` — declarative post-conditions the script asserts.
- `state/` — runtime scratch (logs, FIFOs, handoff URI dropbox). Cleaned
  on every `run.sh` invocation.

## How to run

```bash
cd demo/two-ai-handoff
./run.sh
```

The script will build any required binaries (`cargo build -p pyana-node`),
launch processes, drive the flow, and exit 0/1.

If `cargo build` fails, the script sleeps 60s and retries once (matches
the no-worktree concurrent-cargo policy).

## Blockers (preventing full execution today)

The orchestration scaffolding is complete, but several substrate-level
pieces need to land before the demo runs green:

1. **No standalone `pyana-verifier` binary.** `charlie.py` currently
   shells to a separate `pyana-node mcp` process and calls
   `pyana_verify_sovereign_proof`. This is *separate-process* but the
   binary is the same as the prover, so it doesn't fully meet the
   "structurally independent" bar from 06-the-real-demo.md §"What makes
   this demo real". Fix: extract `verify_*` from
   `circuit/src/effect_vm.rs` into a `pyana-verifier` crate with its
   own `main.rs` that takes (proof_bytes, public_inputs, verification_key)
   on stdin and returns yes/no.

2. **MCP `pyana_create_bearer_cap` does not register a swiss entry on
   the target federation.** It signs a delegation chain but does not call
   `SwissTable::export` (see `captp/src/sturdy.rs`). The output is a
   signature, not a `pyana://` URI. The handoff cannot be enlivened in
   the canonical three-party sense yet. Fix: thread the create flow
   through `captp::sturdy::SwissTable` (per node, via `NodeState`) and
   produce a `pyana-handoff:` compact string. Have
   `pyana_exercise_bearer_cap` validate against the same swiss table
   (or via a `pyana_enliven_handoff` companion tool).

3. **No CapTP HTTP routes on the node.** `discord-bot/src/captp_client.rs`
   posts to `/captp/export`, `/captp/enliven`, `/captp/handoff`, but
   `node/src/api.rs` does not implement those endpoints. (Not a blocker
   for this demo as it uses MCP stdio, but worth noting — see #2 for
   the equivalent MCP gap.)

4. **`pyana_grant_capability` does not generate an Effect VM STARK proof.**
   The current path executes the turn through `TurnExecutor` and emits a
   receipt, but `execution_proof` is `None` (see `node/src/mcp.rs` ~line
   1064). For step 4 to be a real verification (not a hash-equality check),
   the grant turn must produce an Effect VM proof.

5. **`pyana_exercise_bearer_cap` does not project Transfer effects through
   Effect VM.** Same problem as #4 for step 7. Per `06-the-real-demo.md`
   §"What to build to get there" item 1, this depends on
   `convert_turn_effects_to_vm` being honest about its projection. There
   is parallel work on the every-variant projection that should fix this.

6. **No `Effect::Transfer` in the bearer-cap exercise path.** Currently
   `tool_exercise_bearer_cap` creates an `Action` with `effects: vec![]`
   (mcp.rs:2232). The action lands in the receipt but actually transfers
   no value. Fix: accept `effects` (or specifically `from`/`to`/`amount`)
   as MCP parameters and construct `Effect::Transfer { from, to, amount }`.

7. **Receipt chain `previous_receipt_hash` linkage is not enforced.**
   The two turns share an agent (Alice for grant, Bob for exercise), so
   they belong to two *different* receipt chains. Per the 06 spec step
   9, both turns chain via `previous_receipt_hash`, but neither
   wallet/agent on either side currently sets that field when
   constructing turns via MCP. Fix: thread the last-receipt-hash through
   the MCP tool turn construction.

8. **No `pyana_compress_history` proof generation for non-sovereign cells**
   — Alice's cell is not sovereign by default, so step 10 either requires
   making her sovereign first or wiring `pyana_compress_history` to work
   on regular cells. (The demo's run.sh treats step 10 as optional/best-
   effort to avoid blocking on this.)

Once #1, #4, #5, and #6 are addressed, the demo executes the full
"real" flow. #2 and #7 are needed to call it "canonical three-party
handoff" rather than "bearer cap with same node." #3 and #8 are
polish.

## What the demo deliberately does NOT prove

Per `06-the-real-demo.md` §"What the demo deliberately does NOT prove":

- Federation BFT consensus (single-node)
- Cross-federation bridging (single federation)
- Scale (single transfer)
- Privacy (Charlie sees what Alice and Bob do — that's the point)
- The 23 missing AIR variants (Transfer is one of the 18 working ones)
