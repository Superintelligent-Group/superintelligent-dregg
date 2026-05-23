/**
 * Blocklace view — interactive DAG visualization of the blocklace consensus.
 *
 * Shows blocks as nodes (colored by creator), predecessor edges, finality levels,
 * wave numbers, and equivocation detection.
 */

import { bus, state, getVisualizer } from '../app.js';
import * as api from '../api.js';

export const name = 'blocklace';

let container = null;
let dagViz = null;

export function init(el) {
  container = el;

  // Inject DOM for this view (since it's new, not in original HTML)
  if (!document.getElementById('page-blocklace')) {
    // The page element should be added to index.html
    // For now, work with the existing structure by finding or creating
  }

  bus.on('blocks:updated', (blocks) => {
    if (state.currentPage === 'blocklace') {
      renderBlocklace(blocks);
    }
  });
}

export function update(appState) {
  if (appState.blocks) renderBlocklace(appState.blocks);
}

export function destroy() {
  if (dagViz && dagViz.destroy) dagViz.destroy();
  dagViz = null;
}

function renderBlocklace(blocks) {
  const pageEl = document.getElementById('page-blocklace');
  if (!pageEl) return;

  const content = pageEl.querySelector('.blocklace-content') || pageEl;

  if (!blocks || !blocks.length) {
    content.innerHTML = `
      <div class="empty-state">
        <div class="empty-state__icon">&#9632;</div>
        No blocks in the DAG yet. Waiting for consensus...
      </div>`;
    return;
  }

  // Build DAG data structure from blocks
  const dagData = buildDagData(blocks);

  // Render controls and visualization container
  if (!pageEl.querySelector('.blocklace-viz-container')) {
    content.innerHTML = `
      <div class="blocklace-controls">
        <div class="blocklace-controls__left">
          <button class="btn btn-sm btn-secondary blocklace-btn" id="dag-zoom-in" title="Zoom in">+</button>
          <button class="btn btn-sm btn-secondary blocklace-btn" id="dag-zoom-out" title="Zoom out">-</button>
          <button class="btn btn-sm btn-secondary blocklace-btn" id="dag-fit" title="Fit to view">Fit</button>
          <span class="blocklace-controls__info">${dagData.nodes.length} blocks, ${dagData.edges.length} edges</span>
        </div>
        <div class="blocklace-controls__right">
          <label class="blocklace-controls__toggle">
            <input type="checkbox" id="dag-show-finality" checked>
            <span>Show finality</span>
          </label>
          <label class="blocklace-controls__toggle">
            <input type="checkbox" id="dag-animate">
            <span>Animate</span>
          </label>
        </div>
      </div>
      <div class="blocklace-viz-container" id="blocklace-viz"></div>
      <div class="blocklace-detail" id="blocklace-detail" hidden>
        <div class="blocklace-detail__content" id="blocklace-detail-content"></div>
      </div>
      <div class="blocklace-legend">
        <div class="blocklace-legend__item"><span class="blocklace-legend__dot" style="background: #6ba3c7;"></span> Node 0</div>
        <div class="blocklace-legend__item"><span class="blocklace-legend__dot" style="background: #d99a3f;"></span> Node 1</div>
        <div class="blocklace-legend__item"><span class="blocklace-legend__dot" style="background: #9bb87a;"></span> Node 2</div>
        <div class="blocklace-legend__item"><span class="blocklace-legend__dot" style="background: var(--danger);"></span> Equivocator</div>
        <div class="blocklace-legend__item"><span class="blocklace-legend__dot blocklace-legend__dot--ring"></span> Final Leader</div>
      </div>
    `;
  }

  // Get or create DAG visualizer
  const vizMod = getVisualizer('dag-graph');
  if (vizMod) {
    const vizContainer = document.getElementById('blocklace-viz');
    if (!dagViz) {
      dagViz = vizMod.create(vizContainer, {
        onNodeClick: (node) => showBlockDetail(node),
        nodeColors: ['#6ba3c7', '#d99a3f', '#9bb87a', '#c77ab8', '#7ac7b8'],
        showFinality: true,
      });
    }
    dagViz.update(dagData);
  } else {
    // Fallback: render as SVG directly
    renderDagSvg(dagData);
  }

  // Wire controls
  wireControls(dagData);
}

function buildDagData(blocks) {
  const nodes = [];
  const edges = [];

  // Sort by height
  const sorted = [...blocks].sort((a, b) => a.height - b.height);

  sorted.forEach((block, idx) => {
    const creatorId = block.creator_id || (idx % 3);
    nodes.push({
      id: `block-${block.height}`,
      height: block.height,
      hash: block.merkle_root,
      creator: creatorId,
      signatures: block.signatures,
      timestamp: block.timestamp,
      isFinalLeader: block.signatures >= 2, // 2f+1 threshold for 3 nodes
      isEquivocator: block.equivocator || false,
      wave: Math.floor(block.height / 3),
      finality: block.signatures >= 2 ? 'final' : block.signatures >= 1 ? 'tentative' : 'pending',
    });

    // Predecessor edges: each block references the previous block(s)
    if (block.height > 0) {
      // Direct predecessor (same creator's previous block)
      edges.push({
        from: `block-${block.height - 1}`,
        to: `block-${block.height}`,
        type: 'predecessor',
      });

      // Cross-references (blocks from other creators)
      if (block.predecessors) {
        block.predecessors.forEach(predHeight => {
          edges.push({
            from: `block-${predHeight}`,
            to: `block-${block.height}`,
            type: 'reference',
          });
        });
      }
    }
  });

  return { nodes, edges };
}

