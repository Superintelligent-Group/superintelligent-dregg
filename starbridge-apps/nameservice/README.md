# starbridge-nameservice

The first proper starbridge-app — federation name directory, rebuilt from
pyana-native primitives only.

See `src/lib.rs` for the in-source design notes, including the slot-caveat
gap (TODO on `build_register_action` pointing at
`../../SLOT-CAVEATS-DESIGN.md`).

## What this crate exports

- `name_factory_descriptor()` / `factory_descriptors()` —
  `FactoryDescriptor`s pinning the per-name sovereign cell's program
  VK, field constraints, capability template, and per-epoch creation
  budget.
- `build_register_action(wallet, registry, name, owner, expiry)` —
  signed action recording a name registration via four generic
  effects (3× `SetField` + 1× `EmitEvent`). **No new `Effect`
  variant**.
- `build_renew_action(wallet, registry, name, new_expiry)` — extends
  rent.
- `build_transfer_action(wallet, registry, name, old_owner, new_owner)`
  — records owner change.
- Slot-layout constants: `NAME_HASH_SLOT`, `OWNER_HASH_SLOT`,
  `EXPIRY_SLOT`.
- Rent/factory configuration: `DEFAULT_RENT_EPOCH_BLOCKS`,
  `DEFAULT_CREATION_BUDGET`, `NAME_FACTORY_VK`,
  `NAME_CHILD_PROGRAM_VK`.

## How it composes with the Starbridge platform

1. The wasm runtime (`../../wasm/src/runtime.rs`) preloads
   `factory_descriptors()` at startup. The browser-side
   `window.pyana.createFromFactory(NAME_FACTORY_VK, owner_pk, 0)`
   resolves the string VK into the real descriptor and produces a
   per-name sovereign cell.
2. The Starbridge page (`pages/index.html`) is a site fragment
   surfaced under `/starbridge-apps/nameservice/`, importing the
   shared inspector registry (`../shared/inspectors/`) and the
   nameservice's domain inspectors (`../shared/inspectors/name.js`,
   when written).
3. The extension wallet (`../../extension/src/page.ts`) signs the
   `Action` produced by `build_register_action` via `signTurn`. The
   browser-side starbridge UI never holds raw private keys.

## Coexistence with `apps/nameservice/`

The legacy `apps/nameservice/` HTTP service stays for now (Lane C
migrated it to `AppWallet`); this crate is the canonical new
implementation. The dual-existence is documented in
`../../STARBRIDGE-APPS-PLAN.md` §2.

## Standalone check

```sh
cargo check -p starbridge-nameservice
cargo test  -p starbridge-nameservice
```
