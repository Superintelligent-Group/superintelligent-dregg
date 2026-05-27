# dregg-cell

Cell state, permissions, capabilities, predicates, factories, and state
constraints.

## Owns

- `Cell`, `CellState`, `CellId`, and ledger-facing cell state structures.
- Capability sets, caveats, attenuation, and permission requirements.
- Factory descriptors and cell programs.
- Predicate and witnessed-predicate registry data types.

## Does Not Own

- Circuit-backed verifier implementations that would create dependency cycles.
- Turn execution or receipt production.
- Node/API state.

## Local Check

```bash
cargo check -p dregg-cell
```
