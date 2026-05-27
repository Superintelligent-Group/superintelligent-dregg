# dregg-storage

Storage queues, inboxes, relay primitives, and availability helpers.

## Owns

- Storage-oriented queue and inbox primitives.
- Relay/storage helper logic.
- Optional KZG-backed storage proof experiments.

## Does Not Own

- Canonical cell-program templates for storage primitives. Those live in
  `dregg-storage-templates`.
- Node relay service hosting.
- App-specific storage policy.

## Local Check

```bash
cargo check -p dregg-storage
```
