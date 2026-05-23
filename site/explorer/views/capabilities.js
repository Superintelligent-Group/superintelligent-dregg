/**
 * Capabilities view — token listing with delegation chains.
 */

import { bus, state } from '../app.js';
import * as api from '../api.js';

export const name = 'capabilities';

export function init(el) {
  bus.on('tokens:updated', (tokens) => {
    if (state.currentPage === 'capabilities') renderCapabilities(tokens);
  });
}

export function update(appState) {
  if (appState.tokens) renderCapabilities(appState.tokens);
}

export function destroy() {}

function renderCapabilities(tokens) {
  const container = document.getElementById('capabilities-list');
  if (!tokens || !tokens.length) {
    container.innerHTML = '<div class="empty-state"><div class="empty-state__icon">&#8669;</div>No capability tokens held</div>';
    return;
  }
  container.innerHTML = tokens.map(t => `
    <div class="cap-item">
      <span class="cap-item__id">${api.shortHash(t.id, 8, 4)}</span>
      <span class="cap-item__service">${t.service || 'universal'}</span>
      <span class="cap-item__badge cell-badge cell-badge--success">${t.label || 'active'}</span>
    </div>
  `).join('');
}
