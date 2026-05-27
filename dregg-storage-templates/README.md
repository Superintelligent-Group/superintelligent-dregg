# dregg-storage-templates

Canonical cell-program reference templates for storage primitives.

## Owns

- Factory descriptors for storage-as-cell-program primitives.
- Operation-scoped `CellProgram::Cases` templates for storage patterns.

## Does Not Own

- Operator-side storage enforcement loops.
- Node relay hosting.

## Local Check

```bash
cargo check -p dregg-storage-templates
```
