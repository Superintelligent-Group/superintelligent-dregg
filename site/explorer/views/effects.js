/**
 * Effects view — Effect VM state flow visualization.
 *
 * Shows state columns as a table (one row per effect), colors cells that change,
 * shows constraint satisfaction, and provides tweakers to modify values.
 */

import { bus, state, getVisualizer } from '../app.js';
import * as api from '../api.js';

export const name = 'effects';

let container = null;
let currentTrace = null;

// Demo effect types for the VM
const EFFECT_TYPES = [
  { id: 'transfer', label: 'Transfer', columns: ['sender_balance', 'receiver_balance', 'amount', 'nonce'] },
  { id: 'mint', label: 'Mint', columns: ['total_supply', 'receiver_balance', 'amount', 'authority'] },
  { id: 'burn', label: 'Burn', columns: ['total_supply', 'sender_balance', 'amount', 'nullifier'] },
  { id: 'delegate', label: 'Delegate', columns: ['delegator', 'delegate', 'capability_id', 'expiry'] },
  { id: 'revoke', label: 'Revoke', columns: ['revoker', 'revoked_id', 'tree_root', 'epoch'] },
  { id: 'create_note', label: 'Create Note', columns: ['commitment', 'asset_type', 'amount', 'blinding'] },
  { id: 'spend_note', label: 'Spend Note', columns: ['nullifier', 'note_root', 'merkle_path', 'recipient'] },
];

const CONSTRAINTS = [
  { id: 'balance_conservation', label: 'Balance Conservation', check: (row, prev) => row.sender_balance + row.receiver_balance === (prev ? prev.sender_balance + prev.receiver_balance : row.sender_balance + row.receiver_balance) },
  { id: 'non_negative', label: 'Non-Negative Balance', check: (row) => (row.sender_balance || 0) >= 0 && (row.receiver_balance || 0) >= 0 },
  { id: 'monotonic_nonce', label: 'Monotonic Nonce', check: (row, prev) => !prev || (row.nonce || 0) > (prev.nonce || 0) },
  { id: 'valid_amount', label: 'Valid Amount', check: (row) => (row.amount || 0) > 0 },
];

export function init(el) {
  container = el;

  bus.on('effects:ready', () => {
    if (state.currentPage === 'effects') renderEffectsView();
  });
}

export function update() {
  renderEffectsView();
}

export function destroy() {
  currentTrace = null;
}

function renderEffectsView() {
  const pageEl = document.getElementById('page-effects');
  if (!pageEl) return;

  const content = pageEl.querySelector('.effects-content') || pageEl;

  if (!content.querySelector('.effects-vm-container')) {
    content.innerHTML = `
      <div class="effects-vm-container">
        <div class="effects-builder">
          <div class="effects-builder__header">
            <h4 style="font-family: var(--mono); font-size: 12px; color: var(--accent-bright); margin: 0;">Effect Sequence Builder</h4>
            <div class="effects-builder__actions">
              <select id="effect-type-select" class="effects-select">
                ${EFFECT_TYPES.map(t => `<option value="${t.id}">${t.label}</option>`).join('')}
              </select>
              <button class="btn btn-sm btn-primary" id="btn-add-effect">Add Effect</button>
              <button class="btn btn-sm btn-secondary" id="btn-clear-effects">Clear</button>
              <button class="btn btn-sm btn-secondary" id="btn-example-trace">Load Example</button>
            </div>
          </div>
        </div>

        <div class="effects-trace" id="effects-trace">
          <div class="empty-state" style="padding: 20px;">
            Add effects above to build a trace, or load the example.
          </div>
        </div>

        <div class="effects-constraints" id="effects-constraints">
          <div class="effects-constraints__header">
            <h4 style="font-family: var(--mono); font-size: 11px; color: var(--text-dim); margin: 0;">CONSTRAINT SATISFACTION</h4>
          </div>
          <div class="effects-constraints__body" id="constraints-body"></div>
        </div>

        <div class="effects-tweaker" id="effects-tweaker" hidden>
          <div class="effects-tweaker__header">
            <h4 style="font-family: var(--mono); font-size: 11px; color: var(--lantern); margin: 0;">TWEAKER</h4>
            <span style="font-family: var(--mono); font-size: 10px; color: var(--text-muted);">Click any cell value to modify it</span>
          </div>
          <div class="effects-tweaker__body" id="tweaker-body"></div>
        </div>
      </div>
    `;

    wireEffectsControls();
  }
}

