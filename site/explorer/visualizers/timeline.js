/**
 * Timeline Visualizer — temporal progression visualization.
 *
 * Shows events along a time axis with markers for:
 * - Finality progression (pending -> tentative -> final)
 * - Auction phases (bid -> reveal -> settle)
 * - Block production cadence
 *
 * Interface: init(container), update({ events, span? }), destroy()
 */

export const name = 'timeline';

let _container = null;

export function init(container) {
  _container = container;
}

export function update(data) {
  if (!_container) return;
  render(data.events || [], data.span || 60);
}

export function destroy() {
  if (_container) _container.innerHTML = '';
  _container = null;
}

function render(events, spanSeconds) {
  if (!_container) return;

  if (!events.length) {
    _container.innerHTML = '<div style="padding: 12px; font-family: var(--mono); font-size: 10px; color: var(--text-muted);">No timeline events.</div>';
    return;
  }

  const width = _container.clientWidth || 600;
  const height = 120;
  const padding = 40;

  // Determine time range
  const now = Math.floor(Date.now() / 1000);
  const startTime = now - spanSeconds;

  // Filter events within range
  const visibleEvents = events.filter(e => e.timestamp >= startTime);

  let svg = `<svg width="${width}" height="${height}" xmlns="http://www.w3.org/2000/svg">`;

  // Time axis
  const axisY = height - 30;
  svg += `<line x1="${padding}" y1="${axisY}" x2="${width - padding}" y2="${axisY}" stroke="rgba(232,224,208,0.2)" stroke-width="1"/>`;

  // Time labels
  const intervals = 6;
  for (let i = 0; i <= intervals; i++) {
    const x = padding + (i / intervals) * (width - 2 * padding);
    const t = startTime + (i / intervals) * spanSeconds;
    const label = formatTimeShort(t);
    svg += `<line x1="${x}" y1="${axisY - 3}" x2="${x}" y2="${axisY + 3}" stroke="rgba(232,224,208,0.3)" stroke-width="1"/>`;
    svg += `<text x="${x}" y="${axisY + 16}" text-anchor="middle" font-family="'JetBrains Mono', monospace" font-size="8" fill="rgba(232,224,208,0.4)">${label}</text>`;
  }

  // Event markers
  const typeColors = {
    'block': '#5b8a5a',
    'finality': '#9bb87a',
    'bid': '#d99a3f',
    'reveal': '#c77ab8',
    'settle': '#6ba3c7',
    'checkpoint': '#d4685c',
    'default': 'rgba(232,224,208,0.5)',
  };

  visibleEvents.forEach((event, idx) => {
    const tNorm = (event.timestamp - startTime) / spanSeconds;
    const x = padding + tNorm * (width - 2 * padding);
    const color = typeColors[event.type] || typeColors.default;
    const y = axisY - 20 - (idx % 3) * 18; // Stagger to avoid overlap

    // Marker line
    svg += `<line x1="${x}" y1="${y + 6}" x2="${x}" y2="${axisY}" stroke="${color}" stroke-width="1" opacity="0.5"/>`;

    // Marker dot
    svg += `<circle cx="${x}" cy="${y}" r="4" fill="${color}" class="timeline-event" data-idx="${idx}" style="cursor: pointer;">
      <title>${event.label || event.type} @ ${formatTimeShort(event.timestamp)}</title>
    </circle>`;

    // Label (only for every other to avoid crowding)
    if (idx % 2 === 0 && event.label) {
      svg += `<text x="${x}" y="${y - 8}" text-anchor="middle" font-family="'JetBrains Mono', monospace" font-size="8" fill="${color}" opacity="0.8">${event.label}</text>`;
    }
  });

  svg += `</svg>`;

  _container.innerHTML = `
    <div class="timeline-viz">
      ${svg}
      <div class="timeline-legend" style="display: flex; gap: 12px; margin-top: 8px; font-family: var(--mono); font-size: 9px;">
        ${Object.entries(typeColors).filter(([k]) => k !== 'default').map(([type, color]) => `
          <span style="display: flex; align-items: center; gap: 4px;">
            <span style="width: 8px; height: 8px; border-radius: 50%; background: ${color};"></span>
            ${type}
          </span>
        `).join('')}
      </div>
    </div>
  `;
}

function formatTimeShort(ts) {
  const d = new Date(ts * 1000);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}
