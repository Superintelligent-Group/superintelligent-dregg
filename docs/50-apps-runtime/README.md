# Apps And Runtime Surfaces

This page separates current runtime surfaces from legacy or transitional app
surfaces. This matters because older app docs still describe packages that are
not root-workspace members.

## Primary Runtime Surfaces

- `cli/` - user-facing `dregg` CLI. `cargo check -p dregg-cli` is a useful
  quick health check.
- `node/` - daemon, HTTP API, MCP stdio server, relay service, genesis, and
  devnet participation.
- `sdk/` - agent SDK and cipherclerk integration surface.
- `app-framework/` - server, auth, persistence, app cipherclerk, starbridge
  context, and app helper layer.
- `wasm/` and `site/` - browser/runtime surface and published site.

## Canonical App Direction

[../../starbridge-apps/](../../starbridge-apps/) is the successor app model.
Read [../../starbridge-apps/README.md](../../starbridge-apps/README.md) and
[../20-active-design/apps/STARBRIDGE-APPS-PLAN.md](../20-active-design/apps/STARBRIDGE-APPS-PLAN.md).

Current root-workspace starbridge packages:

- `starbridge-apps/nameservice`
- `starbridge-apps/identity`
- `starbridge-apps/subscription`
- `starbridge-apps/governed-namespace`

## Legacy App Area

[../../apps/](../../apps/) currently contains legacy/research app crates such as
`gallery`, `bounty-board`, `compute-exchange`, and `privacy-voting`. Treat
[../../apps/README.md](../../apps/README.md) as historical context until the app
retirement sweep reconciles its runnable commands with the root workspace.

Do not add new domain-specific `Effect::FooApp` variants to support an app.
The starbridge rule is that apps compose generic runtime primitives.

## Demos

- `demo/cross-app-e2e` has stronger independent commitment verification than
  most demos.
- `demo/two-ai-handoff` exercises real verifier/helper paths within a narrower
  scope than its narrative suggests.
- `demo/multi-node-devnet` should be treated cautiously until its scenarios call
  real node/API surfaces instead of fixture checks.
