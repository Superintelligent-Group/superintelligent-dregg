/**
 * State Diff Visualizer — side-by-side before/after comparison of state changes.
 *
 * Shows which fields changed, with color coding for increases/decreases.
 * Interface: init(container), update({ before, after, labels? }), destroy()
 */

export const name = 'state-diff';

let _container = null;

export function init(container) {
  _container = container;
}

export function update(data) {
  if (!_container) return;
  const { before, after, labels } = data;
  render(before, after, labels);
}

export function destroy() {
  if (_container) _container.innerHTML = '';
  _container = null;
}

function render(before, after, labels = {}) {
  if (!_container) return;

  // Collect all keys from both states
  const allKeys = [...new Set([...Object.keys(before || {}), ...Object.keys(after || {})])];

  let html = `
    <div class="state-diff">
      <div class="state-diff__header">
        <span class="state-diff__col-label">Field</span>
        <span class="state-diff__col-label">Before</span>
        <span class="state-diff__col-label">After</span>
        <span class="state-diff__col-label">Change</span>
      </div>
  `;

  allKeys.forEach(key => {
    const bVal = before ? before[key] : undefined;
    const aVal = after ? after[key] : undefined;
    const label = labels[key] || key;

    let changeClass = '';
    let changeText = '--';

    if (bVal !== undefined && aVal !== undefined) {
      if (typeof bVal === 'number' && typeof aVal === 'number') {
        const diff = aVal - bVal;
        if (diff > 0) {
          changeClass = 'state-diff__row--increased';
          changeText = `+${diff}`;
        } else if (diff < 0) {
          changeClass = 'state-diff__row--decreased';
          changeText = `${diff}`;
        } else {
          changeText = '=';
        }
      } else if (bVal !== aVal) {
        changeClass = 'state-diff__row--changed';
        changeText = 'modified';
      } else {
        changeText = '=';
      }
    } else if (bVal === undefined) {
      changeClass = 'state-diff__row--added';
      changeText = 'added';
    } else {
      changeClass = 'state-diff__row--removed';
      changeText = 'removed';
    }

    html += `
      <div class="state-diff__row ${changeClass}">
        <span class="state-diff__field">${label}</span>
        <span class="state-diff__value state-diff__value--before">${formatValue(bVal)}</span>
        <span class="state-diff__value state-diff__value--after">${formatValue(aVal)}</span>
        <span class="state-diff__change">${changeText}</span>
      </div>
    `;
  });

  html += `</div>`;
  _container.innerHTML = html;
}

function formatValue(val) {
  if (val === undefined) return '<span style="color: var(--text-faint);">--</span>';
  if (typeof val === 'number') return val.toLocaleString();
  if (typeof val === 'string' && val.length > 20) return val.slice(0, 8) + '...' + val.slice(-4);
  return String(val);
}
