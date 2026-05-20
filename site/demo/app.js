// pyana explorer — main application
// Loads WASM lazily, initializes all component modules, manages app state.

import { StateExplorer } from './components/explorer.js';
import { ActionBuilder } from './components/builder.js';
import { ProofViewer } from './components/proof-viewer.js';
import { DatalogEvaluator } from './components/datalog.js';
import { MerkleVisualizer } from './components/merkle-viz.js';
import { NoteLifecycle } from './components/note-lifecycle.js';

// ============================================================================
// Application State
// ============================================================================

const appState = {
    tokens: [],
    cells: [],
    nullifiers: [],
    rootKey: null,
    rootKeyHex: null,
    federation: {
        root: null,
        height: 0,
    },
};

// ============================================================================
// Output Log
// ============================================================================

const outputLog = {
    _container: null,

    init() {
        this._container = document.getElementById('output-log');
        document.getElementById('btn-clear-output').addEventListener('click', () => this.clear());
    },

    _append(type, title, body) {
        // Remove welcome message
        const welcome = this._container.querySelector('.output-welcome');
        if (welcome) welcome.remove();

        const entry = document.createElement('div');
        entry.className = `output-entry ${type}`;

        const time = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });

        entry.innerHTML = `
            <div class="entry-header">
                <span class="entry-time">${time}</span>
                <span class="entry-title">${title}</span>
            </div>
            <div class="entry-body">${body}</div>
        `;

        this._container.appendChild(entry);
        this._container.scrollTop = this._container.scrollHeight;
    },

    success(title, body) { this._append('success', title, body); },
    error(title, body) { this._append('error', title, body); },
    info(title, body) { this._append('info', title, body); },
    warning(title, body) { this._append('warning', title, body); },

    clear() {
        this._container.innerHTML = `
            <div class="output-welcome">
                <p>Output cleared.</p>
            </div>
        `;
    },
};

// ============================================================================
// Navigation
// ============================================================================

function setupNavigation() {
    const tabs = document.querySelectorAll('.nav-tab');
    const views = document.querySelectorAll('.view');

    tabs.forEach(tab => {
        tab.addEventListener('click', () => {
            const viewId = tab.dataset.view;

            tabs.forEach(t => t.classList.remove('active'));
            views.forEach(v => v.classList.remove('active'));

            tab.classList.add('active');
            document.getElementById(`view-${viewId}`).classList.add('active');
        });
    });
}

// ============================================================================
// WASM Loading
// ============================================================================

async function loadWasm() {
    const statusEl = document.getElementById('wasm-status');

    try {
        const { default: init, ...exports } = await import('./pkg/pyana_wasm.js');
        await init();
        statusEl.textContent = 'wasm ready';
        statusEl.classList.add('ready');
        console.log('[pyana] WASM module loaded');
        return exports;
    } catch (e) {
        statusEl.textContent = 'wasm error';
        statusEl.classList.add('error');
        console.error('[pyana] Failed to load WASM:', e);
        throw e;
    }
}

// ============================================================================
// Boot
// ============================================================================

async function main() {
    // Render UI immediately (before WASM loads)
    setupNavigation();
    outputLog.init();

    try {
        const wasm = await loadWasm();

        // Initialize components
        const explorer = new StateExplorer(appState);
        const builder = new ActionBuilder(appState, wasm, outputLog);
        const proofViewer = new ProofViewer(wasm);
        const datalogEval = new DatalogEvaluator(wasm);
        const merkleViz = new MerkleVisualizer(wasm);
        const noteLifecycle = new NoteLifecycle(wasm);

        // Listen for state changes
        document.addEventListener('state-updated', () => {
            explorer.update();
        });

        document.addEventListener('notes-updated', (e) => {
            const { nullifiers } = e.detail;
            appState.nullifiers = nullifiers;
            explorer.update();
        });

        // Token inspection
        document.addEventListener('token-inspect', (e) => {
            const { token, index } = e.detail;
            outputLog.info(`Token #${index} Details`,
                `Format: ${token.format}\nLocation: ${token.location}\nAttenuated: ${token.attenuated}\n` +
                (token.service ? `Service: ${token.service}\nActions: ${token.actions}\n` : '') +
                `\nEncoded:\n<span class="highlight">${token.encoded}</span>`);
        });

        // Initial render
        explorer.update();

    } catch (e) {
        outputLog.error('Initialization Failed',
            `Could not load WASM module.\n\n${e.message || e}\n\nBuild with: cd wasm && wasm-pack build --target web --out-dir ../site/demo/pkg`);
    }
}

main();
