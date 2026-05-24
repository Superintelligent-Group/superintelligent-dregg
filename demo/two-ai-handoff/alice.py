#!/usr/bin/env python3
"""Alice — the introducer.

Drives steps 2, 3, 5, 10 of the canonical demo. Reads inputs / writes outputs
to a shared `state/` directory so `run.sh` can orchestrate.

Output JSON (printed on stdout as the last line) shape:
{
  "alice_pk": "<hex>",
  "alice_cell": "<hex>",
  "grant_turn_hash": "<hex>",
  "handoff_uri": "pyana-handoff:...",
  "bearer_cap": { "delegation_chain": "...", "expires_at": N, "permissions": "..." }
}
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from mcp_stdio import McpClient


# Per `expected.json`: the demo transfers 100 units.
TRANSFER_AMOUNT = 100


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--node-bin", required=True)
    parser.add_argument("--data-dir", required=True)
    parser.add_argument("--state-dir", required=True)
    parser.add_argument("--bob-pk", required=True, help="Bob's public key (hex)")
    parser.add_argument("--bob-cell", required=True, help="Bob's cell ID (hex)")
    args = parser.parse_args()

    state_dir = Path(args.state_dir)
    log_dir = state_dir / "logs"
    out_path = state_dir / "alice.out.json"
    uri_path = state_dir / "handoff.uri"

    with McpClient(args.node_bin, args.data_dir, "alice", log_dir) as cli:
        # ── Step 2: Alice becomes a cell ──────────────────────────────────
        agent = cli.tool("pyana_create_agent", {"name": "alice"})
        alice_pk = agent["public_key"]
        # The cell ID is derived from the public key + token domain. For now,
        # we treat alice_pk-derived cell-id as alice's identity. The node
        # internally derives this via `CellId::derive_raw(pk, [0u8;32])`.
        # We pass alice_pk where a cell id is needed; if the node distinguishes,
        # downstream blockers will surface it.
        alice_cell = alice_pk
        print(f"[alice] step 2: created agent pk={alice_pk[:16]}…", file=sys.stderr)

        # ── Step 3: grant Bob TRANSFER_ONLY (modeled as 'signature' perm here) ─
        grant = cli.tool(
            "pyana_grant_capability",
            {
                "to_agent": args.bob_pk,
                "target_cell": alice_cell,
                "permissions": "signature",
            },
        )
        if not grant.get("granted"):
            print(f"[alice] step 3 FAILED: {grant}", file=sys.stderr)
            return 3
        grant_turn_hash = grant["turn_hash"]
        print(f"[alice] step 3: granted, turn_hash={grant_turn_hash[:16]}…", file=sys.stderr)

        # ── Step 5: create bearer cap (sturdy ref) for Bob ────────────────
        # The expiration is set to a generous block height so the demo
        # doesn't race the chain.
        expires_at = 10_000_000
        bearer = cli.tool(
            "pyana_create_bearer_cap",
            {
                "target_cell": alice_cell,
                "permissions": "signature",
                "expires_at": expires_at,
                "bearer_pk": args.bob_pk,
            },
        )
        if not bearer.get("created"):
            print(f"[alice] step 5 FAILED: {bearer}", file=sys.stderr)
            return 5
        # BLOCKER-2: the MCP tool returns a delegation_chain signature, not a
        # `pyana-handoff:` URI. Until blocker-2 is fixed, we encode the
        # bearer-cap payload as a JSON blob with a `pyana+bearer:` prefix so
        # bob.py can route it. When blocker-2 lands, replace this with the
        # real compact handoff string from the tool response.
        handoff_payload = {
            "kind": "bearer-cap-v0",
            "target_cell": alice_cell,
            "bearer_pk": args.bob_pk,
            "permissions": "signature",
            "expires_at": expires_at,
            "delegation_chain": bearer["delegation_chain"],
            "introducer_pk": alice_pk,
        }
        handoff_uri = "pyana+bearer:" + json.dumps(handoff_payload, separators=(",", ":"))
        uri_path.write_text(handoff_uri)
        print(f"[alice] step 5: wrote handoff URI to {uri_path}", file=sys.stderr)

        # ── Step 10: export IVC-compressed history (best-effort, see blocker 8) ─
        compress_ok = False
        compress_result: dict | None = None
        try:
            compress_result = cli.tool(
                "pyana_compress_history",
                {"cell_id": alice_cell, "initial_root": 0},
            )
            compress_ok = compress_result.get("verification") == "valid"
        except RuntimeError as e:
            print(f"[alice] step 10 (best-effort) skipped: {e}", file=sys.stderr)

        # Final result on stdout (the LAST line — run.sh parses this).
        result = {
            "alice_pk": alice_pk,
            "alice_cell": alice_cell,
            "grant_turn_hash": grant_turn_hash,
            "handoff_uri": handoff_uri,
            "transfer_amount": TRANSFER_AMOUNT,
            "bearer_cap": {
                "delegation_chain": bearer["delegation_chain"],
                "expires_at": expires_at,
                "permissions": "signature",
            },
            "compress_history": {
                "ok": compress_ok,
                "result": compress_result,
            },
        }
        out_path.write_text(json.dumps(result, indent=2))
        print(json.dumps(result))
        return 0


if __name__ == "__main__":
    sys.exit(main())
