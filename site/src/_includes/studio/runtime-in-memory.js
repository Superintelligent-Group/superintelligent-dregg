/**
 * InMemoryRuntime — JS driver around the wasm PyanaRuntime handle.
 *
 * Owns one wasm runtime handle. Exposes a Runtime-shaped API (see STUDIO.md
 * § 3). All getters return Preact signals so inspectors auto-re-render when
 * the underlying state changes.
 *
 * State invalidation is push-based on mutation: every mutating call bumps an
 * internal version signal that all observed-object signals depend on. There
 * is no diff; the signals refetch on read. Coarse but correct for v0.
 *
 * Subscription/event API is not yet wired up; mutating calls fire on the
 * `_events` EventTarget if any visualizer wants to listen directly.
 */

const CAPS = Object.freeze({
  read: true,
  mutate: true,
  debug: true,
  timeTravel: false, // sim runtime is always at head; time-travel later
});

export async function createInMemoryRuntime({ wasm, signals }) {
  const { signal, computed } = signals;
  const handle = wasm.create_runtime();

  // Coarse version counter; bumped on any mutation. All cached signals depend
  // on this — reading them after a mutation triggers re-fetch.
  const version = signal(0);
  const cursor = signal(0); // block height; sim runtimes always at head for now
  const events = new EventTarget();

  function fire(type, detail) {
    events.dispatchEvent(new CustomEvent(type, { detail }));
  }
  function bump() { version.value = version.value + 1; }

  // --- Observed-object signals (cached per id) ---
  const cellCache = new Map();
  function getCell(id) {
    if (!cellCache.has(id)) {
      cellCache.set(id, computed(() => {
        version.value; // dep
        try { return wasm.get_cell_state(handle, id); }
        catch { return null; }
      }));
    }
    return cellCache.get(id);
  }

  const listCellsSignal = computed(() => {
    version.value; // dep
    return wasm.get_all_cells(handle) || [];
  });
  function listCells() { return listCellsSignal; }

  // --- Receipts -------------------------------------------------------------
  // The wasm runtime exposes only `get_receipt_chain(handle)` returning the
  // *entire* receipt chain. We cache the full chain as one signal and derive
  // per-receipt lookups from it. listReceipts(agentIdx) is currently global
  // (the chain doesn't carry agent attribution); the agentIdx arg is reserved
  // for when a per-agent filter lands in wasm.
  const receiptChainSignal = computed(() => {
    version.value;
    try { return wasm.get_receipt_chain(handle) || []; }
    catch { return []; }
  });
  function listReceipts(_agentIdx) { return receiptChainSignal; }
  const receiptCache = new Map();
  function getReceipt(turnHash) {
    if (!receiptCache.has(turnHash)) {
      receiptCache.set(turnHash, computed(() => {
        const chain = receiptChainSignal.value;
        return chain.find(r => r.turn_hash === turnHash) || null;
      }));
    }
    return receiptCache.get(turnHash);
  }
  // <pyana-turn> uses the same source-of-truth: a "turn" in this runtime is
  // identified by its turn_hash and surfaces the matching receipt.
  function getTurn(turnHash) { return getReceipt(turnHash); }

  // --- Capabilities ---------------------------------------------------------
  // Capabilities are agent-indexed (no global ID in the sim). URI form:
  //   pyana://capability/<agent_idx>/<token_idx>
  const capTreeCache = new Map();
  function listCapabilities(agentIdx) {
    const key = String(agentIdx);
    if (!capTreeCache.has(key)) {
      capTreeCache.set(key, computed(() => {
        version.value;
        try { return wasm.get_capability_tree(handle, Number(agentIdx)) || null; }
        catch { return null; }
      }));
    }
    return capTreeCache.get(key);
  }
  function getCapability(agentIdx, slotOrIndex) {
    // We don't cache per-cap separately; this is a thin derivation over the
    // agent's tree signal. Returns a computed that finds by slot first, falling
    // back to position index.
    return computed(() => {
      const tree = listCapabilities(agentIdx).value;
      if (!tree || !tree.capabilities) return null;
      const slotNum = Number(slotOrIndex);
      const bySlot = tree.capabilities.find(c => Number(c.slot) === slotNum);
      if (bySlot) return { ...bySlot, agent_index: Number(agentIdx), agent_name: tree.agent_name, cell_id: tree.cell_id };
      const byIndex = tree.capabilities[slotNum];
      if (byIndex) return { ...byIndex, agent_index: Number(agentIdx), agent_name: tree.agent_name, cell_id: tree.cell_id };
      return null;
    });
  }

  // --- Intents --------------------------------------------------------------
  // wasm has no `get_intent(idx)` getter and no `list_intents`. The runtime
  // tracks intent creation in JS-side state populated by createIntent().
  // For a v0 we keep a JS-side ledger of `{intent_id, intent_index, ...input}`
  // returned by create_intent. Match results can be fetched on demand.
  const intentLedger = []; // [{ intent_id, intent_index, agent_index, kind, ... }]
  const intentLedgerSignal = signal(0); // bumped on push
  function listIntents() {
    return computed(() => {
      intentLedgerSignal.value;
      return intentLedger.slice();
    });
  }
  function getIntent(idOrIndex) {
    return computed(() => {
      intentLedgerSignal.value;
      // try as numeric index
      const asNum = Number(idOrIndex);
      if (!Number.isNaN(asNum) && intentLedger[asNum]) return intentLedger[asNum];
      // try by id
      const byId = intentLedger.find(i => i.intent_id === idOrIndex);
      return byId || null;
    });
  }
  function matchIntent(intentIndex, agentIndex) {
    try {
      return wasm.match_intent_for_agent(handle, Number(intentIndex), Number(agentIndex));
    } catch (e) {
      return { matched: false, kind: 'error', error: String(e?.message || e) };
    }
  }

  // --- Federations + Blocks -------------------------------------------------
  // Removed from the wasm runtime: `SimFederation` was a wasm-fictional model
  // that didn't reflect `pyana_federation::{Federation, FederationNode,
  // FederationReceipt}`. Inspectors get a null signal here; they should
  // render an "awaiting pyana-federation wasm32 support" placeholder.
  // Re-light this surface when pyana-federation gains a wasm32 feature gate.
  function getFederation(_fedIdx) {
    return computed(() => null);
  }
  function getBlock(_height) {
    return computed(() => null);
  }
  function listBlocks() {
    return computed(() => []);
  }

  // --- Mutations ---
  function createAgent(name, initialBalance = 0) {
    const result = wasm.create_agent(handle, name, BigInt(initialBalance));
    bump();
    fire('agent-created', result);
    return result;
  }
  function createCell(ownerPkHex, initialBalance = 0) {
    const result = wasm.create_cell(handle, ownerPkHex, BigInt(initialBalance));
    bump();
    fire('cell-created', result);
    return result;
  }
  function executeTurn(agentIndex, actions, fee = 0) {
    const result = wasm.execute_turn(
      handle,
      agentIndex,
      JSON.stringify(actions),
      BigInt(fee),
    );
    bump();
    fire('turn-executed', { agentIndex, actions, result });
    return result;
  }
  function mintToken(agentIndex, resource, actions, expiry = 0) {
    const result = wasm.agent_mint_token(
      handle,
      agentIndex,
      resource,
      JSON.stringify(actions),
      BigInt(expiry),
    );
    bump();
    fire('token-minted', { agentIndex, result });
    return result;
  }
  function advanceHeight(blocks = 1) {
    const result = wasm.advance_height(handle, BigInt(blocks));
    cursor.value = cursor.value + Number(blocks);
    bump();
    fire('height-advanced', { blocks, result });
    return result;
  }
  function createFederation(_name, _numNodes) {
    throw new Error(
      'NotSupported: federation surface removed from wasm runtime — awaiting pyana-federation wasm32 support',
    );
  }
  function createIntent(agentIndex, kind, actions, constraints, resourcePattern, expiry = 0) {
    const result = wasm.create_intent(
      handle,
      Number(agentIndex),
      kind,
      JSON.stringify(actions || []),
      JSON.stringify(constraints || []),
      resourcePattern || '',
      BigInt(expiry),
    );
    intentLedger.push({
      ...result,
      agent_index: Number(agentIndex),
      kind,
      actions: actions || [],
      constraints: constraints || [],
      resource_pattern: resourcePattern || null,
      expiry: Number(expiry),
    });
    intentLedgerSignal.value = intentLedgerSignal.value + 1;
    bump();
    fire('intent-created', result);
    return result;
  }
  function proposeBlock(_fedIndex, _events) {
    throw new Error(
      'NotSupported: federation/block surface removed from wasm runtime — awaiting pyana-federation wasm32 support',
    );
  }

  function destroy() {
    wasm.destroy_runtime(handle);
  }

  return {
    caps: CAPS,
    source: { kind: 'sim', label: 'in-browser sim' },
    version,
    cursor,
    events,

    getCell,
    listCells,
    getReceipt,
    getTurn,
    listReceipts,
    getCapability,
    listCapabilities,
    getIntent,
    listIntents,
    matchIntent,
    getFederation,
    getBlock,
    listBlocks,

    createAgent,
    createCell,
    executeTurn,
    mintToken,
    advanceHeight,
    createFederation,
    createIntent,
    proposeBlock,

    destroy,

    // Escape hatch for the spike: direct wasm + handle access.
    // Will be removed once enough getters/mutators exist on the interface.
    _wasm: wasm,
    _handle: handle,
  };
}