function wireEffectsControls() {
  document.getElementById('btn-add-effect').addEventListener('click', () => {
    const typeId = document.getElementById('effect-type-select').value;
    addEffect(typeId);
  });

  document.getElementById('btn-clear-effects').addEventListener('click', () => {
    currentTrace = null;
    renderTrace();
    renderConstraints();
  });

  document.getElementById('btn-example-trace').addEventListener('click', () => {
    loadExampleTrace();
  });
}

function addEffect(typeId) {
  if (!currentTrace) {
    currentTrace = { effects: [], columns: new Set() };
  }

  const effectType = EFFECT_TYPES.find(t => t.id === typeId);
  if (!effectType) return;

  // Add columns from this effect type
  effectType.columns.forEach(col => currentTrace.columns.add(col));

  // Generate initial values
  const prevRow = currentTrace.effects.length > 0 ? currentTrace.effects[currentTrace.effects.length - 1].values : {};
  const values = {};

  effectType.columns.forEach(col => {
    switch (col) {
      case 'sender_balance':
        values[col] = (prevRow.sender_balance || 1000) - Math.floor(Math.random() * 100 + 10);
        break;
      case 'receiver_balance':
        values[col] = (prevRow.receiver_balance || 0) + Math.floor(Math.random() * 100 + 10);
        break;
      case 'amount':
        values[col] = Math.floor(Math.random() * 100 + 10);
        break;
      case 'nonce':
        values[col] = (prevRow.nonce || 0) + 1;
        break;
      case 'total_supply':
        values[col] = (prevRow.total_supply || 10000);
        break;
      default:
        values[col] = Math.floor(Math.random() * 0xFFFFFF);
    }
  });

  currentTrace.effects.push({
    type: typeId,
    label: effectType.label,
    values,
    step: currentTrace.effects.length,
  });

  renderTrace();
  renderConstraints();
}

function loadExampleTrace() {
  currentTrace = {
    columns: new Set(['sender_balance', 'receiver_balance', 'amount', 'nonce']),
    effects: [
      { type: 'transfer', label: 'Transfer', step: 0, values: { sender_balance: 1000, receiver_balance: 0, amount: 150, nonce: 1 } },
      { type: 'transfer', label: 'Transfer', step: 1, values: { sender_balance: 850, receiver_balance: 150, amount: 75, nonce: 2 } },
      { type: 'transfer', label: 'Transfer', step: 2, values: { sender_balance: 775, receiver_balance: 225, amount: 50, nonce: 3 } },
      { type: 'mint', label: 'Mint', step: 3, values: { sender_balance: 775, receiver_balance: 325, amount: 100, nonce: 4 } },
      { type: 'transfer', label: 'Transfer', step: 4, values: { sender_balance: 675, receiver_balance: 425, amount: 100, nonce: 5 } },
    ],
  };
  renderTrace();
  renderConstraints();
}

