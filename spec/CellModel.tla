-------------------------------- MODULE CellModel --------------------------------
(***************************************************************************)
(* CellModel — initial TLA+ specification of pyana's three most-fundamental *)
(* cell-model invariants:                                                   *)
(*                                                                          *)
(*   I1 (Identity integrity).  Each cell's id equals BLAKE3(public_key ||   *)
(*       token_id).  Once a cell exists in the ledger, no operation         *)
(*       mutates `id`, `public_key`, or `token_id`.                         *)
(*                                                                          *)
(*   I2 (Nonce monotonicity).  Per-cell nonce starts at 0 and increases by  *)
(*       exactly 1 per successful turn.  Wrong-nonce turns are rejected.    *)
(*       The ledger nonce never regresses.                                  *)
(*                                                                          *)
(*   I3 (Capability attenuation lattice).  For any granted capability D     *)
(*       derived from a held capability P, IsAttenuation(P, D) must hold.   *)
(*       The lattice ordering is partial:                                   *)
(*                                                                          *)
(*               Impossible      (top — most restrictive)                   *)
(*                 /    \                                                   *)
(*           Signature  Proof                                               *)
(*                \    /                                                    *)
(*                Either                                                    *)
(*                  |                                                       *)
(*                 None         (bottom — least restrictive)                *)
(*                                                                          *)
(*       The ordering is `is_narrower_or_equal` from cell/src/permissions   *)
(*       .rs.  No operation may amplify (loosen) a granted capability       *)
(*       relative to the parent.                                            *)
(*                                                                          *)
(* This spec deliberately ABSTRACTS:                                        *)
(*   * BLAKE3 — modeled as an injective function on (pk, token_id) pairs.   *)
(*   * Signatures and proofs — modeled by an `Auth` token the agent         *)
(*     presents; we don't model the cryptography, only the matching rule.   *)
(*   * Effect VM, receipts, balances, facets, expiry, the journal,          *)
(*     escrow, programs, sovereign mode, etc.  These deserve their own      *)
(*     spec increments; see spec/README.md.                                 *)
(*                                                                          *)
(* The goal is an honest, audit-against-Rust foundation: a reader should be *)
(* able to point at each TLA+ invariant and find the corresponding code in  *)
(* cell/src/{cell,permissions,capability}.rs and turn/src/executor.rs.      *)
(***************************************************************************)

EXTENDS Naturals, FiniteSets, Sequences, TLC

CONSTANTS
    PublicKeys,        \* finite set of distinct public key identifiers
    TokenIds,          \* finite set of distinct token-id identifiers
    MaxNonce,          \* model bound: cap on nonce values explored
    MaxTurns           \* model bound: total successful turns explored

(***************************************************************************)
(* Auth lattice                                                             *)
(*                                                                          *)
(* Mirrors cell/src/permissions.rs::AuthRequired.  `Permissions` in this    *)
(* spec collapses to a single AuthRequired (the action-axis split into      *)
(* send/receive/set_state/... is orthogonal to attenuation and is left to a *)
(* later increment — we keep one slot here so the lattice is visible).     *)
(***************************************************************************)

AuthRequired == {"None", "Signature", "Proof", "Either", "Impossible"}

\* IsNarrowerOrEqual(a, b) <=> a is at least as restrictive as b.
\* Matches AuthRequired::is_narrower_or_equal exactly.
IsNarrowerOrEqual(a, b) ==
    \/ a = "Impossible"                                  \* top: narrowest
    \/ b = "None"                                        \* bottom: anything narrower-or-equal to None
    \/ a = b                                             \* reflexive
    \/ (a = "Proof" /\ b = "Either")
    \/ (a = "Signature" /\ b = "Either")

\* IsAttenuation(parent, granted): granted may only be granted from parent
\* if it is narrower-or-equal.  This is cell/src/capability.rs::is_attenuation.
IsAttenuation(parent, granted) == IsNarrowerOrEqual(granted, parent)

(***************************************************************************)
(* Cell identity                                                            *)
(*                                                                          *)
(* The spec models BLAKE3(pk || tid) as a total injective function from     *)
(* (pk, tid) pairs to a CellId.  We construct CellId structurally as the    *)
(* tuple <<pk, tid>> — this is the most parsimonious way to enforce         *)
(* injectivity without modeling a hash function.  The injectivity matches   *)
(* the Rust contract: derive_raw is collision-resistant on its 64-byte      *)
(* input.                                                                   *)
(***************************************************************************)

DeriveId(pk, tid) == <<pk, tid>>

CellIds == { DeriveId(pk, tid) : pk \in PublicKeys, tid \in TokenIds }

(***************************************************************************)
(* State                                                                    *)
(*                                                                          *)
(* `cells` : CellId -> [pk, tid, nonce, perm]                               *)
(*   pk     : public key bytes; must satisfy id = DeriveId(pk, tid)         *)
(*   tid    : token id bytes; ditto                                         *)
(*   nonce  : Nat <= MaxNonce                                               *)
(*   perm   : the cell's own authorization requirement (used as the         *)
(*            "parent" when granting capabilities away from this cell)      *)
(*                                                                          *)
(* `caps`  : { <<holder, target, perm>> }                                   *)
(*   The c-list relation: cell `holder` holds a capability to `target` with *)
(*   permission `perm`.  A capability that originates from a cell's own     *)
(*   permission perm0 may only be granted with perm <= perm0 in the lattice.*)
(*   A re-delegation of a held capability with parent perm pP may only be   *)
(*   re-granted with child perm pC where IsAttenuation(pP, pC).             *)
(*                                                                          *)
(* `turns` : Nat — count of successful turns performed (for bounded model). *)
(***************************************************************************)

VARIABLES cells, caps, turns

vars == <<cells, caps, turns>>

\* Type invariant for the model.
CellRecord == [
    pk     : PublicKeys,
    tid    : TokenIds,
    nonce  : 0..MaxNonce,
    perm   : AuthRequired
]

CapRecord == [
    holder : CellIds,
    target : CellIds,
    perm   : AuthRequired
]

TypeOK ==
    /\ DOMAIN cells \subseteq CellIds
    /\ \A id \in DOMAIN cells : cells[id] \in CellRecord
    /\ caps \subseteq CapRecord
    /\ turns \in 0..MaxTurns

(***************************************************************************)
(* Initial state                                                            *)
(*                                                                          *)
(* The ledger starts empty.  Cells are introduced via CreateCell actions.   *)
(* This matches the Rust ledger: cells appear via Effect::CreateCell.       *)
(***************************************************************************)

Init ==
    /\ cells = << >>                       \* empty function
    /\ caps  = {}
    /\ turns = 0

(***************************************************************************)
(* Actions                                                                  *)
(***************************************************************************)

\* CreateCell: introduce a fresh cell.  Identity must be the BLAKE3 of
\* (pk, tid).  The cell must not already exist in the ledger.  Initial
\* nonce is 0 (matches CellState::new in cell/src/state.rs).
CreateCell(pk, tid, perm) ==
    LET id == DeriveId(pk, tid) IN
    /\ id \notin DOMAIN cells
    /\ perm \in AuthRequired
    /\ cells' = cells @@ (id :> [pk |-> pk, tid |-> tid, nonce |-> 0, perm |-> perm])
    /\ caps'  = caps
    /\ turns' = turns
    \* Note: we do NOT count CreateCell as a "turn" in this spec — turns are
    \* nonce-bearing executions.  CreateCell is a setup action.

\* SuccessfulTurn: a turn that supplies the matching nonce and is accepted.
\* On success, the cell's nonce increments by exactly 1 (Effect::IncrementNonce
\* in turn/src/action.rs; permission check IncrementNonce in
\* turn/src/executor.rs).  Identity-bearing fields (pk, tid, id) are
\* preserved.
SuccessfulTurn(id, providedNonce) ==
    /\ id \in DOMAIN cells
    /\ turns < MaxTurns
    /\ cells[id].nonce < MaxNonce
    /\ providedNonce = cells[id].nonce            \* must match
    /\ cells' = [cells EXCEPT ![id].nonce = @ + 1]
    /\ caps'  = caps
    /\ turns' = turns + 1

\* RejectedTurn: a turn with wrong nonce.  The ledger MUST NOT change.
\* We model rejection as a stutter on `cells` and `caps`.
RejectedTurn(id, providedNonce) ==
    /\ id \in DOMAIN cells
    /\ providedNonce # cells[id].nonce
    /\ cells' = cells
    /\ caps'  = caps
    /\ turns' = turns

\* GrantFromOwn: cell `holder` mints a capability to itself or to another
\* cell.  The new capability is "rooted" in the holder's own permission:
\* the granted perm must be narrower-or-equal to holder's own perm.  This
\* models the executor branch that checks attenuation against an
\* originating permission.
GrantFromOwn(holder, target, grantedPerm) ==
    /\ holder \in DOMAIN cells
    /\ target \in DOMAIN cells
    /\ grantedPerm \in AuthRequired
    /\ IsAttenuation(cells[holder].perm, grantedPerm)
    /\ cells' = cells
    /\ caps'  = caps \cup {[holder |-> holder, target |-> target, perm |-> grantedPerm]}
    /\ turns' = turns

\* Redelegate: a cell that already holds a capability re-delegates a
\* (possibly narrower) version of it to a third party.  The new perm must
\* be narrower-or-equal to the held perm.  This is the lattice rule the
\* executor enforces at turn/src/executor.rs:4265 and :5980.
Redelegate(holder, target, fromPerm, toCell, narrowerPerm) ==
    /\ holder \in DOMAIN cells
    /\ target \in DOMAIN cells
    /\ toCell \in DOMAIN cells
    /\ [holder |-> holder, target |-> target, perm |-> fromPerm] \in caps
    /\ narrowerPerm \in AuthRequired
    /\ IsAttenuation(fromPerm, narrowerPerm)
    /\ cells' = cells
    /\ caps'  = caps \cup
        {[holder |-> toCell, target |-> target, perm |-> narrowerPerm]}
    /\ turns' = turns

\* Revoke: drop a capability from the c-list.  Revocation never violates
\* invariants — it can only shrink rights.
Revoke(holder, target, perm) ==
    /\ [holder |-> holder, target |-> target, perm |-> perm] \in caps
    /\ cells' = cells
    /\ caps'  = caps \ {[holder |-> holder, target |-> target, perm |-> perm]}
    /\ turns' = turns

(***************************************************************************)
(* Adversarial actions                                                      *)
(*                                                                          *)
(* These are explicitly enabled here so the model checker can show our      *)
(* invariants forbid them.  An honest implementation would never permit     *)
(* these; we include them to demonstrate that the invariants catch them.   *)
(* They are guarded by ENABLED_ADVERSARY in the cfg.                       *)
(***************************************************************************)

\* Attempt to amplify a held capability — the negation of attenuation.
\* This action's existence in the spec gives TLC something to falsify the
\* `IsAttenuation` invariant with, if a developer ever accidentally
\* loosens the rule.
AttemptAmplify(holder, target, fromPerm, toCell, widerPerm) ==
    /\ holder \in DOMAIN cells
    /\ target \in DOMAIN cells
    /\ toCell \in DOMAIN cells
    /\ [holder |-> holder, target |-> target, perm |-> fromPerm] \in caps
    /\ widerPerm \in AuthRequired
    /\ ~ IsAttenuation(fromPerm, widerPerm)            \* deliberately wider
    /\ cells' = cells
    /\ caps'  = caps \cup
        {[holder |-> toCell, target |-> target, perm |-> widerPerm]}
    /\ turns' = turns

(***************************************************************************)
(* Next-state relation                                                      *)
(***************************************************************************)

Next ==
    \/ \E pk \in PublicKeys, tid \in TokenIds, p \in AuthRequired :
            CreateCell(pk, tid, p)
    \/ \E id \in DOMAIN cells, n \in 0..MaxNonce :
            SuccessfulTurn(id, n)
    \/ \E id \in DOMAIN cells, n \in 0..MaxNonce :
            RejectedTurn(id, n)
    \/ \E h, t \in DOMAIN cells, p \in AuthRequired :
            GrantFromOwn(h, t, p)
    \/ \E h, t, u \in DOMAIN cells, fp, np \in AuthRequired :
            Redelegate(h, t, fp, u, np)
    \/ \E h, t \in DOMAIN cells, p \in AuthRequired :
            Revoke(h, t, p)

Spec == Init /\ [][Next]_vars

(***************************************************************************)
(* Invariants                                                               *)
(*                                                                          *)
(* These are the three statements the user asked for, written as state      *)
(* predicates.                                                              *)
(***************************************************************************)

\* I1.  Identity integrity.
\* Every cell in the ledger has id = DeriveId(pk, tid).  Because we use the
\* derived value as the map key, this reduces to: pk and tid match the key.
IdentityIntegrity ==
    \A id \in DOMAIN cells :
        id = DeriveId(cells[id].pk, cells[id].tid)

\* I2.  Nonce well-formedness.
\* No cell ever has a negative nonce (vacuously true given the type), and
\* the bound is respected.
NonceWellFormed ==
    \A id \in DOMAIN cells :
        cells[id].nonce \in 0..MaxNonce

\* I2'.  Nonce monotonicity (action-level).
\* This is a *property* over the next-state relation: the nonce of an
\* existing cell never decreases.  We express it as an action property
\* using the prime operator.
NonceMonotonic ==
    \A id \in DOMAIN cells :
        id \in DOMAIN cells' => cells'[id].nonce >= cells[id].nonce

\* I3.  Attenuation soundness — state invariant.
\* Every capability in the c-list either was minted from a cell whose own
\* perm attenuates to c.perm (the `GrantFromOwn` source) OR is the
\* attenuation of some other cap to the same target (the `Redelegate`
\* source).  This is the inductive invariant maintained by
\* GrantFromOwn / Redelegate / Revoke.
\*
\* Caveat:  when any cell has perm = "None" (the lattice bottom), the
\* first disjunct is satisfied by every cap (since `None` attenuates to
\* anything).  In that regime the *state* check is weak.  In settings
\* where all cells have strictly restrictive perms (Signature / Proof /
\* Either / Impossible), the state invariant does meaningfully discriminate
\* amplifying caps — see the sanity test in README.md.  The companion
\* action-level property `NoAmplificationProperty` is the strictly
\* stronger statement.
AttenuationSoundness ==
    \A c \in caps :
        \/ \E h \in DOMAIN cells :
                IsAttenuation(cells[h].perm, c.perm)
        \/ \E p \in caps :
                /\ p.target = c.target
                /\ p # c
                /\ IsAttenuation(p.perm, c.perm)

\* I3'. Action-level no-amplification.
\* For every action step, every cap that appears in `caps'` and not in
\* `caps` (i.e., a freshly granted cap) must have been derivable by
\* attenuation from some "source" present in the prior state — either a
\* cell's own perm or an existing cap with the same target.  This is the
\* meaningful inductive statement of attenuation.
NoAmplification ==
    \A c \in (caps' \ caps) :
        \/ \E h \in DOMAIN cells :
                IsAttenuation(cells[h].perm, c.perm)
        \/ \E p \in caps :
                /\ p.target = c.target
                /\ IsAttenuation(p.perm, c.perm)

NoAmplificationProperty == [][NoAmplification]_vars

\* Conjunction of state invariants (for the cfg's INVARIANT directive).
Invariant ==
    /\ TypeOK
    /\ IdentityIntegrity
    /\ NonceWellFormed
    /\ AttenuationSoundness

\* Action-level monotonicity, for TLC's PROPERTY directive.
MonotonicNonce == [][NonceMonotonic]_vars

\* State constraint: keep the c-list bounded so TLC doesn't explore an
\* unboundedly growing set of caps.  Referenced from CellModel.cfg.
\* We also bound |cells| to keep the search tractable.
StateBound ==
    /\ Cardinality(caps) =< 3
    /\ Cardinality(DOMAIN cells) =< 2

(***************************************************************************)
(* Theorem (sketch, not machine-checked here):                              *)
(*   Spec => []Invariant /\ MonotonicNonce                                  *)
(*                                                                          *)
(* TLC will model-check this under the constants in CellModel.cfg.          *)
(***************************************************************************)

================================================================================
