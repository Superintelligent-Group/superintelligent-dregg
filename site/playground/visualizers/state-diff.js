/**
 * State Diff Visualizer — playground version.
 * Side-by-side before/after state comparison.
 */

export const name = 'state-diff';

/**
 * Render a state diff visualization.
 *
 * @param {HTMLElement} container
 * @param {Object} data - { before: Object, after: Object }
 */
export function render(container, data) {
  const { before = {}, after = {} } = data;
  const keys = [...new Set([...Object.keys(before), ...Object.keys(after)])];

  let html = `<div style="font-family: var(--mono); font-size: 10px;">
    <div style="display: grid; grid-template-columns: 1fr 80px 80px 60px; gap: 4px; padding: 6px 8px; background: var(--surface-2); border-radius: 4px 4px 0 0; color: var(--text-muted); font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em;">
      <span>Field</span><span>Before</span><span>After</span><span>Delta</span>
    </div>`;

  keys.forEach(key => {
    const b = before[key];
    const a = after[key];
    let delta = '--';
    let cls = '';

    if (typeof b === 'number' && typeof a === 'number') {
      const d = a - b;
      delta = d > 0 ? `+${d}` : d < 0 ? `${d}` : '=';
      cls = d > 0 ? 'color: var(--accent-bright);' : d < 0 ? 'color: var(--danger);' : '';
    }

    html += `<div style="display: grid; grid-template-columns: 1fr 80px 80px 60px; gap: 4px; padding: 4px 8px; border-bottom: 1px solid var(--border);">
      <span style="color: var(--text-dim);">${key}</span>
      <span style="opacity: 0.6;">${b !== undefined ? b : '--'}</span>
      <span>${a !== undefined ? a : '--'}</span>
      <span style="${cls}">${delta}</span>
    </div>`;
  });

  html += `</div>`;
  container.innerHTML = html;
}
