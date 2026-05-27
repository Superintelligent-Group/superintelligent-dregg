# dregg Documentation Map

This directory is the canonical documentation front door for the backend
runtime. Root-level Markdown files still exist as source material, session
records, and historical audit artifacts, but new contributors should start
here instead of scanning the repository root.

## Reading Order

1. [Start Here](00-start-here/README.md) - contributor orientation and local
   operating rules.
2. [Canonical Runtime](10-canonical/README.md) - the current conceptual model
   and architecture truth.
3. [Testing Reality](40-testing/README.md) - what the tests prove, what is
   ignored, and what is only scaffold.
4. [Apps and Runtime Surfaces](50-apps-runtime/README.md) - CLI, node,
   starbridge apps, legacy apps, demos, and SDK surfaces.
5. [Operations](60-operations/README.md) - build, devnet, CI, and offload notes.

## Documentation Classes

- **Canonical**: current truth. Prefer these docs when there is a conflict.
- **Active design**: committed or near-term direction, but not always fully
  landed.
- **Audit evidence**: bug, test, soundness, and coverage evidence. Treat as
  specific and useful, but check whether follow-up work has since landed.
- **Operations**: runnable commands and environment constraints.
- **History**: useful archaeology, not current authority.

## Update Rule

When adding a new significant design or audit document, also update the relevant
subfolder README here. The goal is to make canonicality visible without asking a
developer to infer it from filenames or session date.
