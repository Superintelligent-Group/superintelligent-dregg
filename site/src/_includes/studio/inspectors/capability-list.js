/**
 * <pyana-capability-list agent="N"> — capabilities held by an agent.
 *
 * Reads `get_capability_tree(handle, agent_index)` from wasm.
 */

import { InspectorBase, shortHex } from './_base.js';

class PyanaCapabilityList extends InspectorBase {
  static get observedAttributes() { return ['uri', 'mode', 'agent']; }
  _render() {
    const { h, render, html, effect } = this._api;
    if (this._dispose) { this._dispose(); this._dispose = null; }
    this.replaceChildren();

    const agentAttr = this.getAttribute('agent');
    if (agentAttr == null) {
      this.innerHTML = `<div class="pyana-inspector pyana-inspector--err">&lt;pyana-capability-list&gt; requires agent="N"</div>`;
      return;
    }
    const agentIdx = Number(agentAttr);
    const sig = this._runtime.listCapabilities(agentIdx);
    const root = document.createElement('div');
    this.appendChild(root);

    const Component = () => {
      const tree = sig.value;
      if (!tree) return html`<div class="pyana-inspector pyana-inspector--empty">no capability tree for agent #${agentIdx}</div>`;
      const caps = tree.capabilities || [];
      if (!caps.length) return html`
        <div class="pyana-inspector pyana-inspector--cell-list">
          <header>0 capabilities (agent ${tree.agent_name || `#${agentIdx}`})</header>
        </div>`;
      return html`
        <div class="pyana-inspector pyana-inspector--cell-list">
          <header>
            ${caps.length} capabilit${caps.length === 1 ? 'y' : 'ies'}
            · ${tree.agent_name || `agent #${agentIdx}`}
            · cell <code title=${tree.cell_id}>${shortHex(tree.cell_id)}</code>
          </header>
          <ul>
            ${caps.map(c => html`
              <li><pyana-capability uri=${`pyana://capability/${agentIdx}/${c.slot}`} mode="compact"></pyana-capability></li>
            `)}
          </ul>
        </div>`;
    };
    this._dispose = effect(() => { render(h(Component, {}), root); });
  }
}
if (!customElements.get('pyana-capability-list')) customElements.define('pyana-capability-list', PyanaCapabilityList);
