/**
 * Notes view — note tree state, nullifiers, root history.
 */

import { bus, state } from '../app.js';
import * as api from '../api.js';

export const name = 'notes';

export function init(el) {
  bus.on('notes:updated', (noteData) => {
    if (state.currentPage === 'notes') renderNotes(noteData);
  });
}

export function update(appState) {
  if (appState.status) {
    renderNoteStats(appState.status, appState.checkpoint);
  }
}

export function destroy() {}

function renderNotes({ checkpoint, blocks, status }) {
  renderNoteStats(status || state.status, checkpoint);
  renderNoteRootHistory(blocks);
  renderNoteCheckpointState(checkpoint);
}

function renderNoteStats(status, checkpoint) {
  if (status) {
    document.getElementById('notes-stat-count').textContent = api.formatNumber(status.note_count);
    document.getElementById('notes-stat-revocations').textContent = api.formatNumber(status.revocation_count);
  }
  if (checkpoint) {
    document.getElementById('notes-stat-nullifier-root').textContent = api.shortHash(checkpoint.nullifier_set_root, 8, 4);
    document.getElementById('notes-stat-tree-root').textContent = api.shortHash(checkpoint.note_tree_root, 8, 4);
  }
}

function renderNoteRootHistory(blocks) {
  const container = document.getElementById('notes-root-history');
  if (!blocks || !blocks.length) {
    container.innerHTML = '<div class="empty-state">No roots attested yet</div>';
    return;
  }
  const recent = blocks.slice(-12).reverse();
  container.innerHTML = `
    <div class="note-root-list">
      ${recent.map(r => `
        <div class="root-item">
          <span class="root-item__height">#${r.height}</span>
          <span class="root-item__hash">${api.shortHash(r.merkle_root, 12, 6)}</span>
          <span class="root-item__time">${api.relativeTime(r.timestamp)}</span>
        </div>
      `).join('')}
    </div>
    <div style="margin-top: 12px; font-family: var(--mono); font-size: 10px; color: var(--text-muted);">
      Each root represents a committed note tree state. Nullifiers prevent double-spend.
    </div>
  `;
}

function renderNoteCheckpointState(checkpoint) {
  const container = document.getElementById('notes-checkpoint-state');
  if (!checkpoint) {
    container.innerHTML = '<div class="empty-state">No checkpoint available</div>';
    return;
  }
  container.innerHTML = `
    <div class="checkpoint-grid">
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Note Tree Root</div>
        <div class="checkpoint-field__value checkpoint-field__value--hash">${api.shortHash(checkpoint.note_tree_root, 12, 6)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Nullifier Set Root</div>
        <div class="checkpoint-field__value checkpoint-field__value--hash">${api.shortHash(checkpoint.nullifier_set_root, 12, 6)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Revocation Tree Root</div>
        <div class="checkpoint-field__value checkpoint-field__value--hash">${api.shortHash(checkpoint.revocation_tree_root, 12, 6)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Ledger State Root</div>
        <div class="checkpoint-field__value checkpoint-field__value--hash">${api.shortHash(checkpoint.ledger_state_root, 12, 6)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Height</div>
        <div class="checkpoint-field__value">${api.formatNumber(checkpoint.height)}</div>
      </div>
      <div class="checkpoint-field">
        <div class="checkpoint-field__label">Timestamp</div>
        <div class="checkpoint-field__value">${api.formatTime(checkpoint.timestamp)}</div>
      </div>
    </div>
    <div style="margin-top: 12px; font-family: var(--mono); font-size: 10px; color: var(--text-muted);">
      Notes carry hidden asset types. Commitments = Poseidon2(value, asset_type, blinding). Nullifiers = BLAKE3(note_commitment, nullifier_key).
    </div>
  `;
}
