# Scripts

This directory is the small, supported command surface for local runtime work.
Prefer these wrappers over one-off command recall when onboarding, reviewing, or
preparing a branch.

## Daily Checks

```powershell
.\scripts\dev-check.ps1
.\scripts\test-fast.ps1
```

`dev-check.ps1` validates docs links, package metadata, formatting, and a small
default compile target. `test-fast.ps1` runs a small deterministic unit-test
lane, preferring `cargo nextest` when it is installed.

For a deeper runtime edit loop, pass the packages explicitly:

```powershell
.\scripts\test-fast.ps1 -Package dregg-cell,dregg-turn,dregg-verifier
```

## Full Evidence

```powershell
.\scripts\test-full.ps1
.\scripts\preflight.ps1
```

`test-full.ps1` is the broad local test lane. `preflight.ps1` runs the
golden-master subsystem gate from `dregg-preflight`.

## Inventory Helpers

```powershell
.\scripts\docs-check.ps1
.\scripts\workspace-package-report.ps1
```

These are lightweight quality checks for documentation moves and workspace
package metadata.
