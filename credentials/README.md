# dregg-credentials

Credential issue, presentation, verification, and revocation primitives.

## Owns

- Credential-shaped API around bridge presentation flows.
- Revocation and predicate-request types used by identity-style apps.

## Does Not Own

- App UI.
- General bridge proof internals.

## Local Check

```bash
cargo check -p dregg-credentials
```
