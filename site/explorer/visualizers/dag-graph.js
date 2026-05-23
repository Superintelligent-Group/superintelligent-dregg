/**
 * DAG Graph Visualizer — generic directed acyclic graph renderer.
 *
 * Used for blocklace visualization. Supports both SVG (small DAGs)
 * and Canvas (large DAGs) rendering modes.
 *
 * Interface: create(container, opts) -> { update(data), destroy() }
 * Data format: { nodes: [{id, ...}], edges: [{from, to, type}] }
 */

export const name = 'dag-graph';

const DEFAULT_OPTS = {
  nodeRadius: 8,
  nodeColors: ['#6ba3c7', '#d99a3f', '#9bb87a', '#c77ab8', '#7ac7b8'],
  edgeColor: 'rgba(232, 224, 208, 0.25)',
  edgeRefColor: 'rgba(232, 224, 208, 0.12)',
  finalGlowColor: 'rgba(155, 184, 122, 0.3)',
  backgroundColor: 'transparent',
  padding: 40,
  layerSpacing: 60,
  nodeSpacing: 100,
  showFinality: true,
  animate: false,
  mode: 'svg', // 'svg' or 'canvas'
  onNodeClick: null,
};

/**
 * Create a new DAG graph instance.
 */
export function create(container, opts = {}) {
  const config = { ...DEFAULT_OPTS, ...opts };
  let currentData = null;
  let svgElement = null;
  let animFrame = null;

  function update(data) {
    currentData = data;
    if (config.mode === 'canvas') {
      renderCanvas(container, data, config);
    } else {
      svgElement = renderSvg(container, data, config);
    }
  }

  function destroy() {
    if (animFrame) cancelAnimationFrame(animFrame);
    container.innerHTML = '';
    currentData = null;
  }

  function setOption(key, value) {
    config[key] = value;
    if (currentData) update(currentData);
  }

  return { update, destroy, setOption };
}

/**
 * Simple init/update interface for use as a registered visualizer.
 */
let _container = null;
let _instance = null;

export function init(container) {
  _container = container;
}

export function update(data) {
  if (!_container) return;
  if (!_instance) {
    _instance = create(_container, {});
  }
  _instance.update(data);
}

export function destroy() {
  if (_instance) _instance.destroy();
  _instance = null;
  _container = null;
}

// =============================================================================
// SVG Renderer
// =============================================================================

function renderSvg(container, data, config) {
  const { nodes, edges } = data;
  if (!nodes.length) {
    container.innerHTML = '<div style="padding: 20px; text-align: center; color: var(--text-muted); font-family: var(--mono); font-size: 11px;">No nodes in DAG</div>';
    return null;
  }

  // Layout: Sugiyama-style layered layout
  const layout = computeLayout(nodes, edges, container, config);
  const { width, height, positions } = layout;

  let svg = `<svg width="${width}" height="${height}" xmlns="http://www.w3.org/2000/svg" class="dag-graph-svg" style="display: block;">`;

  // Defs for markers and filters
  svg += `<defs>
    <marker id="dag-arrow" markerWidth="6" markerHeight="4" refX="6" refY="2" orient="auto">
      <polygon points="0 0, 6 2, 0 4" fill="${config.edgeColor}"/>
    </marker>
    <marker id="dag-arrow-ref" markerWidth="5" markerHeight="3" refX="5" refY="1.5" orient="auto">
      <polygon points="0 0, 5 1.5, 0 3" fill="${config.edgeRefColor}"/>
    </marker>
    <filter id="glow">
      <feGaussianBlur stdDeviation="3" result="blur"/>
      <feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
  </defs>`;

  // Draw edges
  edges.forEach(edge => {
    const fromPos = positions[edge.from];
    const toPos = positions[edge.to];
    if (!fromPos || !toPos) return;

    const isRef = edge.type === 'reference';
    const stroke = isRef ? config.edgeRefColor : config.edgeColor;
    const marker = isRef ? 'url(#dag-arrow-ref)' : 'url(#dag-arrow)';
    const dashArray = isRef ? '3,2' : 'none';
    const strokeWidth = isRef ? 1 : 1.5;

    // Curved path for better readability
    const midY = (fromPos.y + toPos.y) / 2;
    const path = `M ${fromPos.x} ${fromPos.y} C ${fromPos.x} ${midY}, ${toPos.x} ${midY}, ${toPos.x} ${toPos.y}`;

    svg += `<path d="${path}" fill="none" stroke="${stroke}" stroke-width="${strokeWidth}"
            stroke-dasharray="${dashArray}" marker-end="${marker}" opacity="0.8"/>`;
  });

  // Draw nodes
  nodes.forEach(node => {
    const pos = positions[node.id];
    if (!pos) return;

    const color = node.isEquivocator
      ? 'var(--danger)'
      : config.nodeColors[node.creator % config.nodeColors.length];
    const radius = node.isFinalLeader ? config.nodeRadius + 3 : config.nodeRadius;

    // Finality glow ring
    if (config.showFinality && node.isFinalLeader) {
      svg += `<circle cx="${pos.x}" cy="${pos.y}" r="${radius + 5}" fill="none"
              stroke="${color}" stroke-width="1.5" opacity="0.4" filter="url(#glow)"/>`;
    }

    // Node circle
    const opacity = node.finality === 'final' ? 1 : node.finality === 'tentative' ? 0.75 : 0.5;
    svg += `<circle cx="${pos.x}" cy="${pos.y}" r="${radius}" fill="${color}"
            class="dag-graph__node" data-node-id="${node.id}" style="cursor: pointer;" opacity="${opacity}"/>`;

    // Labels
    if (node.height !== undefined) {
      svg += `<text x="${pos.x}" y="${pos.y + radius + 14}" text-anchor="middle"
              font-family="'JetBrains Mono', monospace" font-size="9" fill="rgba(232,224,208,0.5)" pointer-events="none">#${node.height}</text>`;
    }
  });

  svg += `</svg>`;
  container.innerHTML = svg;

  // Wire click events
  if (config.onNodeClick) {
    container.querySelectorAll('.dag-graph__node').forEach(el => {
      el.addEventListener('click', () => {
        const node = nodes.find(n => n.id === el.dataset.nodeId);
        if (node) config.onNodeClick(node);
      });
    });
  }

  return container.querySelector('svg');
}

