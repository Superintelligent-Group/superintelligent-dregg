#!/usr/bin/env python3
"""Bob — the recipient.

Two modes:
  --mode=identity  → create Bob's identity, print {bob_pk, bob_cell}, exit.
                     run.sh invokes this BEFORE alice so Alice knows the
                     recipient pk to bake into her grant + bearer cap.

  --mode=exercise  → read the handoff URI Alice wrote to disk, enliven,
                     exercise the cap to do a Transfer. Drives steps 6+7.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from mcp_stdio import McpClient


def run_identity(args) -> int:
    state_dir = Path(args.state_dir)
    log_dir = state_dir / "logs"

    with McpClient(args.node_bin, args.data_dir, "bob.id", log_dir) as cli:
        agent = cli.tool("pyana_create_agent", {"name": "bob"})
        bob_pk = agent["public_key"]
        # Same identity-as-cell-id convention as alice.py. See alice.py for
        # the caveat — the node derives the real cell id internally.
        bob_cell = bob_pk
        result = {"bob_pk": bob_pk, "bob_cell": bob_cell}
        (state_dir / "bob.identity.json").write_text(json.dumps(result, indent=2))
        print(json.dumps(result))
    return 0


def run_exercise(args) -> int:
    state_dir = Path(args.state_dir)
    log_dir = state_dir / "logs"
    uri_path = state_dir / "handoff.uri"
    if not uri_path.exists():
        print(f"[bob] no handoff URI at {uri_path}", file=sys.stderr)
        return 6

    handoff_uri = uri_path.read_text().strip()
    print(f"[bob] received handoff URI ({len(handoff_uri)} bytes)", file=sys.stderr)

    # Parse the URI. Today this is the `pyana+bearer:<json>` shim from
    # alice.py (see blocker-2). When the real `pyana-handoff:` compact
    # string lands, replace this with `HandoffCertificate::from_compact_string`-
    # equivalent parsing (likely a new MCP tool `pyana_decode_handoff_uri`).
    if handoff_uri.startswith("pyana+bearer:"):
        payload = json.loads(handoff_uri[len("pyana+bearer:") :])
    elif handoff_uri.startswith("pyana-handoff:"):
        print(
            "[bob] received a real pyana-handoff: URI but blocker-2 is unresolved; "
            "no decoder tool yet",
            file=sys.stderr,
        )
        return 6
    else:
        print(f"[bob] unknown URI scheme: {handoff_uri[:32]}", file=sys.stderr)
        return 6

    with McpClient(args.node_bin, args.data_dir, "bob.x", log_dir) as cli:
        # Reload Bob's identity (it was generated in --mode=identity).
        # The current MCP create_agent generates fresh keypairs every call,
        # so we re-create. Identity persistence across MCP sessions is a
        # separate gap (orthogonal to this demo).
        cli.tool("pyana_create_agent", {"name": "bob"})

        # ── Step 6: enliven + Step 7: exercise (one tool today) ───────────
        exercise = cli.tool(
            "pyana_exercise_bearer_cap",
            {
                "target_cell":      payload["target_cell"],
                "method":           "transfer",
                "delegation_chain": payload["delegation_chain"],
                "bearer_pk":        payload["bearer_pk"],
                "expires_at":       payload["expires_at"],
                "permissions":      payload["permissions"],
            },
        )

        if not exercise.get("exercised"):
            print(f"[bob] step 7 FAILED: {exercise}", file=sys.stderr)
            (state_dir / "bob.exercise.json").write_text(json.dumps(exercise, indent=2))
            return 7

        # Snapshot Bob's receipt chain so charlie/run.sh can inspect.
        chain = cli.tool("pyana_get_receipt_chain", {"limit": 50})

        result = {
            "exercise_turn_hash": exercise["turn_hash"],
            "exercised":          True,
            "receipt_chain":      chain,
            "transfer_amount":    args.amount,
        }
        (state_dir / "bob.exercise.json").write_text(json.dumps(result, indent=2))
        print(json.dumps(result))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--node-bin", required=True)
    parser.add_argument("--data-dir", required=True)
    parser.add_argument("--state-dir", required=True)
    parser.add_argument("--mode", choices=["identity", "exercise"], required=True)
    parser.add_argument("--amount", type=int, default=100)
    args = parser.parse_args()
    if args.mode == "identity":
        return run_identity(args)
    return run_exercise(args)


if __name__ == "__main__":
    sys.exit(main())
