# dregg-tests

Workspace-level regression and threat-model tests.

## Owns

- Cross-crate test modules that do not belong to one crate's unit tests.
- Threat-model tripwires and blocked-test labels.

## Does Not Own

- Production runtime behavior.
- Single-crate unit coverage.

## Local Check

```bash
cargo test -p dregg-tests
```
