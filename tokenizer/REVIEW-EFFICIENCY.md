# Efficiency Review: tokenizer + secrets

## 1. X25519 Key Generation

`TokenizerKeypair::generate()` performs a `getrandom(32)` syscall plus Curve25519 scalar-base multiplication. Cost is ~50-80us on modern hardware (ARM/x86). **Not amortized across seals** -- the long-lived keypair is generated once at daemon startup, so this is not a concern. However, `SealedSecret::seal()` generates an ephemeral keypair *per encryption*, meaning every seal pays the full X25519 keygen + DH cost (~100-150us combined). This is correct for sealed-box semantics (forward secrecy per message) and acceptable for credential-write paths which are infrequent.

## 2. ChaCha20-Poly1305 Seal/Unseal Throughput

After the DH, ChaCha20-Poly1305 processes payload at ~2-4 GB/s on modern CPUs (with SIMD). For typical secrets (32-256 bytes), the symmetric cipher time is negligible (<100ns). The dominant cost in `seal()` is the two `getrandom` calls (ephemeral key + nonce = 44 random bytes). On macOS this hits `getentropy()` twice; consolidating into a single 44-byte fill would halve syscall overhead. For `unseal()`, there is no RNG call -- just DH + decrypt -- so it is faster (~60-80us total for small payloads). Throughput scales linearly with payload; the 4KB benchmark confirms this remains sub-microsecond for the symmetric portion.

## 3. AES-256-GCM (EncryptedFileStore) Throughput

`EncryptedFileStore::encrypt/decrypt` instantiates a fresh `Aes256Gcm` cipher on every call. AES-GCM key expansion costs ~200 cycles; for 1-4KB payloads the actual encryption is ~0.3-1.2us (with AES-NI). The cipher construction overhead is dwarfed by the I/O. For bulk operations (batch import of many secrets), caching the `Aes256Gcm` instance would eliminate repeated key schedules, but at current usage patterns this is immaterial.

## 4. File I/O Syscall Count

A single `put()` performs: `create_dir_all` (stat chain), `NamedTempFile::new_in` (open+fchmod), `write_all`, `sync_all` (fsync), `persist` (rename) -- then repeats the entire sequence for the `.meta` file. That is **~10-12 syscalls per put**, with two fsyncs. This is the most expensive operation by far (~2-10ms on SSD depending on fsync behavior).

Recommendations:
- Batch the `.enc` and `.meta` writes into a single temp file or defer meta fsync (metadata loss is recoverable).
- For `list()`, each `.meta` file triggers a separate `read_to_string` + JSON parse. A namespace index file would reduce this to one read, but only matters at scale (100+ secrets per namespace).
- The `exists()` check in `put()` does a redundant `path.exists()` before reading meta -- minor, but avoidable by attempting the read directly and handling ENOENT.

## 5. Keychain Lookup Latency

`KeychainStore::get()` calls `entry.get_password()` which dispatches to macOS Security framework (synchronous IPC to `securityd`). Typical latency: **0.5-2ms per call**. The `exists()` method also calls `get_password()` just to check presence, paying the full round-trip. This is inappropriate for hot paths -- if secrets are accessed per-request, results must be cached in-process. The `CompositeStore` tries keychain first on every `get()`, so a missing-key lookup pays the full keychain penalty before falling back to the file store.

Mitigation: Add an in-memory LRU or TTL cache in `CompositeStore`, or reverse the backend order (file first, keychain as backup) for read-heavy workloads.

## 6. Memory Bounds

`SecretValue` wraps an unbounded `Vec<u8>`. There is no size check in `put()` or `seal()`. A caller could store a multi-GB "secret" and the entire plaintext would be held in memory, plus a same-sized ciphertext allocation during encrypt/decrypt. The `SealedSecret::from_bytes()` allocates `data[44..].to_vec()` without a length cap.

Recommendation: Enforce a maximum secret size (e.g., 1MB) at the `SecretStore::put()` trait level and in `SealedSecret::seal()`/`from_bytes()`. This prevents accidental OOM from malformed input and keeps memory usage predictable. The `ZeroizeOnDrop` on `SecretValue` is correct for cleanup but does not bound allocation.

## Summary

The crypto is well-chosen and efficient for the intended workload. The primary performance concerns are: (a) two fsyncs per secret write, (b) keychain IPC on hot read paths without caching, and (c) no size bound on secret payloads. None are critical for the current use case (low-frequency credential management) but would matter if the store were used in a request-serving loop.
