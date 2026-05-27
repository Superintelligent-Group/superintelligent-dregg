# History And Archaeology

History docs are useful for understanding why the runtime looks the way it
does, but they should not override canonical docs, current code, or current
tests.

## Existing History Areas

- [../../docs-history/](../../docs-history/) - session snapshots and superseded
  designs preserved for archaeology.
- [../../old-docs/](../../old-docs/) - older documentation set.
- [../../.docs-history-noclaude/](../../.docs-history-noclaude/) - additional
  historical research and integration notes.

## Root-Level Session Docs

- [sessions/HANDOFF-2026-05-25.md](sessions/HANDOFF-2026-05-25.md) - historical
  shared-agent session state and operational warnings.
- [sessions/SESSION-2026-05-25-SUMMARY.md](sessions/SESSION-2026-05-25-SUMMARY.md) -
  landing summary for the large 2026-05-25 session.
- [sessions/PREV-SESSION-AUDIT.md](sessions/PREV-SESSION-AUDIT.md) - prior state
  reconciliation.
- [TOPLEVEL-MD-INDEX-2026-05-25.md](TOPLEVEL-MD-INDEX-2026-05-25.md) - old
  root-level Markdown index before the docs taxonomy cleanup.

## Rule

When a historical document still contains a true, important runtime claim,
promote that claim into `docs/10-canonical/` or the relevant subfolder README.
Do not make future developers rediscover canonical behavior from session logs.
