# dregg-turn

Atomic call-forest transaction model for dregg.

## Owns

- `Action`, `Turn`, `TurnReceipt`, and call forests.
- Executor behavior and effect application.
- Receipt verification helpers and replay-chain checks.
- Obligations, escrows, queues, conflict sets, and bilateral binding helpers.

## Does Not Own

- Cell primitive definitions.
- Long-lived node state.
- Network transport.

## Local Check

```bash
cargo check -p dregg-turn
```