function renderTrace() {
  const container = document.getElementById('effects-trace');
  if (!currentTrace || !currentTrace.effects.length) {
    container.innerHTML = '<div class="empty-state" style="padding: 20px;">Add effects above to build a trace, or load the example.</div>';
    document.getElementById('effects-tweaker').hidden = true;
    return;
  }

  document.getElementById('effects-tweaker').hidden = false;

  const columns = [...currentTrace.columns];

  let html = `
    <table class="effects-table">
      <thead>
        <tr>
          <th class="effects-table__step">Step</th>
          <th class="effects-table__type">Effect</th>
          ${columns.map(col => `<th class="effects-table__col">${col}</th>`).join('')}
        </tr>
      </thead>
      <tbody>
  `;

  currentTrace.effects.forEach((effect, idx) => {
    const prev = idx > 0 ? currentTrace.effects[idx - 1].values : null;
    html += `<tr class="effects-table__row" data-step="${idx}">`;
    html += `<td class="effects-table__step-cell">${idx}</td>`;
    html += `<td class="effects-table__type-cell"><span class="cell-badge cell-badge--info">${effect.label}</span></td>`;

    columns.forEach(col => {
      const val = effect.values[col];
      const prevVal = prev ? prev[col] : val;
      let cellClass = 'effects-table__val-cell';

      if (val !== undefined && prevVal !== undefined && val !== prevVal) {
        cellClass += val > prevVal ? ' effects-table__val-cell--increased' : ' effects-table__val-cell--decreased';
      }

      const displayVal = val !== undefined ? val : '--';
      html += `<td class="${cellClass}" data-step="${idx}" data-col="${col}" title="Click to modify">${displayVal}</td>`;
    });

    html += `</tr>`;
  });

  html += `</tbody></table>`;
  container.innerHTML = html;

  // Wire cell click events for tweaking
  container.querySelectorAll('.effects-table__val-cell[data-col]').forEach(cell => {
    cell.addEventListener('click', () => {
      const step = parseInt(cell.dataset.step);
      const col = cell.dataset.col;
      openTweaker(step, col);
    });
  });
}

function renderConstraints() {
  const body = document.getElementById('constraints-body');
  if (!currentTrace || !currentTrace.effects.length) {
    body.innerHTML = '<div style="padding: 8px; font-family: var(--mono); font-size: 10px; color: var(--text-muted);">No trace to check.</div>';
    return;
  }

  let html = '';
  CONSTRAINTS.forEach(constraint => {
    let allPass = true;
    let failSteps = [];

    currentTrace.effects.forEach((effect, idx) => {
      const prev = idx > 0 ? currentTrace.effects[idx - 1].values : null;
      try {
        if (!constraint.check(effect.values, prev)) {
          allPass = false;
          failSteps.push(idx);
        }
      } catch {
        // Constraint not applicable
      }
    });

    const statusBadge = allPass
      ? '<span class="cell-badge cell-badge--success">PASS</span>'
      : `<span class="cell-badge cell-badge--danger">FAIL @ step${failSteps.length > 1 ? 's' : ''} ${failSteps.join(',')}</span>`;

    html += `
      <div class="constraint-row">
        <span class="constraint-row__label">${constraint.label}</span>
        ${statusBadge}
      </div>
    `;
  });

  body.innerHTML = html;
}

function openTweaker(step, col) {
  const body = document.getElementById('tweaker-body');
  const effect = currentTrace.effects[step];
  if (!effect) return;

  const currentVal = effect.values[col] !== undefined ? effect.values[col] : 0;

  body.innerHTML = `
    <div class="tweaker-form">
      <div class="tweaker-form__field">
        <label>Step ${step}, Column: <strong>${col}</strong></label>
        <div class="tweaker-form__input-row">
          <input type="number" id="tweaker-input" value="${currentVal}" class="tweaker-form__input">
          <button class="btn btn-sm btn-primary" id="tweaker-apply">Apply</button>
        </div>
      </div>
      <div class="tweaker-form__slider">
        <input type="range" id="tweaker-slider" min="0" max="${Math.max(currentVal * 3, 10000)}" value="${currentVal}" style="width: 100%;">
      </div>
    </div>
  `;

  const input = document.getElementById('tweaker-input');
  const slider = document.getElementById('tweaker-slider');
  const applyBtn = document.getElementById('tweaker-apply');

  slider.addEventListener('input', () => {
    input.value = slider.value;
  });

  input.addEventListener('input', () => {
    slider.value = input.value;
  });

  applyBtn.addEventListener('click', () => {
    const newVal = parseInt(input.value);
    if (!isNaN(newVal)) {
      currentTrace.effects[step].values[col] = newVal;
      renderTrace();
      renderConstraints();
    }
  });
}
