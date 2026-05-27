# Developer Experience Cleanup Plan

This plan tracks the documentation and contributor-experience cleanup needed to
make dregg easier to understand and safer to modify.

## Lane 1: Canonical Docs Front Door

Status: started.

Acceptance criteria:

- `README.md` points to `docs/` first.
- `docs/README.md` defines documentation classes.
- `docs/10-canonical/` explains canonicality and runtime layers.
- Legacy app status is explicit.
- Existing root index and handoff docs link to the new docs front door.

## Lane 2: Root Markdown Retirement

Status: started.

Acceptance criteria:

- Root-level Markdown is reduced to true entry points and compatibility stubs.
- Design-active docs move under `docs/20-active-design/`.
- Audit docs move under `docs/30-audits/` or `audits/`.
- Session docs move under `docs/90-history/` or `docs-history/`.
- Moved files leave link-preserving stubs or all references are updated.
- `scripts/docs-check.ps1` passes.

Do this as a separate commit because it will be noisy.

## Lane 3: Executable Command Surface

Status: pending.

Acceptance criteria:

- Add a single local command surface (`justfile`, `Makefile`, or `xtask`).
- Include `check-fast`, `test-fast`, `test-full`, `node-check`, `preflight`,
  and `docs-check`.
- Document which commands are cheap, expensive, local-only, or CI-shaped.
- Make the command surface work on Windows/PowerShell and Unix shells.

## Lane 4: Package Metadata And Crate Readmes

Status: pending.

Acceptance criteria:

- Every workspace package has `description`, `license`, and a useful README or
  crate-level module doc.
- Each crate README says: purpose, owns, does not own, quick check, main tests.
- Public API barrel re-exports are documented as stable, transitional, or
  convenience-only.

## Lane 5: Test Dashboard

Status: pending.

Acceptance criteria:

- Convert `TEST-REALITY-AUDIT.md` into a living dashboard.
- Track real, ignored, slow/manual, scaffold, and synthetic tests separately.
- Add unblock labels that map ignored tests to debt/design docs.
- Make demo result files distinguish `pass`, `pending`, `scaffold`, and `fail`.

## Lane 6: Main-Surface Decomposition

Status: pending.

Acceptance criteria:

- Split large node/API/MCP/cipherclerk/test files by behavior.
- Keep public route/API shapes stable while moving implementation internals.
- Add focused tests before moving any behavior with protocol or proof meaning.
- Document module ownership in the owning crate README.
