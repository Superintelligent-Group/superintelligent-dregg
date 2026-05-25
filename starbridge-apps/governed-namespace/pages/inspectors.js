// starbridge-apps/governed-namespace/pages/inspectors.js
//
// Web components for the starbridge-governed-namespace app:
//
//   <pyana-namespace uri="...">             — browse view (version + committee summary)
//   <pyana-namespace-route-table uri="..."> — DFA route table visualization
//   <pyana-namespace-proposal uri="...">    — propose / vote / commit UI
//   <pyana-namespace-dispatch uri="...">    — lookup form (input path → classified target)
//
// All four components resolve URIs through `window.pyana` (the in-browser
// PyanaRuntime — see wasm/src/runtime.rs) and produce signed turns via
// `window.pyana.signTurn(turnSpec)` (the extension wallet API — see
// extension/src/page.ts). No app-domain enforcement runs here; the
// cell-program (`governance_program` in src/lib.rs) is the enforcement
// loop. The web components only assemble turn specs and render state.
//
// Slot indices mirror the constants in `src/lib.rs`. Keep in sync:
//   ROUTE_TABLE_ROOT_SLOT          = 0
//   VERSION_SLOT                   = 1
//   GOVERNANCE_COMMITTEE_ROOT_SLOT = 2
//   THRESHOLD_SLOT                 = 3
//   DISPUTE_WINDOW_HEIGHT_SLOT     = 4
//   PENDING_PROPOSAL_ROOT_SLOT     = 5

const ROUTE_TABLE_ROOT_SLOT = 0;
const VERSION_SLOT = 1;
const GOVERNANCE_COMMITTEE_ROOT_SLOT = 2;
const THRESHOLD_SLOT = 3;
const DISPUTE_WINDOW_HEIGHT_SLOT = 4;
const PENDING_PROPOSAL_ROOT_SLOT = 5;

// Method names — must match the Rust `symbol(...)` arguments.
const METHOD_PROPOSE = 'propose_table_update';
const METHOD_VOTE = 'vote_on_proposal';
const METHOD_COMMIT = 'commit_table_update';
const METHOD_REGISTER = 'register_service';

// Vote-kind tag bytes — matches `VoteKind::tag_field()` in src/lib.rs.
const VOTE_TAG_APPROVE = 1;
const VOTE_TAG_REJECT = 2;

