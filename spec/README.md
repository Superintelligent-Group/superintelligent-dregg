# pyana cell-model spec (TLA+)

A first-cut formal specification of pyana's three most-fundamental cell-model
invariants. The goal is an honest, audit-against-Rust foundation — a reader
should be able to point at each TLA+ predicate and find the corresponding code
in `cell/src/*` and `turn/src/*`.

This is intentionally small (a few hundred lines of TLA+). It does not try to
model the whole system.

## What is modeled

### I1. Identity integrity

`cell.id == BLAKE3(cell.public_key || cell.token_id)`.

- Modeled by `DeriveId(pk, tid) == <<pk, tid>>` and using the derived value as
  the key of the `cells` function. The injectivity of BLAKE3 over a 64-byte
  input is abstracted as the injectivity of tuple construction.
- The invariant `IdentityIntegrity` asserts that every cell in the ledger
  satisfies `id == DeriveId(cells[id].pk, cells[id].tid)`.
- Because the actions only ever insert at `DeriveId(pk, tid)` and never
  update `pk` or `tid` after insertion, the invariant is maintained by
  construction. Rust analog: `Cell.id`, `Cell.public_key`, `Cell.token_id`
  are `pub(crate)` and only set via `Ledger::update_with` (see
  `cell/src/cell.rs` doc comment near `pub(crate) id: CellId`).

### I2. Nonce monotonicity

Per-cell nonce starts at 0, increments by exactly 1 per successful turn, and
wrong-nonce turns are rejected. The ledger nonce never regresses.

- `Init` sets every newly-created cell's nonce to 0 (`CreateCell` action).
- `SuccessfulTurn(id, providedNonce)` requires
  `providedNonce = cells[id].nonce` and updates `cells[id].nonce` to
  `cells[id].nonce + 1`.
- `RejectedTurn(id, providedNonce)` is enabled exactly when
  `providedNonce # cells[id].nonce`, and is a no-op on `cells`/`caps`.
- The action property `MonotonicNonce == [][NonceMonotonic]_vars` says that
  no transition decreases any existing cell's nonce.
- Rust analog: `Effect::IncrementNonce` in `turn/src/action.rs`, the nonce
  check in `turn/src/executor.rs`.

### I3. Capability attenuation lattice

For any granted capability `D` derived from a held capability `P`,
`is_attenuation(P.permissions, D.permissions)` must hold. The five-element
lattice is partial:

```
            Impossible      (top — most restrictive)
              /    \
        Signature  Proof
              \    /
             Either
                |
               None         (bottom — least restrictive)
```

- `IsNarrowerOrEqual(a, b)` mirrors
  `cell/src/permissions.rs::AuthRequired::is_narrower_or_equal` exactly.
- `IsAttenuation(parent, granted) == IsNarrowerOrEqual(granted, parent)`
  mirrors `cell/src/capability.rs::is_attenuation` exactly.
- Actions `GrantFromOwn` and `Redelegate` carry `IsAttenuation` as a
  precondition — they cannot fire with an amplifying permission.
- The state invariant `AttenuationSoundness` asserts that every capability
  in the c-list either was minted from a cell whose own perm attenuates to
  the cap's perm, or is an attenuation of some other capability already
  in `caps`.
- The action-level property `NoAmplificationProperty` is the strictly
  stronger statement: every freshly granted cap (in `caps' \ caps`) must
  be derivable by attenuation from the prior state.
- Rust analog: `pyana_cell::is_attenuation` callsites in
  `turn/src/executor.rs:4265` and `:5980`.

## What is deliberately abstracted

This spec is the first increment. The following are not modeled here:

- **BLAKE3.** Treated as the injective tuple constructor `<<pk, tid>>`.
- **Cryptographic auth.** No signatures, no proofs. The lattice rule is
  about which auth would suffice, not about verifying any particular signature
  or proof.
- **The action axis of `Permissions`** (send / receive / set_state /
  set_permissions / set_verification_key / increment_nonce / delegate /
  access). Each action has its own `AuthRequired`, but they all use the same
  lattice — modeling one axis suffices for the lattice invariant. A future
  increment can split per-action.
- **Effect VM, receipts, journal, conflict / fast-path / eventual semantics.**
  These are large enough to deserve their own spec module(s).
- **Balances, value commitments, notes, nullifiers, escrow, obligations,
  bridge effects.** Conservation laws for these are the obvious next spec.
- **Facets (`EffectMask`), expiry, breadstuff tokens.** Faceted attenuation
  is a strictly stronger version of the same lattice rule and would extend
  `IsAttenuation` to a pair `(authNarrower, maskSubset)`.
- **Sovereign vs hosted mode, programs, verification keys.**
- **CapTP-level chaining, three-party introduction, delegated refs and
  staleness.** These are causal-soundness questions, not lattice questions.

