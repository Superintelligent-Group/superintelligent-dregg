# dregg-audit

Audit helper crate for dregg.

## Owns

- Small helper utilities used by audit and evidence workflows.
- Commitment/evidence helpers that do not belong in production runtime crates.

## Does Not Own

- Canonical protocol behavior.
- Test harness ownership.
- Security-audit conclusions.

## Local Check

```bash
cargo check -p dregg-audit
```