// =============================================================================
// Canvas Renderer (for large DAGs)
// =============================================================================

function renderCanvas(container, data, config) {
  const { nodes, edges } = data;
  const layout = computeLayout(nodes, edges, container, config);
  const { width, height, positions } = layout;

  let canvas = container.querySelector('canvas');
  if (!canvas) {
    canvas = document.createElement('canvas');
    canvas.style.display = 'block';
    container.innerHTML = '';
    container.appendChild(canvas);
  }

  const dpr = window.devicePixelRatio || 1;
  canvas.width = width * dpr;
  canvas.height = height * dpr;
  canvas.style.width = width + 'px';
  canvas.style.height = height + 'px';

  const ctx = canvas.getContext('2d');
  ctx.scale(dpr, dpr);
  ctx.clearRect(0, 0, width, height);

  // Draw edges
  edges.forEach(edge => {
    const fromPos = positions[edge.from];
    const toPos = positions[edge.to];
    if (!fromPos || !toPos) return;

    ctx.beginPath();
    ctx.moveTo(fromPos.x, fromPos.y);
    const midY = (fromPos.y + toPos.y) / 2;
    ctx.bezierCurveTo(fromPos.x, midY, toPos.x, midY, toPos.x, toPos.y);
    ctx.strokeStyle = edge.type === 'reference' ? config.edgeRefColor : config.edgeColor;
    ctx.lineWidth = edge.type === 'reference' ? 1 : 1.5;
    if (edge.type === 'reference') ctx.setLineDash([3, 2]);
    else ctx.setLineDash([]);
    ctx.stroke();
  });

  // Draw nodes
  nodes.forEach(node => {
    const pos = positions[node.id];
    if (!pos) return;

    const color = node.isEquivocator
      ? '#d4685c'
      : config.nodeColors[node.creator % config.nodeColors.length];
    const radius = node.isFinalLeader ? config.nodeRadius + 3 : config.nodeRadius;

    if (config.showFinality && node.isFinalLeader) {
      ctx.beginPath();
      ctx.arc(pos.x, pos.y, radius + 5, 0, Math.PI * 2);
      ctx.strokeStyle = color;
      ctx.lineWidth = 1.5;
      ctx.globalAlpha = 0.4;
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    ctx.beginPath();
    ctx.arc(pos.x, pos.y, radius, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.globalAlpha = node.finality === 'final' ? 1 : 0.6;
    ctx.fill();
    ctx.globalAlpha = 1;
  });

  // Click handling on canvas
  if (config.onNodeClick) {
    canvas.onclick = (e) => {
      const rect = canvas.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;

      for (const node of nodes) {
        const pos = positions[node.id];
        if (!pos) continue;
        const dist = Math.sqrt((x - pos.x) ** 2 + (y - pos.y) ** 2);
        if (dist <= config.nodeRadius + 4) {
          config.onNodeClick(node);
          break;
        }
      }
    };
  }
}

// =============================================================================
// Layout Algorithm
// =============================================================================

function computeLayout(nodes, edges, container, config) {
  const width = container.clientWidth || 800;
  const { padding, layerSpacing, nodeSpacing } = config;

  // Group nodes by height (layer)
  const layers = {};
  nodes.forEach(node => {
    const layer = node.height || 0;
    if (!layers[layer]) layers[layer] = [];
    layers[layer].push(node);
  });

  const layerKeys = Object.keys(layers).map(Number).sort((a, b) => a - b);
  const maxLayer = layerKeys.length;
  const height = Math.max(400, maxLayer * layerSpacing + padding * 2);

  // Compute positions
  const positions = {};
  layerKeys.forEach((layerKey, layerIdx) => {
    const layerNodes = layers[layerKey];
    const layerWidth = layerNodes.length * nodeSpacing;
    const startX = (width - layerWidth) / 2 + nodeSpacing / 2;
    const y = padding + (maxLayer - 1 - layerIdx) * layerSpacing;

    layerNodes.forEach((node, nodeIdx) => {
      positions[node.id] = {
        x: startX + nodeIdx * nodeSpacing,
        y: y,
      };
    });
  });

  return { width, height, positions };
}
