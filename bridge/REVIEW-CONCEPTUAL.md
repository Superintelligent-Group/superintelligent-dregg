# Conceptual Review: bridge / token / macaroon

Reviewer: Claude Opus 4.6
Date: 2026-05-20
Scope: Design coherence and security of the caveat-to-fact-to-witness pipeline.

---

## 1. Bridge Abstraction Correctness

The `macaroon_to_factset` conversion is **lossy in two identified ways**:

- **ValidityWindow drops `not_before`**: The comment says "it's ephemeral -- once the window opens, it's always valid," but this is incorrect for tokens whose `not_before` is in the future. A ZK proof generated before the window opens would be valid after conversion since the constraint is absent from the committed state. This is a semantic gap: the plaintext macaroon verifier enforces `not_before`, but the ZK path does not.

- **Budget drops `class`, `parent_id`, and `window`**: Only `id` and `limit` are committed. Two budgets with the same ID/limit but different classes or windows would produce identical facts. This is acceptable if budget enforcement is purely external (the code says "passthrough locally"), but it means the ZK proof cannot attest to the budget class.

- **Unknown caveats are silently dropped** (line 179): `PyanaGrant::Unknown` produces an empty vec. If an upstream service adds a custom caveat type, the ZK conversion loses it entirely. This is fail-open in the ZK path while the macaroon path is fail-closed (unknown caveats would not be "cleared" during verification). Design tension here.

## 2. Macaroon HMAC Chain Security

The HMAC-SHA256 chain is textbook-correct: `T_n = HMAC(T_{n-1}, encode(C_n))` with constant-time tag comparison. XChaCha20-Poly1305 for VID/ticket encryption uses 192-bit nonces (no collision concern).

**Third-party caveat discharge reuse**: The `verify_discharge` method requires a `BindToParentToken` caveat containing `SHA256(root_tail)[0..16]`. This binds the discharge to the specific attenuated state of the root macaroon. However, the binding is only 128 bits (truncated SHA-256). While this is adequate for collision resistance, the binding check at line 241 uses a slice comparison (`wire_caveat.body[..16] == expected_hash`) which is **not constant-time**. The root tail comparison IS constant-time, but the binding check leaks timing information about the expected parent tail hash. This is a minor side-channel.

**Empty discharge exception**: Discharges with zero caveats skip the binding requirement (line 262-268, and the test `test_unbound_discharge_works_when_empty`). The reasoning is that the discharge key itself serves as binding. This is sound -- without the VID decryption (which requires replaying the HMAC chain), an attacker cannot obtain the discharge key.

## 3. Biscuit Datalog Consistency

The two backends express the same policies through different mechanisms:

- Macaroon: imperative `verify_caveats` with match-any/set-containment semantics per caveat type.
- Biscuit: declarative Datalog with `allow if app($app, $actions), request_app($app), ...`

These are **semantically equivalent** for the defined caveat types. The `pyana.rs` module generates matching Datalog. However, two gaps exist:

- **FeatureGlob is macaroon-only**: The comment in `attenuation_datalog` says "Biscuit doesn't use glob matching." This means feature glob restrictions cannot round-trip through a Biscuit token. A token converted macaroon->biscuit->macaroon would lose glob constraints.
- **Budget/Revocable are passthrough in both**: Consistent -- neither backend enforces these locally.

The attenuation model is consistent: both backends can only narrow. Biscuit uses `check if` blocks (which are additive restrictions), macaroons use HMAC-chained caveats. Both produce the same effect: added constraints never expand the capability set.

## 4. Token Format Detection

`em2_` and `eb2_` prefixes are unambiguous -- they share no common prefix with each other or with the legacy `biscuit:` prefix. The `detect_bytes` fallback uses MsgPack array markers (0x90-0x9F, 0xDC, 0xDD) vs. defaulting to Biscuit for anything else. **Risk**: a malformed token starting with 0x90 would be misrouted to the macaroon parser, which would then fail with a decode error. This is not a security issue (both paths validate signatures) but could produce confusing error messages. No collision potential between the two prefixes.

## 5. Revocation Filter

The cuckoo filter with 0.1% FPR is appropriate for this use case. Properties:

- **DoS via false positives**: An attacker who knows the filter structure could craft nonce values that collide with legitimate tokens, causing false revocations. At 0.1% FPR this affects ~1 in 1000 tokens. The `ScalableCuckooFilter` grows dynamically, so filling it is not a practical amplification vector. However, the filter has no expiration -- revoked nonces accumulate forever. A large revocation list (millions of entries) grows storage linearly but does not degrade lookup time.
- **No authentication on revocation**: The `revoke()` method takes a bare string. Ensuring only authorized parties can insert into the filter is the caller's responsibility (the sidecar/mesh layer). The filter itself provides no replay protection.

## 6. Fact Encoding Injectivity

The encoding is **NOT fully injective**. Two distinct caveats can produce the same `FactSet`:

- `App { id: "x", actions: "y" }` produces `Fact::binary(intern("app"), intern("x"), intern("y"))`. The `SymbolTable::intern` call uses `FieldElement::from_symbol(name)` which presumably hashes the string. If two distinct strings hash to the same `FieldElement`, they would be indistinguishable. This is a hash collision problem bounded by the field size (253 bits for BN-like fields, or 31 bits for BabyBear). Given `bytes_to_babybear` compresses 256 bits through Poseidon2 into a ~31-bit BabyBear element, **collision probability is approximately 2^{-31}** for any pair of distinct symbols. This is uncomfortably high for a security-critical system -- an attacker could potentially find a caveat string whose symbol hash collides with a different predicate or term, causing the ZK proof to validate against the wrong policy.

- The `FactSet` is an unordered set. Two tokens with the same caveats in different orders produce identical fact sets (which is correct behavior -- caveat order should not matter for authorization semantics).

## 7. ZK Round-Trip Information Loss

Converting token -> ZK -> back is **not designed as a round-trip**. The pipeline is one-way: plaintext token -> committed FactSet -> FoldDelta chain -> PresentationProof. There is no `factset_to_macaroon` inverse function. The ZK proof proves a statement about authorization without revealing the underlying token.

However, within the ZK path itself, the `bytes_to_babybear` compression (256 bits -> ~31-bit BabyBear via Poseidon2) is a **one-way lossy transformation**. The original symbol strings cannot be recovered from the circuit witness. The `SymbolTable` exists only on the prover side and is never serialized into the proof. This is by design (privacy), but it means the verifier cannot inspect which caveats were proven -- only that the authorization conclusion is valid for the stated public inputs (federation root, request predicate, timestamp).

---

## Summary of Findings

| # | Severity | Finding |
|---|----------|---------|
| 1 | Medium | `not_before` dropped in ZK path -- tokens valid before their activation window |
| 2 | Low | Discharge binding check is not constant-time (128-bit timing leak) |
| 3 | Low | FeatureGlob cannot round-trip through Biscuit backend |
| 4 | Medium | BabyBear symbol compression gives only ~31-bit collision resistance |
| 5 | Info | Unknown caveats are fail-open in ZK path vs fail-closed in plaintext path |
| 6 | Info | Revocation filter has no entry expiration or authenticated insertion |
