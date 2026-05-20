# Pyana Project Status (2026-05-20)

## What's Built: 45k LOC, 16 crates, ~550 tests

| Crate | LOC | Tests | Status |
|-------|-----|-------|--------|
| macaroon | 1.9k | 29 | Solid — HMAC chain, 3P caveats, constant-time verify |
| secrets | 0.8k | 6 | Hardened — atomic writes, tempfile+rename |
| token | 3.3k | 61 | Working — Macaroon+Biscuit, Datalog injection fixed |
| tokenizer | 0.4k | 11 | Done — X25519 seal/unseal |
| commit | 2.7k | 79 | Solid — 4-ary Merkle, fold deltas, symbol table |
| trace | 2.5k | 58 | Solid — Datalog evaluator, derivation traces, trace verifier |
| circuit | 5.5k | 90 | **Real STARK proofs** — FRI, Poseidon2, IVC |
| federation | 2.4k | 27 | **Real Ed25519** — BFT consensus, revocation tree |
| audit | 1.8k | 46 | Working — budget enforcement, consistency proofs |
| bridge | 2.6k | 36/44 | ~80% — 8 test failures (API mismatches) |
| wire | 2.3k | 23 | **Real TCP** — STARK verification, postcard framing |
| store | 2.3k | 60 | Working — redb persistence, encrypted keys |
| tests | 3.0k | 129 | Adversarial suite — soundness, byzantine, fuzzing |
| demo | 2.9k | 28 | Integration demo (being unified with real stack) |
| hints | 4.2k | — | Imported — threshold signatures (BLS12-381) |
| morpheus | 7.9k | — | Imported — adaptive BFT (tests need rename fix) |

## What's Real vs. Simulated

### Fully Real (cryptographic security):
- STARK proof generation and verification (FRI + Merkle + Fiat-Shamir)
- Ed25519 signatures for federation consensus
- Poseidon2 hash over BabyBear field
- AES-256-GCM encrypted secret storage
- X25519-ChaCha20Poly1305 sealed secrets
- HMAC-SHA256 macaroon chains with constant-time verify
- TCP wire protocol with real STARK proof verification
- 4-ary Merkle membership/non-membership proofs

### Structurally Real (correct logic, placeholder crypto):
- IVC composition (hash-chain accumulation, not recursive STARK-in-STARK)
- commit/ Merkle uses BLAKE3 (not algebraic hash — can't be proven in-circuit)
- Presentation proof combines mock proofs for fold+derivation, real STARK for issuer membership

### Remaining Gaps:
- No recursive proof verification (STARK verifying a STARK)
- Two incompatible Merkle systems (BLAKE3 in commit/ vs Poseidon2 in circuit/)
- Federation consensus uses channels (not TCP/wire protocol yet)
- Bridge crate has API mismatches with token crate
- Demo uses standalone implementations in some paths

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Presentation Layer                             │
│  wire/ — TCP protocol, STARK verification, postcard framing      │
├─────────────────────────────────────────────────────────────────┤
│                    Proof Layer                                    │
│  circuit/ — BabyBear STARK, Poseidon2, AIR constraints           │
│  circuit/stark.rs — FRI prover/verifier (real ~24 KiB proofs)    │
│  circuit/ivc.rs — hash-chain IVC (sub-linear proof growth)       │
├─────────────────────────────────────────────────────────────────┤
│                    Policy Layer                                   │
│  trace/ — Datalog evaluator, derivation traces                   │
│  token/ — AuthToken trait, Macaroon+Biscuit backends             │
│  commit/ — 4-ary Merkle trees, fold deltas, state commitment     │
├─────────────────────────────────────────────────────────────────┤
│                    Federation Layer                               │
│  federation/ — BFT consensus, Ed25519, revocation trees          │
│  morpheus/ — Full adaptive BFT protocol (imported)               │
│  hints/ — Threshold signatures (imported)                        │
├─────────────────────────────────────────────────────────────────┤
│                    Storage Layer                                  │
│  store/ — redb persistence, encrypted keys, recovery             │
│  audit/ — usage log, budget enforcement, consistency proofs      │
│  secrets/ — keychain + encrypted file store                      │
└─────────────────────────────────────────────────────────────────┘
```

## Next Steps (Priority Order)

1. Unify Merkle systems (commit/ → use Poseidon2 from circuit/ for provable path)
2. Complete bridge integration (fix remaining 8 test failures)
3. Wire federation/ into wire/ (real multi-machine deployment)
4. Implement recursive STARK verification (the real IVC)
5. Performance benchmarks (proof gen time, verification time, proof sizes)
6. Documentation / paper draft
