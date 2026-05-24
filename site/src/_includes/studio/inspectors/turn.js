/**
 * <pyana-turn uri="pyana://turn/<hex32>"> — single turn.
 *
 * In the sim runtime a "turn" is identified by its turn_hash; its observable
 * state is the matching TurnReceipt (pre/post state, computrons, actions).
 * Backed by the same `get_receipt_chain` lookup as <pyana-receipt>, but
 * presented as a turn (with an embedded receipt for the effects view).
 */

import { parseRef } from '../uri.js';
import { InspectorBase, renderParseError, shortHex } from './_base.js';

class PyanaTurn extends InspectorBase {
  _render() {
    const { h, render, html, effect } = this._api;
    const refAttr = this.getAttribute('uri');
    const mode = this.getAttribute('mode') || 'default';

    if (this._dispose) { this._dispose(); this._dispose = null; }
    this.replaceChildren();

    let parsed = null;
    try { parsed = parseRef(refAttr); } catch {}
    if (renderParseError(this, refAttr, parsed, 'turn')) return;

    const sig = this._runtime.getTurn(parsed.id);
    const root = document.createElement('div');
    this.appendChild(root);

    const Component = () => {
      const t = sig.value;
      if (!t) return html`<div class="pyana-inspector pyana-inspector--empty">turn not found: <code>${shortHex(parsed.id, 16)}</code></div>`;
      if (mode === 'compact') {
        return html`
          <span class="pyana-inspector pyana-inspector--compact">
            <code title=${parsed.id}>${shortHex(parsed.id)}</code>
            · ${String(t.action_count)} effects
          </span>`;
      }
      return html`
        <div class="pyana-inspector pyana-inspector--cell">
          <header>
            <span class="pyana-inspector__kind">turn</span>
            <code class="pyana-inspector__id" title=${parsed.id}>${shortHex(parsed.id, 24)}</code>
          </header>
          <dl class="pyana-inspector__kv">
            <dt>turn hash</dt><dd><code>${t.turn_hash}</code></dd>
            <dt>effects</dt><dd>${String(t.action_count)}</dd>
            <dt>computrons</dt><dd>${String(t.computrons_used)}</dd>
            <dt>timestamp</dt><dd>${String(t.timestamp)}</dd>
            <dt>state transition</dt>
            <dd>
              <code title=${t.pre_state_hash}>${shortHex(t.pre_state_hash, 12)}</code>
              → <code title=${t.post_state_hash}>${shortHex(t.post_state_hash, 12)}</code>
            </dd>
            <dt>receipt</dt>
            <dd><pyana-receipt uri=${`pyana://receipt/${t.turn_hash}`} mode="compact"></pyana-receipt></dd>
          </dl>
        </div>`;
    };
    this._dispose = effect(() => { render(h(Component, {}), root); });
  }
}
if (!customElements.get('pyana-turn')) customElements.define('pyana-turn', PyanaTurn);
