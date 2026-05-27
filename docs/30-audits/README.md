# Audits

Audits are evidence. They are often more specific than design docs, but they can
go stale after fixes land. Use them to find the claim, then verify current code
before changing behavior.

## Start Here

- [tests/TEST-REALITY-AUDIT.md](tests/TEST-REALITY-AUDIT.md) - test honesty and
  scaffold inventory.
- [soundness/AIR-SOUNDNESS-AUDIT.md](soundness/AIR-SOUNDNESS-AUDIT.md) - AIR soundness
  sweep.
- [soundness/EXECUTOR-VK-AUDIT.md](soundness/EXECUTOR-VK-AUDIT.md) - executor and VK
  layering.
- [soundness/KIMI-DAMAGE-AUDIT.md](soundness/KIMI-DAMAGE-AUDIT.md) - prior-code damage
  audit.
- [tests/META-TEST-AUDIT.md](tests/META-TEST-AUDIT.md) - meta-level test quality.

## Per-Area Test Audits

- [tests/CELL-TURN-TEST-AUDIT.md](tests/CELL-TURN-TEST-AUDIT.md)
- [tests/CIRCUIT-VERIFIER-TEST-AUDIT.md](tests/CIRCUIT-VERIFIER-TEST-AUDIT.md)
- [tests/INTENT-BRIDGE-TEST-AUDIT.md](tests/INTENT-BRIDGE-TEST-AUDIT.md)
- [tests/FEDERATION-CAPTP-TEST-AUDIT.md](tests/FEDERATION-CAPTP-TEST-AUDIT.md)
- [tests/SDK-NODE-WIRE-TEST-AUDIT.md](tests/SDK-NODE-WIRE-TEST-AUDIT.md)
- [tests/STARBRIDGE-APPS-TEST-AUDIT.md](tests/STARBRIDGE-APPS-TEST-AUDIT.md)
- [tests/SUBSTRATE-TEST-AUDIT.md](tests/SUBSTRATE-TEST-AUDIT.md)

## Archive Directory

The existing [../../audits/](../../audits/) directory contains older per-crate
and cross-cutting audits. Keep using it as evidence, but promote only still-true
claims into canonical docs.
