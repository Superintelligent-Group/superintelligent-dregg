/**
 * Proofs view — proof system backends with anatomy visualizer.
 */

import { bus, state, getVisualizer } from '../app.js';
import * as api from '../api.js';

export const name = 'proofs';

export function init(el) {
  bus.on('proofs:updated', (pirInfo) => {
    if (state.currentPage === 'proofs') renderProofsView(pirInfo);
  });
}

export function update() {}
export function destroy() {}

function renderProofsView(pirInfo) {
  const container = document.getElementById('proofs-list');
  container.innerHTML = `
    <div class="proof-item">
      <div class="proof-item__header">
        <span class="proof-item__type">STARK</span>
        <span class="proof-item__size">BabyBear field, Poseidon2 hash</span>
      </div>
      <div class="proof-item__details">
        <div><div class="proof-item__detail-label">Backend</div><div class="proof-item__detail-value">MerklePoseidon2StarkAir (production)</div></div>
        <div><div class="proof-item__detail-label">Uses</div><div class="proof-item__detail-value">Membership, block transition, presentation</div></div>
        <div><div class="proof-item__detail-label">Verification</div><div class="proof-item__detail-value">FRI + Fiat-Shamir + action binding</div></div>
      </div>
    </div>
    <div class="proof-item">
      <div class="proof-item__header">
        <span class="proof-item__type">Kimchi</span>
        <span class="proof-item__size">Pasta curves</span>
      </div>
      <div class="proof-item__details">
        <div><div class="proof-item__detail-label">Backend</div><div class="proof-item__detail-value">Plonkish constraint system</div></div>
        <div><div class="proof-item__detail-label">Uses</div><div class="proof-item__detail-value">Recursive verification, IVC folding</div></div>
        <div><div class="proof-item__detail-label">Status</div><div class="proof-item__detail-value">Spike phase (constraint system verified)</div></div>
      </div>
    </div>
    <div class="proof-item">
      <div class="proof-item__header">
        <span class="proof-item__type">Composed</span>
        <span class="proof-item__size">Multi-proof binding</span>
      </div>
      <div class="proof-item__details">
        <div><div class="proof-item__detail-label">Binding</div><div class="proof-item__detail-value">BLAKE3 composition commitment over sub-proofs</div></div>
        <div><div class="proof-item__detail-label">Modes</div><div class="proof-item__detail-value">sequential, parallel, recursive</div></div>
        <div><div class="proof-item__detail-label">Public Inputs</div><div class="proof-item__detail-value">pi[2..6] action binding, pi[6..10] composition commitment</div></div>
      </div>
    </div>
    ${pirInfo ? `
    <div class="proof-item">
      <div class="proof-item__header">
        <span class="proof-item__type">PIR Index</span>
        <span class="proof-item__size">${pirInfo.num_rows} rows x ${pirInfo.row_width} cols</span>
      </div>
      <div class="proof-item__details">
        <div><div class="proof-item__detail-label">Database Size</div><div class="proof-item__detail-value">${pirInfo.num_rows} capability tags</div></div>
        <div><div class="proof-item__detail-label">Row Width</div><div class="proof-item__detail-value">${pirInfo.row_width} field elements</div></div>
        <div><div class="proof-item__detail-label">Tags</div><div class="proof-item__detail-value">${pirInfo.tags.length > 0 ? pirInfo.tags.slice(0, 5).join(', ') + (pirInfo.tags.length > 5 ? '...' : '') : 'none'}</div></div>
      </div>
    </div>
    ` : ''}
    <div class="proof-anatomy-mount" id="proof-anatomy-mount">
      <div class="proof-item" style="border-color: var(--accent); background: var(--surface-2);">
        <div class="proof-item__header">
          <span class="proof-item__type" style="background: var(--accent-soft); color: var(--accent-bright);">Anatomy</span>
          <span class="proof-item__size">Paste a hex-encoded proof to inspect</span>
        </div>
        <div class="proof-anatomy-input" style="margin-top: 12px;">
          <textarea id="proof-hex-input" placeholder="Paste hex-encoded STARK proof here..." style="width: 100%; height: 60px; font-family: var(--mono); font-size: 10px; background: var(--surface-3); border: 1px solid var(--border-2); border-radius: var(--radius); color: var(--text); padding: 8px; resize: vertical;"></textarea>
          <button class="btn btn-sm btn-primary" id="btn-parse-proof" style="margin-top: 8px;">Parse Proof</button>
        </div>
        <div id="proof-anatomy-output" style="margin-top: 12px;"></div>
      </div>
    </div>
  `;

  // Wire up proof anatomy
  const parseBtn = document.getElementById('btn-parse-proof');
  if (parseBtn) {
    parseBtn.addEventListener('click', () => {
      const hex = document.getElementById('proof-hex-input').value.trim();
      if (hex) {
        const viz = getVisualizer('proof-anatomy');
        if (viz) {
          const output = document.getElementById('proof-anatomy-output');
          viz.init(output);
          viz.update({ hex });
        }
      }
    });
  }
}
