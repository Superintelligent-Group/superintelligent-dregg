# Active Design

These documents describe committed or near-term design direction. They are
important, but they are not automatically equivalent to landed runtime behavior.
Use [../10-canonical/README.md](../10-canonical/README.md) and tests to decide
what is already true.

## High-Priority Runtime Designs

- [proofs/SILVER-VISION-E2E-VERIFICATION.md](proofs/SILVER-VISION-E2E-VERIFICATION.md)
  - cross-federation end-to-end verification.
- [proofs/SOVEREIGN-WITNESS-AIR-DESIGN.md](proofs/SOVEREIGN-WITNESS-AIR-DESIGN.md)
  - sovereign witness AIR teeth.
- [proofs/STAGE-7-GAMMA-2-PI-DESIGN.md](proofs/STAGE-7-GAMMA-2-PI-DESIGN.md) -
  bilateral cross-cell algebraic binding via public inputs.
- [proofs/STAGE-7-GAMMA-2-PHASE-2-SKETCH.md](proofs/STAGE-7-GAMMA-2-PHASE-2-SKETCH.md)
  - joint aggregation AIR sketch.
- [proofs/VK-AS-RE-EXECUTION-RECIPE.md](proofs/VK-AS-RE-EXECUTION-RECIPE.md) -
  canonical VK encoders and proof re-execution framing.

## User-Space And App Designs

- [apps/STARBRIDGE-APPS-PLAN.md](apps/STARBRIDGE-APPS-PLAN.md) - canonical
  plan for post-legacy apps.
- [apps/STORAGE-AS-CELL-PROGRAMS.md](apps/STORAGE-AS-CELL-PROGRAMS.md) -
  storage primitives as cell-program patterns.
- [apps/SLOT-CAVEATS-DESIGN.md](apps/SLOT-CAVEATS-DESIGN.md) and
  [apps/SLOT-CAVEATS-EVALUATION.md](apps/SLOT-CAVEATS-EVALUATION.md) - state
  constraints and caveat evaluation model.
- [apps/AUTHORIZATION-CUSTOM-DESIGN.md](apps/AUTHORIZATION-CUSTOM-DESIGN.md) -
  custom authorization modes via witnessed predicates.

## Protocol Integration Designs

- [protocol/DESIGN-captp-integration.md](protocol/DESIGN-captp-integration.md)
- [protocol/DESIGN-pipelined-send.md](protocol/DESIGN-pipelined-send.md)
- [protocol/DESIGN-receipts.md](protocol/DESIGN-receipts.md)
- [protocol/DESIGN-commitment-framework.md](protocol/DESIGN-commitment-framework.md)
- [protocol/DFA-RATIONALIZATION-DESIGN.md](protocol/DFA-RATIONALIZATION-DESIGN.md)
- [protocol/FEDERATION-UNIFICATION-DESIGN.md](protocol/FEDERATION-UNIFICATION-DESIGN.md)

## Review Rule

Before implementing from one of these docs, check whether a newer audit or debt
entry changed the target. The most common failure mode in this repo is building
from a plausible but superseded design note.
