# Runtime Flows

These walkthroughs explain how the canonical runtime moves from user intent to
state, receipt, proof, and app-level behavior. They are intentionally shorter
than the design docs and point to the code paths a developer should open first.

## Walkthroughs

- [turn-execution.md](turn-execution.md) - action/turn construction, executor
  paths, and receipt emission.
- [receipt-chain.md](receipt-chain.md) - receipt continuity, witnessed receipts,
  replay verification, and federation handoff.
- [effect-vm-proof.md](effect-vm-proof.md) - Effect VM trace/proof/verification
  path and current soundness caveats.
- [starbridge-app.md](starbridge-app.md) - canonical app path through
  `app-framework/` and `starbridge-apps/`.

## Reading Order

1. Start with [../runtime-map.md](../runtime-map.md).
2. Read [turn-execution.md](turn-execution.md).
3. Read [receipt-chain.md](receipt-chain.md) before changing receipt, verifier,
   or replay behavior.
4. Read [effect-vm-proof.md](effect-vm-proof.md) before changing proof-carrying
   execution.
5. Read [starbridge-app.md](starbridge-app.md) before adding app examples or
   app-framework helpers.
