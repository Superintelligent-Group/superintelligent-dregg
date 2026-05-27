# dregg-app-framework

Application framework for dregg services and starbridge apps.

## Owns

- App server helpers, admin auth, persistence, middleware, and content stores.
- App cipherclerk wrappers.
- Starbridge app context and factory/inspector registration helpers.

## Does Not Own

- Core protocol semantics.
- Node daemon routing.
- Domain-specific runtime effects.

## Local Check

```bash
cargo check -p dregg-app-framework
```
