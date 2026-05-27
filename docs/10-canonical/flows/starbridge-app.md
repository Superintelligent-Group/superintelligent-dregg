# Starbridge App Flow

Starbridge apps are the canonical userspace examples. They should demonstrate
how app-specific behavior is expressed through the app framework and runtime
turns, not by adding ad hoc runtime effects.

## Code Anchors

| Concern | Open first |
| --- | --- |
| App framework crate | [../../../app-framework](../../../app-framework) |
| Embedded executor lifecycle test | [../../../app-framework/tests/integration_app_cipherclerk_lifecycle.rs](../../../app-framework/tests/integration_app_cipherclerk_lifecycle.rs) |
| App framework README | [../../../app-framework/README.md](../../../app-framework/README.md) |
| Starbridge apps directory | [../../../starbridge-apps](../../../starbridge-apps) |
| Identity executor integration | [../../../starbridge-apps/identity/tests/integration_issue_present_verify.rs](../../../starbridge-apps/identity/tests/integration_issue_present_verify.rs) |
| Apps design index | [../../20-active-design/apps/README.md](../../20-active-design/apps/README.md) |
| Legacy apps warning | [../../../apps/README.md](../../../apps/README.md) |

## Flow

1. App code builds domain actions with app-specific helpers.
2. `AppCipherclerk` signs those actions in the correct federation context.
3. `EmbeddedExecutor` submits the action or assembled turn.
4. The executor produces a `TurnReceipt`.
5. App tests assert observable receipt behavior: action count, emitted events,
   rejection, receipt chain continuity, and federation-id signature binding.

## Canonical App Rule

New canonical app work belongs under `starbridge-apps/` unless there is a
specific reason to revive legacy `apps/` code. Legacy app code can remain useful
as research material, but it should not be treated as the primary app direction.

## What To Test

For app behavior, prefer executor-facing tests:

- build the action with the app helper,
- submit through `EmbeddedExecutor` or the real node/API surface,
- assert the receipt event or rejection,
- include an adversarial case when the app enforces a constraint.

Pure helper tests are still useful, but they do not prove runtime integration.
