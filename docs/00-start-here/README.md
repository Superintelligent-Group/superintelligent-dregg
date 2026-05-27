# Start Here

This repo is the core dregg backend runtime: cells, turns, proofs, federation,
CapTP, storage, apps, SDKs, demos, and developer tooling. It is a large research
runtime, so developer experience depends on knowing which documents are
canonical and which are session evidence.

## First Read

1. [../../README.md](../../README.md) - one-page product/runtime entry point.
2. [../10-canonical/README.md](../10-canonical/README.md) - current conceptual
   map.
3. [../40-testing/README.md](../40-testing/README.md) - test claims and known
   scaffolds.
4. [../50-apps-runtime/README.md](../50-apps-runtime/README.md) - runnable
   surfaces and app status.
5. [../60-operations/README.md](../60-operations/README.md) - local commands.

## Local Rules

These are promoted from [../../HANDOFF.md](../../HANDOFF.md) because they affect
every backend-runtime change:

- Prefer `cargo check -p <crate>` and `cargo test -p <crate>` while iterating.
  Full-workspace commands are expensive and should be deliberate.
- Do not use `git stash` in shared-agent worktrees.
- Do not bypass hooks with `--no-verify`.
- Do not reintroduce the old "wallet" terminology. The canonical term is
  `cipherclerk`.
- Do not hide warnings with broad `#[allow(...)]` just to get a green build.
  Investigate first, then document any scoped allowance.
- If a test is scaffold, synthetic, ignored, or blocked by a lane, label it as
  such. Do not let a green check imply a claim that is not actually tested.

## How To Pick Up Work

1. Check `git status --short --branch`.
2. Read the relevant subfolder README under `docs/`.
3. For current debt, start with
   [../10-canonical/debt/SILVER-DEBT.md](../10-canonical/debt/SILVER-DEBT.md).
4. For test truth, start with [../40-testing/README.md](../40-testing/README.md),
   then the source audit linked there.
5. Make the smallest change that improves a real runtime, test, or documentation
   contract.

For the ongoing docs and DX cleanup sequence, see
[developer-experience-plan.md](developer-experience-plan.md).
