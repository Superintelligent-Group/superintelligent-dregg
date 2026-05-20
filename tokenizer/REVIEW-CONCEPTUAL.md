# Cryptographic Correctness Review: tokenizer + secrets

## 1. X25519 + ChaCha20-Poly1305 (tokenizer/src/encrypt.rs)

**Key derivation:** The raw 32-byte X25519 shared secret is used directly as the ChaCha20-Poly1305 key. This is non-standard (NaCl uses HSalsa20 to derive a sub-key), but cryptographically defensible: the X25519 output is indistinguishable from random to any party that does not hold either private key. The code documents this intentional divergence. No KDF (HKDF, BLAKE2b) is applied, which means there is no domain separation -- if another protocol in this system uses the same DH output for a different purpose, keys would collide. Acceptable for a single-purpose sealed box.

**Nonce uniqueness:** A fresh 12-byte random nonce is generated via `getrandom` for each seal. Combined with a fresh ephemeral keypair per seal (which already makes the key unique per message), the probability of a (key, nonce) collision is negligible. Even without the ephemeral key, 96-bit random nonces are safe well below 2^32 messages per recipient key.

**Forward secrecy:** Yes. Each seal generates a new ephemeral X25519 keypair that is never persisted. Compromise of the recipient's static key allows decryption of past ciphertexts (since the ephemeral public key is embedded in the wire format), but this is inherent to any sealed-box scheme. Compromise of the sealing process (e.g., memory dump during `seal()`) is time-bounded to that invocation.

## 2. AES-256-GCM (secrets/src/encrypted.rs)

**Nonce generation:** 12-byte random nonce from `getrandom` per encryption. Correct. With a single long-lived master key, the birthday bound for 96-bit nonces is approximately 2^48 encryptions before a collision becomes likely at probability 2^-32. For a local credential store this is effectively unlimited.

**Key handling:** The master key is wrapped in `Zeroizing<[u8; 32]>` inside `EncryptedFileStore`. However, `new()` accepts a bare `[u8; 32]` on the stack -- the caller's copy is not zeroized by this code. `from_bytes` in the tokenizer has the same pattern. This is a minor gap: the caller must ensure their copy is zeroized.

**Authenticated encryption:** The `aes_gcm` crate appends the 16-byte tag to the ciphertext (standard `encrypt()` behavior). The file format is `[nonce][ciphertext||tag]`. Decryption checks the tag before returning plaintext. Correct.

## 3. Composite Store Consistency

`CompositeStore::put` writes to ALL backends. `get` returns the first hit. This means after a partial failure (keychain succeeds, file fails), a subsequent `get` returns the keychain value, but `list` (which only the file backend can service) may show stale data. There is no reconciliation, versioning, or consistency check between backends. A secret updated in the keychain but stale on disk will silently serve the keychain value on get but show the old metadata on list. This is documented implicitly by the design (keychain cannot list) but could confuse consumers that rely on metadata timestamps.

## 4. Key Zeroization

- `SecretValue`: derives `ZeroizeOnDrop`. Correct.
- `EncryptedFileStore::master_key`: `Zeroizing<[u8; 32]>`. Correct.
- `TokenizerKeypair::secret`: `StaticSecret` from x25519-dalek, which implements `Zeroize` on drop. Correct.
- **Gap in tokenizer `seal()`:** The `Zeroizing<[u8; 32]>` ephemeral bytes are zeroized, but the `StaticSecret` constructed from them lives until end-of-scope. The DH shared secret (`SharedSecret`) is a 32-byte array on the stack -- x25519-dalek does NOT zeroize `SharedSecret` on drop. This intermediate shared secret survives in memory until the stack frame is reused. Similarly, the `cipher` object holds the expanded key schedule until dropped (chacha20poly1305 does not implement `ZeroizeOnDrop`).
- **Gap in `open()`:** Same pattern -- `shared` is not explicitly zeroized.
- **Decrypted plaintext:** Returned as a plain `Vec<u8>` from both `unseal` and `decrypt`. Not wrapped in `Zeroizing`. The secrets crate wraps results in `SecretValue` (which zeroizes), but the tokenizer returns raw `Vec<u8>`.

## 5. Sealed Secret Format

The wire format is `[32B ephemeral_pk][12B nonce][ciphertext||16B tag]`. It is NOT versioned -- there is no magic byte, version field, or algorithm identifier. Key rotation requires the caller to try decryption with multiple keys (or tag ciphertexts externally). If the algorithm changes in the future, there is no way to distinguish old from new format in the byte stream. Adding a 1-byte version prefix would be a backward-compatible improvement.

## 6. tempfile+rename Atomicity

The pattern (`NamedTempFile::new_in(dir)` + `persist()`) is correct on Unix (atomic rename within the same filesystem). On Windows, `persist()` calls `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`, which is atomic on NTFS for same-volume renames. This is correct across platforms. The `fsync` before rename ensures durability. The Unix-only `set_permissions(0o600)` is gated behind `#[cfg(unix)]`; on Windows, ACL protection is not applied -- the file inherits directory permissions. Acceptable for now, but Windows users get weaker file isolation.

## 7. Percent-Encoding in sanitize_name

The function is injective: every distinct input produces a distinct output. Alphanumeric chars plus `-` and `_` pass through; all others are percent-encoded byte-by-byte. Since the pass-through characters (`[A-Za-z0-9_-]`) are disjoint from `%`, and percent-encoding is itself injective (each byte maps to a unique `%XX`), no two distinct inputs can produce the same output. However, the function does NOT encode `%` itself specially -- an input containing a literal `%41` would encode to `%2541`, which is fine (injective), but decoding would need to be aware of this double-encoding if round-tripping is ever needed.

## Summary of Findings

| Area | Verdict |
|------|---------|
| Core crypto construction | Sound |
| Nonce uniqueness | Adequate |
| Forward secrecy | Present (ephemeral DH) |
| Zeroization | Partial gaps (shared secret, cipher state, raw Vec returns) |
| Format versioning | Missing -- no algorithm agility |
| Composite store consistency | Eventual; no reconciliation on divergence |
| Filesystem atomicity | Correct cross-platform |
| Name sanitization | Injective, no collision risk |
