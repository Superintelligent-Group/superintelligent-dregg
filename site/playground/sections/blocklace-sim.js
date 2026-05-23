/**
 * Blocklace Simulation Section — simulate N nodes producing blocks,
 * visualize the DAG, observe tau ordering and finality.
 */

import { state, notifyStateChange } from '../playground.js';

const NODE_COLORS = ['#6ba3c7', '#d99a3f', '#9bb87a', '#c77ab8', '#7ac7b8'];

let simulation = null;
let animationFrame = null;

export function initBlocklaceSim(wasm) {
  const section = document.getElementById('section-blocklace-sim');
  if (!section) return;

  section.innerHTML = `
    <div class="pg-section__header">
      <h2>Blocklace Simulator</h2>
      <p>Simulate a Cordial Miners consensus with N nodes. Watch blocks form a DAG, observe finality progression, detect equivocators.</p>
    </div>

    <div class="bsim-controls">
      <div class="bsim-controls__row">
        <label class="bsim-label">
          Nodes
          <input type="number" id="bsim-node-count" value="3" min="2" max="7" class="bsim-input bsim-input--small">
        </label>
        <label class="bsim-label">
          Block Rate (ms)
          <input type="number" id="bsim-rate" value="1000" min="200" max="5000" step="100" class="bsim-input bsim-input--small">
        </label>
        <label class="bsim-label">
          Network Delay (ms)
          <input type="number" id="bsim-delay" value="100" min="0" max="2000" step="50" class="bsim-input bsim-input--small">
        </label>
        <label class="bsim-label">
          <input type="checkbox" id="bsim-equivocate">
          Inject Equivocator
        </label>
      </div>
      <div class="bsim-controls__actions">
        <button class="pg-btn pg-btn--accent" id="bsim-start">Start</button>
        <button class="pg-btn pg-btn--ghost" id="bsim-stop" disabled>Stop</button>
        <button class="pg-btn pg-btn--ghost" id="bsim-step">Step</button>
        <button class="pg-btn pg-btn--ghost" id="bsim-reset">Reset</button>
      </div>
    </div>

    <div class="bsim-stats" id="bsim-stats">
      <div class="bsim-stat"><span class="bsim-stat__label">Blocks</span><span class="bsim-stat__value" id="bsim-block-count">0</span></div>
      <div class="bsim-stat"><span class="bsim-stat__label">Final</span><span class="bsim-stat__value" id="bsim-final-count">0</span></div>
      <div class="bsim-stat"><span class="bsim-stat__label">Wave</span><span class="bsim-stat__value" id="bsim-wave">0</span></div>
      <div class="bsim-stat"><span class="bsim-stat__label">Equivocations</span><span class="bsim-stat__value" id="bsim-equivocations">0</span></div>
    </div>

    <div class="bsim-dag" id="bsim-dag">
      <div class="pg-empty">Press Start or Step to begin the simulation.</div>
    </div>

    <div class="bsim-log" id="bsim-log">
      <div class="bsim-log__header">Event Log</div>
      <div class="bsim-log__body" id="bsim-log-body"></div>
    </div>
  `;

  wireSimControls();
}

function wireSimControls() {
  document.getElementById('bsim-start').addEventListener('click', startSimulation);
  document.getElementById('bsim-stop').addEventListener('click', stopSimulation);
  document.getElementById('bsim-step').addEventListener('click', stepSimulation);
  document.getElementById('bsim-reset').addEventListener('click', resetSimulation);
}

function createSimulation() {
  const nodeCount = parseInt(document.getElementById('bsim-node-count').value) || 3;
  const rate = parseInt(document.getElementById('bsim-rate').value) || 1000;
  const delay = parseInt(document.getElementById('bsim-delay').value) || 100;
  const equivocate = document.getElementById('bsim-equivocate').checked;

  return {
    nodeCount,
    rate,
    delay,
    equivocate,
    blocks: [],
    nodes: Array.from({ length: nodeCount }, (_, i) => ({
      id: i,
      height: 0,
      tip: null,
      seen: new Set(),
    })),
    time: 0,
    wave: 0,
    finalCount: 0,
    equivocations: 0,
    log: [],
  };
}

function startSimulation() {
  if (!simulation) simulation = createSimulation();

  document.getElementById('bsim-start').disabled = true;
  document.getElementById('bsim-stop').disabled = false;

  const tick = () => {
    stepSimulation();
    animationFrame = setTimeout(tick, simulation.rate);
  };
  tick();
}

function stopSimulation() {
  if (animationFrame) {
    clearTimeout(animationFrame);
    animationFrame = null;
  }
  document.getElementById('bsim-start').disabled = false;
  document.getElementById('bsim-stop').disabled = true;
}

