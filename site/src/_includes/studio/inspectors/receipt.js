/**
 * <pyana-receipt uri="pyana://receipt/<hex32>"> — single TurnReceipt.
 *
 * The wasm sim exposes only `get_receipt_chain(handle)` returning the entire
 * chain; the JS runtime caches it and we look up by turn_hash.
 *
 * Receipt shape (from wasm/src/bindings.rs::get_receipt_chain):
 *   { turn_hash, pre_state_hash, post_state_hash, timestamp,
 *     computrons_used, action_count }
 */

import { parseRef } from '../uri.js';
import { InspectorBase, renderParseError, shortHex } from './_base.js';

class PyanaReceipt extends InspectorBase {
  _render() {
    const { h, render, html, effect } = this._api;
    const refAttr = this.getAttribute('uri');
    const mode = this.getAttribute('mode') || 'default';

    if (this._dispose) { this._dispose(); this._dispose = null; }
    this.replaceChildren();

    let parsed = null;
    try { parsed = parseRef(refAttr); } catch {}
    if (renderParseError(this, refAttr, parsed, 'receipt')) return;

    const sig = this._runtime.getReceipt(parsed.id);
    const root = document.createElement('div');
    this.appendChild(root);

    const Component = () => {
      const r = sig.value;
      if (!r) return html`<div class="pyana-inspector pyana-inspector--empty">receipt not found: <code>${shortHex(parsed.id, 16)}</code></div>`;
      if (mode === 'compact') {
        return html`
          <span class="pyana-inspector pyana-inspector--compact">
            <code title=${parsed.id}>${shortHex(parsed.id)}</code>
            · ${String(r.action_count)} actions
            · ${String(r.computrons_used)} comp
          </span>`;
      }
      return html`
        <div class="pyana-inspector pyana-inspector--cell">
          <header>
            <span class="pyana-inspector__kind">receipt</span>
            <code class="pyana-inspector__id" title=${parsed.id}>${shortHex(parsed.id, 24)}</code>
          </header>
          <dl class="pyana-inspector__kv">
            <dt>turn hash</dt><dd><code>${r.turn_hash}</code></dd>
            <dt>pre state</dt><dd><code>${r.pre_state_hash}</code></dd>
            <dt>post state</dt><dd><code>${r.post_state_hash}</code></dd>
            <dt>timestamp</dt><dd>${String(r.timestamp)}</dd>
            <dt>computrons</dt><dd>${String(r.computrons_used)}</dd>
            <dt>actions</dt><dd>${String(r.action_count)}</dd>
          </dl>
        </div>`;
    };

    this._dispose = effect(() => { render(h(Component, {}), root); });
  }
}
if (!customElements.get('pyana-receipt')) customElements.define('pyana-receipt', PyanaReceipt);
