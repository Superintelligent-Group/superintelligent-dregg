#!/usr/bin/env python3
"""Charlie — the independent verifier.

Per 06-the-real-demo.md, Charlie is *structurally untrusted* and
*structurally independent* of the prover. Today, Charlie's independence is
weak: he runs a separate `pyana-node mcp` process (different OS PID, different
data dir) and calls `pyana_verify_sovereign_proof`.

When a standalone `pyana-verifier` binary is extracted (blocker 1), this script
should shell to that binary directly — no shared MCP server with the prover —
which is structurally stronger.

Charlie reads expected proof artifacts from `state/` (written by the run.sh
orchestrator from receipt-chain snapshots) and verifies them. Output is a
single JSON object on stdout:

  {
    "grant_verified":    bool,
    "exercise_verified": bool,
    "pid":               int,        # for the run.sh independence assertion
    "independent_node":  bool,       # always True today
    "blocker_notes":     [...]
  }
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

from mcp_stdio import McpClient


def verify_proof(cli: McpClient, proof_hex: str, public_inputs: list[int]) -> bool:
    if not proof_hex:
        return False
    try:
        result = cli.tool(
            "pyana_verify_sovereign_proof",
            {"proof_hex": proof_hex, "public_inputs": public_inputs},
        )
        return bool(result.get("valid") or result.get("verified"))
    except RuntimeError as e:
        print(f"[charlie] verify_proof error: {e}", file=sys.stderr)
        return False


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--node-bin", required=True)
    parser.add_argument("--data-dir", required=True)
    parser.add_argument("--state-dir", required=True)
    args = parser.parse_args()

    state_dir = Path(args.state_dir)
    log_dir = state_dir / "logs"

    alice_out = json.loads((state_dir / "alice.out.json").read_text())
    bob_out_path = state_dir / "bob.exercise.json"
    bob_out = json.loads(bob_out_path.read_text()) if bob_out_path.exists() else None

    blocker_notes: list[str] = []

    with McpClient(args.node_bin, args.data_dir, "charlie", log_dir) as cli:
        # ── Step 4: verify the grant turn's proof ─────────────────────────
        # BLOCKER-4: the grant turn does not yet produce an Effect VM proof,
        # so we have no proof_hex to feed the verifier. When the blocker
        # is fixed, run.sh will populate state/grant.proof.json with
        # {"proof_hex": "...", "public_inputs": [...]}.
        grant_proof_path = state_dir / "grant.proof.json"
        if grant_proof_path.exists():
            gp = json.loads(grant_proof_path.read_text())
            grant_verified = verify_proof(cli, gp["proof_hex"], gp["public_inputs"])
        else:
            blocker_notes.append("BLOCKER-4: no grant proof artifact")
            grant_verified = False

        # ── Step 8: verify the exercise turn's proof ──────────────────────
        # BLOCKER-5: same story.
        exercise_proof_path = state_dir / "exercise.proof.json"
        if exercise_proof_path.exists():
            ep = json.loads(exercise_proof_path.read_text())
            exercise_verified = verify_proof(cli, ep["proof_hex"], ep["public_inputs"])
        else:
            blocker_notes.append("BLOCKER-5: no exercise proof artifact")
            exercise_verified = False

        result = {
            "grant_verified":    grant_verified,
            "exercise_verified": exercise_verified,
            "pid":               os.getpid(),
            "independent_node":  True,   # separate data dir + process
            "independent_binary": False, # see blocker 1
            "blocker_notes":     blocker_notes,
            "alice_grant_turn":  alice_out.get("grant_turn_hash"),
            "bob_exercise_turn": (bob_out or {}).get("exercise_turn_hash"),
        }
        (state_dir / "charlie.verdict.json").write_text(json.dumps(result, indent=2))
        print(json.dumps(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
