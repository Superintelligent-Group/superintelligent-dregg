/**
 * Overview view — dashboard with stats cards, recent activity, checkpoint info.
 */

import { bus, state } from '../app.js';
import * as api from '../api.js';

export const name = 'overview';

let container = null;

export function init(el) {
  container = el;

  bus.on('status:updated', (status) => {
    if (state.currentPage === 'overview') renderStats(status);
  });

  bus.on('overview:updated', ({ intents, conditionals, checkpoint, blocks }) => {
    if (state.currentPage !== 'overview') return;
    renderIntentStats(intents, conditionals);
    renderCheckpoint(checkpoint);
    renderRecentRoots(blocks);
  });
}

export function update(appState) {
  if (appState.status) renderStats(appState.status);
}

export function destroy() {}

function renderStats(status) {
  if (!status) return;
  const el = (id) => document.getElementById(id);
  el('stat-height').textContent = api.formatNumber(status.latest_height);
  el('stat-peers').textContent = api.formatNumber(status.peer_count);
  el('stat-revocations').textContent = api.formatNumber(status.revocation_count);
  el('stat-notes').textContent = api.formatNumber(status.note_count);
}

function renderIntentStats(intents, conditionals) {
  document.getElementById('stat-intents').textContent = api.formatNumber(intents?.length || 0);
  document.getElementById('stat-conditionals').textContent = api.formatNumber(conditionals?.length || 0);
}

function renderCheckpoint(cp) {
  if (!cp) {
    document.getElementById('checkpoint-info').innerHTML = '<div class="empty-state">No checkpoint available</div>';
    document.getElementById('checkpoint-badge').textContent = '--';
    return;
  }
  document.getElementById('checkpoint-badge').textContent = `height ${cp.height}`;
  document.getElementById('checkpoint-info').innerHTML = `
    <div class="checkpoint-grid">
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Height</div>
        <div class="checkpoint-field__value">${api.formatNumber(cp.height)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Epoch</div>
        <div class="checkpoint-field__value">${api.formatNumber(cp.epoch)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Federation Members</div>
        <div class="checkpoint-field__value">${cp.federation_members}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">QC Votes</div>
        <div class="checkpoint-field__value">${cp.qc_votes}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Ledger Root</div>
        <div class="checkpoint-field__value checkpoint-field__value--hash">${api.shortHash(cp.ledger_state_root, 12, 6)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Note Tree</div>
        <div class="checkpoint-field__value checkpoint-field__value--hash">${api.shortHash(cp.note_tree_root, 12, 6)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Nullifier Set</div>
        <div class="checkpoint-field__value checkpoint-field__value--hash">${api.shortHash(cp.nullifier_set_root, 12, 6)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Revocation Tree</div>
        <div class="checkpoint-field__value checkpoint-field__value--hash">${api.shortHash(cp.revocation_tree_root, 12, 6)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Timestamp</div>
        <div class="checkpoint-field__value">${api.formatTime(cp.timestamp)}</div>
      </div>
    </div>
  `;
}

function renderRecentRoots(blocks) {
  const container = document.getElementById('recent-roots');
  if (!blocks || !blocks.length) {
    container.innerHTML = '<div class="empty-state">No attested roots found</div>';
    return;
  }
  const roots = blocks.slice(-10).reverse();
  container.innerHTML = roots.map(r => `
    <div class="root-item">
      <span class="root-item__height">#${r.height}</span>
      <span class="root-item__hash">${api.shortHash(r.merkle_root, 12, 6)}</span>
      <span class="root-item__sigs">${r.signatures} sigs</span>
      <span class="root-item__time">${api.relativeTime(r.timestamp)}</span>
    </div>
  `).join('');
}
