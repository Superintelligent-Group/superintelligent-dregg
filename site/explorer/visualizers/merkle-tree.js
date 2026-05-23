/**
 * Merkle Tree Visualizer — interactive tree with expand/collapse,
 * proof path highlighting, and insertion animation.
 *
 * Interface: init(container), update({ root, leaves, proofPath? }), destroy()
 */

export const name = 'merkle-tree';

let _container = null;
let _state = {
  root: null,
  leaves: [],
  proofPath: null,
  expandedNodes: new Set(),
  depth: 4,
};

export function init(container) {
  _container = container;
  render();
}

export function update(data) {
  if (data.root !== undefined) _state.root = data.root;
  if (data.leaves) _state.leaves = data.leaves;
  if (data.proofPath) _state.proofPath = data.proofPath;
  if (data.depth) _state.depth = data.depth;
  render();
}

export function destroy() {
  _container = null;
  _state = { root: null, leaves: [], proofPath: null, expandedNodes: new Set(), depth: 4 };
}

function render() {
  if (!_container) return;

  const { root, leaves, proofPath, depth } = _state;

  // Build tree structure
  const tree = buildTree(leaves, depth);

  // Render as interactive SVG
  const width = _container.clientWidth || 600;
  const height = (depth + 1) * 80 + 60;

  let svg = `<svg width="${width}" height="${height}" xmlns="http://www.w3.org/2000/svg" class="merkle-tree-svg">`;

  // Render tree recursively
  svg += renderNode(tree, width / 2, 40, width / 4, 0, depth, proofPath);

  svg += `</svg>`;

  // Add controls
  _container.innerHTML = `
    <div class="merkle-tree-controls" style="display: flex; gap: 8px; margin-bottom: 12px; align-items: center;">
      <span style="font-family: var(--mono); font-size: 10px; color: var(--text-muted);">MERKLE TREE (depth ${depth})</span>
      <span style="font-family: var(--mono); font-size: 10px; color: var(--accent-bright); margin-left: auto;">${leaves.length} leaves</span>
      ${root ? `<span style="font-family: var(--mono); font-size: 10px; color: var(--info);">root: ${shortHash(root)}</span>` : ''}
    </div>
    ${svg}
    ${proofPath ? `
    <div style="margin-top: 12px; font-family: var(--mono); font-size: 10px; color: var(--text-muted);">
      <span style="color: var(--lantern);">Proof path highlighted</span> — ${proofPath.length} intermediate nodes
    </div>` : ''}
  `;

  // Wire click events for expand/collapse
  _container.querySelectorAll('.merkle-node').forEach(nodeEl => {
    nodeEl.addEventListener('click', () => {
      const nodeId = nodeEl.dataset.nodeId;
      if (_state.expandedNodes.has(nodeId)) {
        _state.expandedNodes.delete(nodeId);
      } else {
        _state.expandedNodes.add(nodeId);
      }
      render();
    });
  });
}

function buildTree(leaves, depth) {
  // Build a complete binary tree of given depth
  const totalLeaves = Math.pow(2, depth);
  const paddedLeaves = [...leaves];
  while (paddedLeaves.length < totalLeaves) {
    paddedLeaves.push(null); // empty leaf
  }

  // Build bottom-up
  let currentLevel = paddedLeaves.map((leaf, idx) => ({
    id: `leaf-${idx}`,
    hash: leaf || '0'.repeat(16),
    isLeaf: true,
    isEmpty: !leaf,
    level: depth,
    index: idx,
  }));

  const allNodes = [...currentLevel];

  for (let level = depth - 1; level >= 0; level--) {
    const nextLevel = [];
    for (let i = 0; i < currentLevel.length; i += 2) {
      const left = currentLevel[i];
      const right = currentLevel[i + 1] || { hash: '0'.repeat(16), isEmpty: true };
      const node = {
        id: `node-${level}-${i / 2}`,
        hash: hashPair(left.hash, right.hash),
        isLeaf: false,
        isEmpty: left.isEmpty && right.isEmpty,
        level: level,
        index: Math.floor(i / 2),
        left: left,
        right: right,
      };
      nextLevel.push(node);
      allNodes.push(node);
    }
    currentLevel = nextLevel;
  }

  return currentLevel[0]; // root
}

function renderNode(node, x, y, hSpread, level, maxDepth, proofPath) {
  if (!node) return '';
  let svg = '';

  const isOnPath = proofPath && proofPath.includes(node.id);
  const nodeColor = isOnPath ? 'var(--lantern)' : (node.isEmpty ? 'rgba(232,224,208,0.1)' : 'var(--accent)');
  const textColor = isOnPath ? 'var(--lantern)' : (node.isEmpty ? 'var(--text-faint)' : 'var(--text-dim)');
  const radius = node.isLeaf ? 6 : 8;

  // Draw edges to children
  if (node.left) {
    const childY = y + 70;
    const leftX = x - hSpread;
    const rightX = x + hSpread;
    const edgeColor = proofPath && proofPath.includes(node.left.id) ? 'var(--lantern)' : 'rgba(232,224,208,0.15)';
    const edgeColor2 = proofPath && proofPath.includes(node.right?.id) ? 'var(--lantern)' : 'rgba(232,224,208,0.15)';

    svg += `<line x1="${x}" y1="${y}" x2="${leftX}" y2="${childY}" stroke="${edgeColor}" stroke-width="1.5"/>`;
    svg += `<line x1="${x}" y1="${y}" x2="${rightX}" y2="${childY}" stroke="${edgeColor2}" stroke-width="1.5"/>`;

    svg += renderNode(node.left, leftX, childY, hSpread / 2, level + 1, maxDepth, proofPath);
    if (node.right) {
      svg += renderNode(node.right, rightX, childY, hSpread / 2, level + 1, maxDepth, proofPath);
    }
  }

  // Draw node
  svg += `<circle cx="${x}" cy="${y}" r="${radius}" fill="${nodeColor}"
          class="merkle-node" data-node-id="${node.id}" style="cursor: pointer;"/>`;

  // Hash label (truncated)
  const label = node.hash ? shortHash(node.hash) : '';
  svg += `<text x="${x}" y="${y - radius - 4}" text-anchor="middle"
          font-family="'JetBrains Mono', monospace" font-size="8" fill="${textColor}" pointer-events="none">${label}</text>`;

  return svg;
}

function hashPair(a, b) {
  // Simulated hash (for visualization, not crypto)
  let hash = 0;
  const str = a + b;
  for (let i = 0; i < str.length; i++) {
    hash = ((hash << 5) - hash) + str.charCodeAt(i);
    hash |= 0;
  }
  return Math.abs(hash).toString(16).padStart(16, '0').slice(0, 16);
}

function shortHash(hash) {
  if (!hash || hash.length <= 8) return hash || '';
  return hash.slice(0, 6) + '..';
}