The spec also abstracts the *checking* of identity integrity by treating
`DeriveId` as the canonical constructor: a Rust bug where `cell.id`
diverges from `derive_raw(pk, tid)` cannot be expressed in this model. To
catch such a bug, a later increment would model `id` as an independent
field and add an action `CorruptId(id, newId)` that the invariant must
forbid.

## How to run TLC

The deliverable is the spec text plus this README; running TLC is optional.
If you have the TLA+ tools installed:

```sh
# Get the tools (one-time):
#   https://github.com/tlaplus/tlaplus/releases
#   download tla2tools.jar

# Model-check (from the repo root):
java -cp /path/to/tla2tools.jar tlc2.TLC \
    -workers auto \
    -config spec/CellModel.cfg \
    spec/CellModel.tla
```

You should see TLC report something like:

```
Model checking completed. No error has been found.
  Estimates of the probability that TLC did not check all reachable states
  ...
```

The state space under the cfg constants (2 public keys, 2 token ids,
MaxNonce = 2, MaxTurns = 3, |caps| <= 6) is small enough to enumerate fully
on a laptop in seconds.

## Sanity check: showing the invariants bite

Two ways to convince yourself the invariants actually constrain behavior:

1. **Remove the precondition in `Redelegate`.** Delete the
   `IsAttenuation(fromPerm, narrowerPerm)` line. TLC will report the
   `Invariant` (which includes `AttenuationSoundness`) violated and
   produce a short counterexample where a `Signature`-restricted cap is
   re-delegated as a `None`-restricted (more permissive) cap. We verified
   this manually: TLC finds the counterexample in <1s at depth 3.

2. **Change `SuccessfulTurn` to allow `providedNonce <= cells[id].nonce`.**
   TLC will report `MonotonicNonce` violated and produce a trace where the
   same nonce is used twice (replay) and the ledger nonce decreases.

## Next spec increments

In rough priority order:

1. **Balance conservation.** Add a `balance` field to `CellRecord`, a
   `Transfer(from, to, amount)` action, and the invariant
   `sum(cells[i].balance) = initial total` (modulo mint/burn actions, if any).
2. **Receipt-chain causal soundness.** Model `RECEIPT_CHAIN` as a sequence
   per cell with hash linkage, and assert: the parent hash of receipt `n+1`
   equals the hash of receipt `n`. Add `Reorder` and `Replay` adversary
   actions; show they violate the invariant.
3. **Per-action permission split.** Replace single `perm` with the full
   8-axis `Permissions` record; encode `Effect::SetPermissions`'s "applied
   last" rule and prove an action cannot weaken its own permission check.
4. **Facet attenuation.** Add `EffectMask` as a bitset over a small finite
   universe; extend `IsAttenuation` to require subset on the mask.
5. **CapTP three-party introduction.** Two cells already holding caps to
   each other; introduce a third. Show the introduction can only grant
   attenuated rights and cannot forge a cap to a cell the introducer
   doesn't hold.
6. **Sovereign cell upgrade.** Model `SetVerificationKey` with
   `AuthRequired = Proof` and assert that pre-image VK is bound to
   post-image VK by the upgrade proof statement.
7. **Effect VM.** Once the above are stable, lift to a small operational
   semantics that processes an `Effect` list end-to-end. This is the
   biggest piece and probably wants its own module
   (`spec/EffectVM.tla`).

## File map

- `CellModel.tla` — the spec.
- `CellModel.cfg` — TLC configuration for the smallest reasonable model.
- `README.md` — this file.

## Reading the spec against the Rust

| TLA+                            | Rust                                                   |
|----------------------------------|---------------------------------------------------------|
| `DeriveId(pk, tid)`              | `CellId::derive_raw` in `types/src/lib.rs`              |
| `IsNarrowerOrEqual`              | `AuthRequired::is_narrower_or_equal` in `cell/src/permissions.rs` |
| `IsAttenuation`                  | `is_attenuation` in `cell/src/capability.rs`            |
| `CreateCell` action              | `Effect::CreateCell` in `turn/src/action.rs`            |
| `SuccessfulTurn`                 | `Effect::IncrementNonce` + nonce check in `turn/src/executor.rs` |
| `GrantFromOwn` / `Redelegate`    | `Effect::GrantCapability`, `CapabilitySet::attenuate` in `cell/src/capability.rs` |
| `Revoke`                         | `Effect::RevokeCapability` in `turn/src/action.rs`      |
| `IdentityIntegrity` invariant    | `pub(crate) id`/`public_key`/`token_id` sealing in `cell/src/cell.rs` |
| `MonotonicNonce` property        | nonce-replay rejection in `turn/src/executor.rs`        |
| `AttenuationSoundness` invariant | `is_attenuation` callsites in `turn/src/executor.rs`    |