function stepSimulation() {
  if (!simulation) simulation = createSimulation();

  // Pick a random node to produce a block
  const creatorIdx = Math.floor(Math.random() * simulation.nodeCount);
  const creator = simulation.nodes[creatorIdx];

  // Build predecessors (blocks this node has seen from others)
  const predecessors = [];
  simulation.nodes.forEach((node, idx) => {
    if (idx !== creatorIdx && node.tip !== null) {
      predecessors.push(node.tip);
    }
  });

  // Create block
  const blockId = simulation.blocks.length;
  const block = {
    id: blockId,
    creator: creatorIdx,
    height: creator.height,
    predecessors: predecessors.map(b => b.id),
    timestamp: simulation.time,
    hash: Math.floor(Math.random() * 0xFFFFFFFF).toString(16).padStart(8, '0'),
    isEquivocator: false,
    isFinal: false,
    wave: simulation.wave,
    signatures: 0,
  };

  // Check for equivocation injection
  if (simulation.equivocate && creatorIdx === 0 && simulation.blocks.length > 0 && Math.random() < 0.1) {
    block.isEquivocator = true;
    simulation.equivocations++;
    addLog(`Node ${creatorIdx} EQUIVOCATED at height ${block.height}`, 'danger');
  }

  // Count signatures (nodes that have seen previous blocks from this creator)
  block.signatures = Math.min(simulation.nodeCount, 1 + predecessors.length);

  // Check finality (2f+1 threshold)
  const threshold = Math.floor((simulation.nodeCount * 2) / 3) + 1;
  if (block.signatures >= threshold) {
    block.isFinal = true;
    simulation.finalCount++;
  }

  simulation.blocks.push(block);
  creator.height++;
  creator.tip = block;

  // Propagate to other nodes (with delay simulation)
  simulation.nodes.forEach((node, idx) => {
    if (idx !== creatorIdx) {
      node.seen.add(blockId);
    }
  });

  // Advance time and wave
  simulation.time += simulation.rate;
  if (simulation.blocks.length % simulation.nodeCount === 0) {
    simulation.wave++;
  }

  addLog(`Node ${creatorIdx} produced block #${blockId} (h=${block.height}, sigs=${block.signatures}${block.isFinal ? ', FINAL' : ''})`, block.isFinal ? 'success' : 'info');

  renderSimState();
}

function resetSimulation() {
  stopSimulation();
  simulation = null;
  document.getElementById('bsim-dag').innerHTML = '<div class="pg-empty">Press Start or Step to begin the simulation.</div>';
  document.getElementById('bsim-log-body').innerHTML = '';
  document.getElementById('bsim-block-count').textContent = '0';
  document.getElementById('bsim-final-count').textContent = '0';
  document.getElementById('bsim-wave').textContent = '0';
  document.getElementById('bsim-equivocations').textContent = '0';
}

function renderSimState() {
  if (!simulation) return;

  // Update stats
  document.getElementById('bsim-block-count').textContent = simulation.blocks.length;
  document.getElementById('bsim-final-count').textContent = simulation.finalCount;
  document.getElementById('bsim-wave').textContent = simulation.wave;
  document.getElementById('bsim-equivocations').textContent = simulation.equivocations;

  // Render DAG as SVG
  renderDag();
}

function renderDag() {
  const container = document.getElementById('bsim-dag');
  if (!simulation || !simulation.blocks.length) return;

  const width = container.clientWidth || 600;
  const blocks = simulation.blocks;
  const maxHeight = Math.max(...blocks.map(b => b.height));
  const svgHeight = Math.max(300, (maxHeight + 2) * 50);

  // Position nodes
  const positions = {};
  blocks.forEach(block => {
    const x = 60 + (block.creator * (width - 120) / Math.max(simulation.nodeCount - 1, 1));
    const y = svgHeight - 40 - block.height * 45;
    positions[block.id] = { x, y };
  });

  let svg = `<svg width="${width}" height="${svgHeight}" xmlns="http://www.w3.org/2000/svg">`;

  // Edges
  blocks.forEach(block => {
    const to = positions[block.id];
    block.predecessors.forEach(predId => {
      const from = positions[predId];
      if (from && to) {
        svg += `<line x1="${from.x}" y1="${from.y}" x2="${to.x}" y2="${to.y}" stroke="rgba(232,224,208,0.15)" stroke-width="1"/>`;
      }
    });
  });

  // Nodes
  blocks.forEach(block => {
    const pos = positions[block.id];
    const color = block.isEquivocator ? '#d4685c' : NODE_COLORS[block.creator % NODE_COLORS.length];
    const r = block.isFinal ? 9 : 6;

    if (block.isFinal) {
      svg += `<circle cx="${pos.x}" cy="${pos.y}" r="${r + 4}" fill="none" stroke="${color}" stroke-width="1" opacity="0.3"/>`;
    }
    svg += `<circle cx="${pos.x}" cy="${pos.y}" r="${r}" fill="${color}" opacity="${block.isFinal ? 1 : 0.6}"/>`;
  });

  // Creator labels at top
  simulation.nodes.forEach((node, idx) => {
    const x = 60 + (idx * (width - 120) / Math.max(simulation.nodeCount - 1, 1));
    svg += `<text x="${x}" y="20" text-anchor="middle" font-family="'JetBrains Mono', monospace" font-size="9" fill="${NODE_COLORS[idx]}">Node ${idx}</text>`;
  });

  svg += `</svg>`;
  container.innerHTML = svg;
}

function addLog(message, type = 'info') {
  if (!simulation) return;
  simulation.log.push({ message, type, time: simulation.time });

  const body = document.getElementById('bsim-log-body');
  const colorMap = { info: 'var(--text-dim)', success: 'var(--accent-bright)', danger: 'var(--danger)' };
  body.innerHTML = simulation.log.slice(-20).reverse().map(entry =>
    `<div class="bsim-log__entry" style="color: ${colorMap[entry.type] || colorMap.info};">${entry.message}</div>`
  ).join('');
}
