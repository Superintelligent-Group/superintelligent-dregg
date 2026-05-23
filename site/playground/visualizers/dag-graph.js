/**
 * DAG Graph Visualizer — shared with explorer.
 * Re-exports from the explorer's implementation.
 *
 * For the playground, this provides the same DAG rendering capability
 * used in the blocklace simulator section.
 */

// Since the playground and explorer are sibling directories on the same site,
// we re-implement the core rendering logic here for standalone use.

export const name = 'dag-graph';

const DEFAULT_COLORS = ['#6ba3c7', '#d99a3f', '#9bb87a', '#c77ab8', '#7ac7b8'];

/**
 * Render a DAG into a container element as SVG.
 *
 * @param {HTMLElement} container
 * @param {Object} data - { nodes: [{id, height, creator, isFinal, ...}], edges: [{from, to, type}] }
 * @param {Object} opts - { nodeColors, onNodeClick, showFinality }
 */
export function render(container, data, opts = {}) {
  const { nodes, edges } = data;
  const colors = opts.nodeColors || DEFAULT_COLORS;
  const width = container.clientWidth || 600;

  if (!nodes.length) {
    container.innerHTML = '<div style="padding: 20px; text-align: center; font-family: var(--mono); font-size: 10px; color: var(--text-muted);">Empty DAG</div>';
    return;
  }

  // Layout
  const layers = {};
  nodes.forEach(n => {
    const layer = n.height || 0;
    if (!layers[layer]) layers[layer] = [];
    layers[layer].push(n);
  });

  const layerKeys = Object.keys(layers).map(Number).sort((a, b) => a - b);
  const layerCount = layerKeys.length;
  const height = Math.max(300, layerCount * 55 + 80);
  const positions = {};

  layerKeys.forEach((key, idx) => {
    const layerNodes = layers[key];
    const spacing = width / (layerNodes.length + 1);
    const y = height - 40 - idx * 50;
    layerNodes.forEach((node, nIdx) => {
      positions[node.id] = { x: spacing * (nIdx + 1), y };
    });
  });

  let svg = `<svg width="${width}" height="${height}" xmlns="http://www.w3.org/2000/svg">`;

  // Edges
  edges.forEach(edge => {
    const from = positions[edge.from];
    const to = positions[edge.to];
    if (!from || !to) return;
    const opacity = edge.type === 'reference' ? 0.15 : 0.3;
    svg += `<line x1="${from.x}" y1="${from.y}" x2="${to.x}" y2="${to.y}" stroke="rgba(232,224,208,${opacity})" stroke-width="1.5"/>`;
  });

  // Nodes
  nodes.forEach(node => {
    const pos = positions[node.id];
    if (!pos) return;
    const color = node.isEquivocator ? '#d4685c' : colors[node.creator % colors.length];
    const r = node.isFinal ? 8 : 5;
    if (node.isFinal && opts.showFinality !== false) {
      svg += `<circle cx="${pos.x}" cy="${pos.y}" r="${r + 4}" fill="none" stroke="${color}" stroke-width="1" opacity="0.4"/>`;
    }
    svg += `<circle cx="${pos.x}" cy="${pos.y}" r="${r}" fill="${color}" class="dag-node" data-id="${node.id}" style="cursor:pointer;" opacity="${node.isFinal ? 1 : 0.6}"/>`;
  });

  svg += `</svg>`;
  container.innerHTML = svg;

  if (opts.onNodeClick) {
    container.querySelectorAll('.dag-node').forEach(el => {
      el.addEventListener('click', () => {
        const node = nodes.find(n => n.id === el.dataset.id);
        if (node) opts.onNodeClick(node);
      });
    });
  }
}