function u64BE(n) {
  // Big-endian-padded 32-byte field element. Matches `u64_field` in
  // src/lib.rs and pyana_cell::program::field_from_u64_be.
  const out = new Uint8Array(32);
  let v = BigInt(n);
  for (let i = 31; i >= 24 && v > 0n; i--) {
    out[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return out;
}

function fieldToU64BE(bytes) {
  let v = 0n;
  for (let i = 24; i < 32; i++) {
    v = (v << 8n) | BigInt(bytes[i] ?? 0);
  }
  return Number(v);
}

function hex(bytes) {
  return Array.from(bytes ?? [], (b) => b.toString(16).padStart(2, '0')).join('');
}

function fieldShort(bytes) {
  const h = hex(bytes);
  if (h.length <= 12) return h;
  return `${h.slice(0, 8)}…${h.slice(-4)}`;
}

function isZero(bytes) {
  return Array.from(bytes ?? []).every((b) => b === 0);
}

async function blake3Field(input) {
  // Hash arbitrary bytes/text to a 32-byte FieldElement via BLAKE3.
  // Falls back to a SHA-256 stub if the runtime exposes no BLAKE3
  // (the in-browser runtime should always provide window.pyana.blake3).
  if (window.pyana?.blake3) {
    return window.pyana.blake3(input);
  }
  const enc = typeof input === 'string' ? new TextEncoder().encode(input) : input;
  const buf = await crypto.subtle.digest('SHA-256', enc);
  return new Uint8Array(buf);
}

// =========================================================================
// <pyana-namespace> — browse view
// =========================================================================

class NamespaceInspector extends HTMLElement {
  static get observedAttributes() {
    return ['uri'];
  }
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this._state = null;
    this._error = null;
  }
  connectedCallback() {
    this.render();
    this.refresh();
  }
  attributeChangedCallback() {
    this.refresh();
  }
  async refresh() {
    const uri = this.getAttribute('uri');
    if (!uri || !window.pyana?.readCell) return;
    try {
      const cell = await window.pyana.readCell(uri);
      this._state = cell?.state ?? null;
      this._error = null;
    } catch (e) {
      this._error = String(e);
    }
    this.render();
  }
  render() {
    const f = this._state?.fields;
    const tableRoot = f ? fieldShort(f[ROUTE_TABLE_ROOT_SLOT]) : '—';
    const version = f ? fieldToU64BE(f[VERSION_SLOT]) : '—';
    const committee = f ? fieldShort(f[GOVERNANCE_COMMITTEE_ROOT_SLOT]) : '—';
    const threshold = f ? fieldToU64BE(f[THRESHOLD_SLOT]) : '—';
    const window_h = f ? fieldToU64BE(f[DISPUTE_WINDOW_HEIGHT_SLOT]) : '—';
    const pending = f
      ? isZero(f[PENDING_PROPOSAL_ROOT_SLOT])
        ? '(none)'
        : fieldShort(f[PENDING_PROPOSAL_ROOT_SLOT])
      : '—';
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: block; font-family: monospace; padding: 1em; }
        dl { display: grid; grid-template-columns: max-content 1fr; gap: 0.4em 1em; }
        dt { font-weight: bold; }
        .error { color: #b00; }
      </style>
      <h2>Governed Namespace</h2>
      ${this._error ? `<div class="error">${this._error}</div>` : ''}
      <dl>
        <dt>route_table_root</dt><dd>${tableRoot}</dd>
        <dt>version</dt><dd>${version}</dd>
        <dt>governance_committee_root</dt><dd>${committee}</dd>
        <dt>threshold</dt><dd>${threshold}</dd>
        <dt>dispute_window_height</dt><dd>${window_h}</dd>
        <dt>pending_proposal_root</dt><dd>${pending}</dd>
      </dl>
    `;
  }
}

// =========================================================================
// <pyana-namespace-route-table> — DFA route table visualization
// =========================================================================
//
// Reads the cell's route table from the runtime's blob store (the
// route table is content-addressed under route_table_root; the
// runtime resolves the root to its serialized bytes), then renders
// each (path-prefix, target) pair as a row in a table. Falls back to
// an empty-state UI when the cell is fresh (route_table_root == ZERO).

class RouteTableInspector extends HTMLElement {
  static get observedAttributes() {
    return ['uri'];
  }
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this._table = null;
    this._error = null;
  }
  connectedCallback() {
    this.render();
    this.refresh();
  }
  attributeChangedCallback() {
    this.refresh();
  }
  async refresh() {
    const uri = this.getAttribute('uri');
    if (!uri || !window.pyana?.readCell) return;
    try {
      const cell = await window.pyana.readCell(uri);
      const root = cell?.state?.fields?.[ROUTE_TABLE_ROOT_SLOT];
      if (!root || isZero(root)) {
        this._table = { routes: [] };
      } else if (window.pyana.resolveRouteTable) {
        // The runtime exposes a helper that resolves the
        // route_table_root to a `{ routes: [(path, target)...] }`
        // serializable form. If unavailable, the component shows the
        // root hash only.
        this._table = await window.pyana.resolveRouteTable(root);
      } else {
        this._table = { root_hex: hex(root), routes: null };
      }
      this._error = null;
    } catch (e) {
      this._error = String(e);
    }
    this.render();
  }
  render() {
    const rows =
      this._table?.routes?.length > 0
        ? this._table.routes
            .map(
              (r) =>
                `<tr><td>${r.path ?? r[0]}</td><td>${describeTarget(r.target ?? r[1])}</td></tr>`,
            )
            .join('')
        : `<tr><td colspan="2"><em>${this._table?.root_hex ? `root=${this._table.root_hex.slice(0, 16)}… (no resolver)` : 'empty route table'}</em></td></tr>`;
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: block; font-family: monospace; padding: 1em; }
        table { width: 100%; border-collapse: collapse; }
        th, td { text-align: left; padding: 0.3em 0.6em; border-bottom: 1px solid #ddd; }
        .error { color: #b00; }
      </style>
      <h2>Route Table</h2>
      ${this._error ? `<div class="error">${this._error}</div>` : ''}
      <table>
        <thead><tr><th>path</th><th>target</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    `;
  }
}

function describeTarget(target) {
  if (!target) return '—';
  if (target.Handler !== undefined) return `Handler(${target.Handler})`;
  if (target.Drop !== undefined) return 'Drop';
  if (target.Federation !== undefined) {
    const id = target.Federation.group_id || target.Federation;
    return `Federation(${hex(id).slice(0, 12)}…)`;
  }
  if (target.Userspace !== undefined) {
    return `Userspace(${target.Userspace.kind}, ${target.Userspace.payload?.length ?? 0}b)`;
  }
  return JSON.stringify(target);
}

// =========================================================================
// <pyana-namespace-proposal> — propose / vote / commit UI
// =========================================================================
//
// Three sub-forms wired to the four turn-builders in src/lib.rs:
//   1. Propose: enter a JSON route-table spec + description + dispute
//      window height; emits a propose_table_update turn.
//   2. Vote: enter a proposal-root hex + Approve/Reject; emits a
//      vote_on_proposal turn.
//   3. Commit: enter the threshold-sig bytes (hex); emits a
//      commit_table_update turn with Authorization::Custom carrying
//      the WitnessedPredicate { kind: Custom { vk_hash: GOVERNANCE_VK } }.

class ProposalInspector extends HTMLElement {
  static get observedAttributes() {
    return ['uri'];
  }
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this._error = null;
    this._receipt = null;
  }
  connectedCallback() {
    this.render();
  }
  attributeChangedCallback() {
    this.render();
  }
  render() {
    const uri = this.getAttribute('uri') ?? '';
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: block; font-family: monospace; padding: 1em; }
        section { border: 1px solid #ddd; padding: 0.8em; margin-bottom: 0.8em; }
        h3 { margin-top: 0; }
        label { display: block; margin: 0.3em 0; }
        textarea, input { width: 100%; box-sizing: border-box; font-family: monospace; }
        button { padding: 0.4em 1em; cursor: pointer; }
        .error { color: #b00; white-space: pre-wrap; }
        .ok { color: #060; }
      </style>
      <h2>Governance</h2>
      <p>Target: <code>${uri || '(no uri attribute set)'}</code></p>

      <section>
        <h3>Propose route-table update</h3>
        <label>Proposed routes (JSON):
          <textarea id="propose-routes" rows="4">[
  {"path": "/public/*", "target": {"Handler": "public"}},
  {"path": "/treasury/*", "target": {"Handler": "treasury"}}
]</textarea>
        </label>
        <label>Description: <input id="propose-desc" value="Add /public + /treasury routes"/></label>
        <label>Dispute window height: <input id="propose-window" type="number" value="1000"/></label>
        <button id="propose-btn">Submit proposal</button>
      </section>

      <section>
        <h3>Vote on pending proposal</h3>
        <label>Prior pending proposal root (hex): <input id="vote-prior" placeholder="(read from cell.fields[5])"/></label>
        <label>Vote kind:
          <select id="vote-kind">
            <option value="approve">Approve</option>
            <option value="reject">Reject</option>
          </select>
        </label>
        <label>Weight: <input id="vote-weight" type="number" value="1"/></label>
        <button id="vote-btn">Submit vote</button>
      </section>

      <section>
        <h3>Commit table update (Authorization::Custom)</h3>
        <label>Committed route table (JSON, must match the proposal):
          <textarea id="commit-routes" rows="4">[
  {"path": "/public/*", "target": {"Handler": "public"}},
  {"path": "/treasury/*", "target": {"Handler": "treasury"}}
]</textarea>
        </label>
        <label>New version: <input id="commit-version" type="number" value="1"/></label>
        <label>Governance committee root (hex): <input id="commit-committee"/></label>
        <label>Threshold-signature bytes (hex):
          <textarea id="commit-sig" rows="3"></textarea>
        </label>
        <button id="commit-btn">Submit commit (Custom auth)</button>
      </section>

      <section>
        <h3>Register service</h3>
        <label>Path: <input id="reg-path" value="/treasury/main"/></label>
        <label>Target cell URI: <input id="reg-target" value="pyana://cell/..."/></label>
        <button id="reg-btn">Register</button>
      </section>

      ${this._error ? `<div class="error">Error: ${this._error}</div>` : ''}
      ${this._receipt ? `<div class="ok">OK: receipt=${this._receipt}</div>` : ''}
    `;
    this.shadowRoot.getElementById('propose-btn').addEventListener('click', () => this.onPropose());
    this.shadowRoot.getElementById('vote-btn').addEventListener('click', () => this.onVote());
    this.shadowRoot.getElementById('commit-btn').addEventListener('click', () => this.onCommit());
    this.shadowRoot.getElementById('reg-btn').addEventListener('click', () => this.onRegister());
  }
  async _send(method, args) {
    const uri = this.getAttribute('uri');
    if (!uri) {
      this._error = 'No uri attribute set';
      this.render();
      return;
    }
    try {
      // Delegate the turn-builder construction to the loader module
      // (shared/turn-builders/index.js). The loader's
      // `namespaceTurnBuilders` resolves the JSON args into the
      // canonical Effect-shaped Action and hands it to
      // window.pyana.signTurn for wallet-side signing.
      const builders = window.pyanaTurnBuilders?.['governed-namespace'];
      if (!builders) {
        throw new Error('namespace turn-builders not loaded');
      }
      const turn = await builders[method]({ target: uri, ...args });
      const signed = await window.pyana.signTurn(turn);
      const receipt = await window.pyana.submitTurn(signed);
      this._receipt = receipt?.hash_hex ?? JSON.stringify(receipt).slice(0, 40);
      this._error = null;
    } catch (e) {
      this._error = String(e);
    }
    this.render();
  }
  async onPropose() {
    const routes = JSON.parse(this.shadowRoot.getElementById('propose-routes').value);
    const description = this.shadowRoot.getElementById('propose-desc').value;
    const dispute_window_height = Number(
      this.shadowRoot.getElementById('propose-window').value,
    );
    return this._send(METHOD_PROPOSE, { routes, description, dispute_window_height });
  }
  async onVote() {
    const prior = this.shadowRoot.getElementById('vote-prior').value;
    const kind = this.shadowRoot.getElementById('vote-kind').value;
    const weight = Number(this.shadowRoot.getElementById('vote-weight').value);
    return this._send(METHOD_VOTE, { prior_proposal_root_hex: prior, vote_kind: kind, vote_weight: weight });
  }
  async onCommit() {
    const routes = JSON.parse(this.shadowRoot.getElementById('commit-routes').value);
    const new_version = Number(this.shadowRoot.getElementById('commit-version').value);
    const committee = this.shadowRoot.getElementById('commit-committee').value;
    const sig = this.shadowRoot.getElementById('commit-sig').value;
    return this._send(METHOD_COMMIT, {
      routes,
      new_version,
      governance_committee_root_hex: committee,
      threshold_sig_hex: sig,
    });
  }
  async onRegister() {
    const path = this.shadowRoot.getElementById('reg-path').value;
    const target_uri = this.shadowRoot.getElementById('reg-target').value;
    return this._send(METHOD_REGISTER, { path, target_uri });
  }
}

// =========================================================================
// <pyana-namespace-dispatch> — lookup form (path → classified target)
// =========================================================================

class DispatchInspector extends HTMLElement {
  static get observedAttributes() {
    return ['uri'];
  }
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this._result = null;
    this._error = null;
  }
  connectedCallback() {
    this.render();
  }
  attributeChangedCallback() {
    this.render();
  }
  render() {
    const last = this._result
      ? `
        <h3>Result</h3>
        <p>target: <strong>${describeTarget(this._result.target)}</strong></p>
        <p>matched_prefix: <code>${this._result.matched_prefix ?? '(empty)'}</code></p>
        <p>remainder: <code>${this._result.remainder ?? '(empty)'}</code></p>
      `
      : '';
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: block; font-family: monospace; padding: 1em; }
        button { padding: 0.4em 1em; cursor: pointer; }
        .error { color: #b00; }
      </style>
      <h2>Dispatch lookup</h2>
      <p>Classify an input path against the live route table.</p>
      <label>Path: <input id="path" value="/treasury/transfer" style="width: 100%; font-family: monospace;"/></label>
      <button id="go">Classify</button>
      ${this._error ? `<div class="error">${this._error}</div>` : ''}
      ${last}
    `;
    this.shadowRoot.getElementById('go').addEventListener('click', () => this.onLookup());
  }
  async onLookup() {
    const uri = this.getAttribute('uri');
    const path = this.shadowRoot.getElementById('path').value;
    if (!uri) {
      this._error = 'No uri attribute set';
      this.render();
      return;
    }
    try {
      // Delegates to `window.pyana.classifyNamespacePath(cell_uri, path)`
      // — the runtime's read-side dispatch helper that walks the live
      // route table (resolved from the cell's slot 0 commitment) using
      // pyana_dfa::Router::classify_path.
      if (!window.pyana?.classifyNamespacePath) {
        throw new Error(
          'classifyNamespacePath helper not exposed by runtime; ' +
            'see starbridge-governed-namespace::dispatch helper for the equivalent server-side',
        );
      }
      this._result = await window.pyana.classifyNamespacePath(uri, path);
      this._error = null;
    } catch (e) {
      this._error = String(e);
      this._result = null;
    }
    this.render();
  }
}

// =========================================================================
// Element registration
// =========================================================================

customElements.define('pyana-namespace', NamespaceInspector);
customElements.define('pyana-namespace-route-table', RouteTableInspector);
customElements.define('pyana-namespace-proposal', ProposalInspector);
customElements.define('pyana-namespace-dispatch', DispatchInspector);

export {
  NamespaceInspector,
  RouteTableInspector,
  ProposalInspector,
  DispatchInspector,
  ROUTE_TABLE_ROOT_SLOT,
  VERSION_SLOT,
  GOVERNANCE_COMMITTEE_ROOT_SLOT,
  THRESHOLD_SLOT,
  DISPUTE_WINDOW_HEIGHT_SLOT,
  PENDING_PROPOSAL_ROOT_SLOT,
  METHOD_PROPOSE,
  METHOD_VOTE,
  METHOD_COMMIT,
  METHOD_REGISTER,
  VOTE_TAG_APPROVE,
  VOTE_TAG_REJECT,
  u64BE,
  fieldToU64BE,
  hex,
  blake3Field,
};