function renderDagSvg(dagData) {
  const vizContainer = document.getElementById('blocklace-viz');
  if (!vizContainer) return;

  const width = vizContainer.clientWidth || 800;
  const height = Math.max(400, dagData.nodes.length * 30);
  const colors = ['#6ba3c7', '#d99a3f', '#9bb87a', '#c77ab8', '#7ac7b8'];

  // Layout: distribute nodes by height (y) and creator (x)
  const nodePositions = {};
  const maxHeight = Math.max(...dagData.nodes.map(n => n.height), 0);
  const creators = [...new Set(dagData.nodes.map(n => n.creator))];
  const creatorCount = Math.max(creators.length, 3);

  dagData.nodes.forEach(node => {
    const xSlot = node.creator % creatorCount;
    const x = 80 + (xSlot * (width - 160) / Math.max(creatorCount - 1, 1));
    const y = 40 + ((maxHeight - node.height) / Math.max(maxHeight, 1)) * (height - 80);
    nodePositions[node.id] = { x, y };
  });

  // Build SVG
  let svg = `<svg width="${width}" height="${height}" xmlns="http://www.w3.org/2000/svg" class="dag-svg">`;

  // Draw edges
  dagData.edges.forEach(edge => {
    const from = nodePositions[edge.from];
    const to = nodePositions[edge.to];
    if (!from || !to) return;

    const isRef = edge.type === 'reference';
    const stroke = isRef ? 'rgba(232, 224, 208, 0.15)' : 'rgba(232, 224, 208, 0.3)';
    const dashArray = isRef ? '4,3' : 'none';

    svg += `<line x1="${from.x}" y1="${from.y}" x2="${to.x}" y2="${to.y}"
            stroke="${stroke}" stroke-width="1.5" stroke-dasharray="${dashArray}"
            marker-end="url(#arrowhead)"/>`;
  });

  // Arrow marker
  svg += `<defs><marker id="arrowhead" markerWidth="6" markerHeight="4" refX="6" refY="2" orient="auto">
    <polygon points="0 0, 6 2, 0 4" fill="rgba(232, 224, 208, 0.3)"/>
  </marker></defs>`;

  // Draw nodes
  dagData.nodes.forEach(node => {
    const pos = nodePositions[node.id];
    if (!pos) return;

    const color = node.isEquivocator ? 'var(--danger)' : colors[node.creator % colors.length];
    const radius = node.isFinalLeader ? 10 : 7;

    // Finality glow
    if (node.isFinalLeader) {
      svg += `<circle cx="${pos.x}" cy="${pos.y}" r="${radius + 4}" fill="none" stroke="${color}" stroke-width="1" opacity="0.4"/>`;
    }

    svg += `<circle cx="${pos.x}" cy="${pos.y}" r="${radius}" fill="${color}"
            class="dag-node" data-node-id="${node.id}" style="cursor: pointer;" opacity="${node.finality === 'final' ? 1 : 0.7}"/>`;

    // Height label
    svg += `<text x="${pos.x}" y="${pos.y + radius + 14}" text-anchor="middle"
            font-family="var(--mono)" font-size="9" fill="var(--text-muted)">#${node.height}</text>`;
  });

  svg += `</svg>`;
  vizContainer.innerHTML = svg;

  // Wire click events on nodes
  vizContainer.querySelectorAll('.dag-node').forEach(nodeEl => {
    nodeEl.addEventListener('click', () => {
      const nodeId = nodeEl.dataset.nodeId;
      const node = dagData.nodes.find(n => n.id === nodeId);
      if (node) showBlockDetail(node);
    });
  });
}

function showBlockDetail(node) {
  const detail = document.getElementById('blocklace-detail');
  const content = document.getElementById('blocklace-detail-content');
  if (!detail || !content) return;

  detail.hidden = false;
  content.innerHTML = `
    <div class="detail-grid" style="font-size: 11px;">
      <span class="detail-grid__label">Height</span>
      <span class="detail-grid__value detail-grid__value--highlight">#${node.height}</span>
      <span class="detail-grid__label">Creator</span>
      <span class="detail-grid__value">Node ${node.creator}</span>
      <span class="detail-grid__label">Hash</span>
      <span class="detail-grid__value detail-grid__value--hash">${api.shortHash(node.hash, 12, 6)}</span>
      <span class="detail-grid__label">Signatures</span>
      <span class="detail-grid__value">${node.signatures}</span>
      <span class="detail-grid__label">Wave</span>
      <span class="detail-grid__value">${node.wave}</span>
      <span class="detail-grid__label">Finality</span>
      <span class="detail-grid__value">${renderFinality(node.finality)}</span>
      <span class="detail-grid__label">Final Leader</span>
      <span class="detail-grid__value">${node.isFinalLeader ? '<span class="cell-badge cell-badge--success">yes</span>' : 'no'}</span>
      <span class="detail-grid__label">Equivocator</span>
      <span class="detail-grid__value">${node.isEquivocator ? '<span class="cell-badge cell-badge--danger">detected</span>' : 'none'}</span>
      <span class="detail-grid__label">Time</span>
      <span class="detail-grid__value">${api.relativeTime(node.timestamp)}</span>
    </div>
    <button class="btn btn-sm btn-secondary" style="margin-top: 12px;" onclick="document.getElementById('blocklace-detail').hidden=true">Close</button>
  `;
}

function renderFinality(level) {
  switch (level) {
    case 'final': return '<span class="cell-badge cell-badge--success">final</span>';
    case 'tentative': return '<span class="cell-badge cell-badge--warning">tentative</span>';
    default: return '<span class="cell-badge cell-badge--hosted">pending</span>';
  }
}

function wireControls(dagData) {
  // Zoom and fit controls would manipulate the SVG viewBox
  const fitBtn = document.getElementById('dag-fit');
  if (fitBtn) {
    fitBtn.onclick = () => renderDagSvg(dagData);
  }
}
