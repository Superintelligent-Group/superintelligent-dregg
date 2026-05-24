# Factory descriptor mirrors (JSON)

JSON mirrors of `FactoryDescriptor` definitions checked in for the
in-browser runtime to load without parsing Rust source.

Each starbridge-app's Rust crate is the source of truth; the JSON
files here are generated/checked at release time. A factory
descriptor's `hash()` is content-addressed, so any drift between the
Rust and JSON forms produces a different `factory_vk` — the wasm
runtime refuses to load mismatched descriptors.

Today: empty. The first entry will be
`name_factory.json` (mirror of
`starbridge_nameservice::name_factory_descriptor()`) once a build-time
generator is wired in. For now the wasm runtime calls the Rust
function directly via the `starbridge-nameservice` crate's exports.

See `starbridge-apps/nameservice/src/lib.rs::name_factory_descriptor`
for the canonical definition.
