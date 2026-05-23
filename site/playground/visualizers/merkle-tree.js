/**
 * Merkle Tree Visualizer — playground version.
 * Interactive tree rendering with expand/collapse and proof path highlight.
 */

export const name = 'merkle-tree';

/**
 * Render an interactive Merkle tree visualization.
 *
 * @param {HTMLElement} container
 * @param {Object} data - { leaves: string[], proofPath?: string[], depth?: number }
 */
export function render(container, data) {
  const { leaves = [], proofPath = null, depth = 4 } = data;
  const width = container.clientWidth || 500;
  const height = (depth + 1) * 70 + 40;

  const tree = buildTree(leaves, depth);

  let svg = `<svg width="${width}" height="${height}" xmlns="http://www.w3.org/2000/svg">`;
  svg += renderNode(tree, width / 2, 30, width / 4, depth, proofPath);
  svg += `</svg>`;

  container.innerHTML = `
    <div style="font-family: var(--mono); font-size: 10px; color: var(--text-muted); margin-bottom: 8px;">
      Merkle Tree (depth ${depth}, ${leaves.length} leaves)
    </div>
    ${svg}
  `;
}

function buildTree(leaves, depth) {
  const totalLeaves = Math.pow(2, depth);
  const padded = [...leaves];
  while (padded.length < totalLeaves) padded.push(null);

  let level = padded.map((leaf, i) => ({ id: `l-${i}`, hash: leaf || '00000000', isEmpty: !leaf, isLeaf: true }));
  const allLevels = [level];

  for (let d = depth - 1; d >= 0; d--) {
    const next = [];
    for (let i = 0; i < level.length; i += 2) {
      const left = level[i];
      const right = level[i + 1] || { hash: '00000000', isEmpty: true };
      next.push({
        id: `n-${d}-${i / 2}`,
        hash: simHash(left.hash + right.hash),
        isEmpty: left.isEmpty && right.isEmpty,
        isLeaf: false,
        left, right,
      });
    }
    allLevels.push(next);
    level = next;
  }

  return level[0];
}

function renderNode(node, x, y, spread, depth, proofPath) {
  if (!node || depth < 0) return '';
  let svg = '';

  const isOnPath = proofPath && proofPath.includes(node.id);
  const color = isOnPath ? '#d99a3f' : (node.isEmpty ? 'rgba(232,224,208,0.1)' : '#5b8a5a');
  const r = node.isLeaf ? 5 : 7;

  if (node.left) {
    const cy = y + 60;
    const lx = x - spread;
    const rx = x + spread;
    svg += `<line x1="${x}" y1="${y}" x2="${lx}" y2="${cy}" stroke="rgba(232,224,208,0.15)" stroke-width="1"/>`;
    svg += `<line x1="${x}" y1="${y}" x2="${rx}" y2="${cy}" stroke="rgba(232,224,208,0.15)" stroke-width="1"/>`;
    svg += renderNode(node.left, lx, cy, spread / 2, depth - 1, proofPath);
    svg += renderNode(node.right, rx, cy, spread / 2, depth - 1, proofPath);
  }

  svg += `<circle cx="${x}" cy="${y}" r="${r}" fill="${color}"/>`;
  svg += `<text x="${x}" y="${y - r - 3}" text-anchor="middle" font-family="monospace" font-size="7" fill="rgba(232,224,208,0.4)">${node.hash.slice(0, 4)}</text>`;

  return svg;
}

function simHash(str) {
  let h = 0;
  for (let i = 0; i < str.length; i++) h = ((h << 5) - h) + str.charCodeAt(i) | 0;
  return Math.abs(h).toString(16).padStart(8, '0').slice(0, 8);
}
