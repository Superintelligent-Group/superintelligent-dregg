# Generalized Intent Solver: Heterogeneous Exchange Model

## Design Summary

A unified solver that finds multi-party settlements involving **any combination** of
fungible assets, capability grants, services, storage, and namespace entries. Instead of
separate codepaths for "asset rings" vs "capability matching," a single graph-based
solver treats ALL exchange items uniformly through **subjective satisfaction**.

## 1. The Generalized Exchange Model

An intent declares: "I offer {items} and want {items}" where items are drawn from a
typed union:

```rust
enum ExchangeItem {
    Asset { id: AssetId, amount: u64 },
    Capability { spec: MatchSpec, duration_epochs: u64 },
    Service { endpoint: String, invocations: u64 },
    Storage { queue_id: String, bytes: u64, duration_epochs: u64 },
    Name { namespace: String, entry: String },
}
```

Items of DIFFERENT types can appear in the same intent. A compound exchange might be:
"I offer [100 tokens, read access on oracle/*] and want [compute access, storage hosting]."

### Compatibility: Subjective Satisfaction

Following Anoma's validity predicate model, we do NOT impose a global price.
Compatibility between items is determined by **type-specific subsumption checks**:

- **Asset vs Asset**: exact AssetId match + amount >= min required
- **Capability vs Capability**: `satisfies_spec()` from matcher.rs (pattern-based)
- **Service vs Service**: endpoint match + invocations >= required
- **Storage vs Storage**: size >= required, duration >= required
- **Name vs Name**: namespace + entry exact match

Cross-type items (Asset offered vs Capability wanted) require the **participant** to
declare subjective acceptance. The solver scores edges by how many items in a want-set
are satisfiable by the counterparty's offer-set.

## 2. Valuation Model: Subjective with Scoring

Each edge A->B in the graph has a **satisfaction score** in [0.0, 1.0]:
- 1.0: A's offer fully satisfies ALL of B's wants
- Fractional: A's offer satisfies SOME of B's wants (partial coverage)
- 0.0: no overlap (no edge)

Score = (number of B's wants satisfiable by A's offers) / (total B's wants)

This is subjective because participants declare their wants; the solver merely checks
structural compatibility. A participant saying "I'll trade read-access for 100 tokens"
has implicitly declared that these are equivalent TO THEM.

## 3. Graph Construction

```
For each pair (i, j) where i != j:
    score = can_satisfy(intents[i].offering, intents[j].wanting)
    if score > 0:
        add edge i -> j with weight score
```

The `can_satisfy` function iterates over the receiver's wants and checks each against
the offerer's offerings using type-specific matching:
- Asset: id match + amount sufficient
- Capability: MatchSpec satisfaction via `resource_matches()` + `actions_match()`
- Service/Storage/Name: structural equality checks

## 4. DFA Routing Integration

Intents declare a **zone** (namespace path) that the DFA classifier routes to solver shards:

```
"/defi/*"     -> AssetRingSolver (optimized for fungible-only rings)
"/services/*" -> CapabilityBarterSolver
"/mixed/*"    -> GeneralizedCSPSolver (this design)
"/storage/*"  -> StorageMarketSolver
```

Cross-zone rings use "bridge intents" that appear in both zones. A bridge intent in
/defi/* that offers tokens wanting capability from /services/* gets duplicated into
/mixed/* where the generalized solver handles it.

The DFA router from `rbg/src/routing.rs` already classifies by byte-level patterns.
Intent zone = first path segment of the intent's resource_pattern field.

## 5. Constraint Propagation for Compound Wants

When an intent has compound wants (multiple items needed simultaneously), the solver
treats them as an AND-conjunction. A valid solution for participant B requires finding
a SET of counterparties whose combined offers cover ALL of B's wants.

For tractability:
- **Phase 1**: Find pairwise edges (can A satisfy ANY of B's wants?)
- **Phase 2**: For each cycle candidate, verify compound satisfaction (does the full
  cycle's combined offers satisfy every participant's full want-set?)

This makes compound satisfaction a verification step (cheap) rather than a search
constraint (expensive).

## 6. Proof of Valid Solution

For the STARK proof covering a generalized settlement:
1. **Asset conservation**: sum of transferred amounts is zero-sum
2. **Capability existence**: each granted capability has Merkle membership in grantor's c-list
3. **Service availability**: endpoint + invocation count verified against service registry
4. **Atomicity**: all legs settle or none do

The existing `trustless.rs` proof architecture handles this via compound turns.

## 7. Scenarios Covered

| Scenario | Items | Ring Size |
|----------|-------|-----------|
| Token swap | Asset <-> Asset | 2 |
| Service purchase | Asset <-> Service | 2 |
| Capability barter | Cap <-> Cap | 2 |
| 3-party mixed | Asset + Cap <-> Cap <-> Asset | 3 |
| Compound want | [Asset, Cap] <-> [Service, Storage] | 2+ |
| DAO governance | Name <-> Cap <-> Asset | 3 |

## 8. Implementation Path

1. `ExchangeItem` enum + `GeneralizedExchange` struct (offering/wanting)
2. `can_satisfy()` compatibility function with type-specific matching
3. `GeneralizedIntentGraph` builder using can_satisfy as edge predicate
4. Cycle detection (reuse existing Johnson's bounded DFS)
5. Compound satisfaction verification per cycle
6. Integration with DFA classifier for zone-sharded solving
