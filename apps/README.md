# Legacy Apps

This directory contains legacy and research app surfaces used to explore the
dregg design space. It is not the canonical app direction.

Canonical app work now lives under [../starbridge-apps/](../starbridge-apps/).
Start with [../docs/50-apps-runtime/README.md](../docs/50-apps-runtime/README.md)
and
[../docs/20-active-design/apps/STARBRIDGE-APPS-PLAN.md](../docs/20-active-design/apps/STARBRIDGE-APPS-PLAN.md).

## Current Status

The root Cargo workspace does not currently include these legacy app crates as
workspace members:

- `apps/gallery`
- `apps/bounty-board`
- `apps/compute-exchange`
- `apps/privacy-voting`

Do not treat old commands in historical docs as proof that these apps are part
of the active workspace. If you need to revive one, first decide whether it
should become a starbridge app, a demo, or a historical artifact.

## Rule For New App Work

Do not add domain-specific runtime effects such as `Effect::FooApp`. The
starbridge stance is that app behavior composes generic dregg primitives:

- factories,
- state constraints,
- capabilities,
- authorization modes,
- predicates,
- turns,
- receipts.

If an app appears to need a new domain effect, first look for the missing
generic primitive.

## Historical Value

These app crates can still be useful as design probes. They show where the
runtime surface felt too weak, where app-framework helpers were needed, and
where starbridge userspace requirements came from. Treat them as source
material, not as canonical backend-runtime proof.
