// Merkle Tree Visualizer — interactive SVG rendering with membership proofs

export class MerkleVisualizer {
    constructor(wasm) {
        this.wasm = wasm;
        this.svg = document.getElementById('merkle-svg');
        this.leaves = [];
        this.rootHash = null;
        this.highlightedPath = null;

        this._bind();
    }

    _bind() {
        document.getElementById('btn-merkle-build').addEventListener('click', () => this._build());
        document.getElementById('btn-merkle-member').addEventListener('click', () => this._proveMembership());
        document.getElementById('btn-merkle-absent').addEventListener('click', () => this._proveAbsence());
        document.getElementById('btn-merkle-add').addEventListener('click', () => this._addLeaf());
        document.getElementById('btn-merkle-remove-last').addEventListener('click', () => this._removeLastLeaf());
    }

    _getLeaves() {
        const text = document.getElementById('merkle-leaves').value.trim();
        return text.split('\n').map(s => s.trim()).filter(s => s.length > 0);
    }

    _setLeaves(leaves) {
        document.getElementById('merkle-leaves').value = leaves.join('\n');
    }

    _build() {
        this.leaves = this._getLeaves();
        if (this.leaves.length === 0) {
            this._showMessage('Add at least one leaf to build the tree.');
            return;
        }

        try {
            const result = this.wasm.compute_merkle_root(JSON.stringify(this.leaves));
            this.rootHash = result.root_hex;
            this.highlightedPath = null;

            document.getElementById('merkle-root-display').textContent = result.root_hex.slice(0, 16) + '...';
            document.getElementById('merkle-root-display').title = result.root_hex;
            document.getElementById('merkle-leaf-count').textContent = result.num_leaves;

            this._renderTree();
        } catch (e) {
            this._showMessage('Error: ' + (e.message || String(e)));
        }
    }

    _proveMembership() {
        const target = document.getElementById('merkle-member-leaf').value.trim();
        if (!target) return;

        this.leaves = this._getLeaves();
        if (this.leaves.length === 0) {
            this._showMessage('Build the tree first.');
            return;
        }

        try {
            const result = this.wasm.merkle_membership_proof(JSON.stringify(this.leaves), target);
            if (result.is_member) {
                // Highlight the path from leaf to root
                const leafIdx = this.leaves.indexOf(target);
                this.highlightedPath = { type: 'member', leafIdx, depth: result.proof_path_len };
                this._renderTree();
                this._showMessage(`"${target}" is a MEMBER. Proof path: ${result.proof_path_len} levels.`, 'success');
            } else {
                this.highlightedPath = null;
                this._renderTree();
                this._showMessage(`"${target}" is NOT a member of this tree.`, 'error');
            }
        } catch (e) {
            this._showMessage('Error: ' + (e.message || String(e)));
        }
    }

    _proveAbsence() {
        const target = document.getElementById('merkle-absent-leaf').value.trim();
        if (!target) return;

        this.leaves = this._getLeaves();
        if (this.leaves.length === 0) {
            this._showMessage('Build the tree first.');
            return;
        }

        try {
            const result = this.wasm.merkle_non_membership_proof(JSON.stringify(this.leaves), target);
            if (result.proven_absent) {
                this.highlightedPath = { type: 'absent' };
                this._renderTree();
                this._showMessage(`"${target}" is PROVEN ABSENT from the tree.`, 'success');
            } else {
                this._showMessage(`Could not generate non-membership proof. The leaf may be present.`, 'warning');
            }
        } catch (e) {
            this._showMessage('Error: ' + (e.message || String(e)));
        }
    }

    _addLeaf() {
        const input = document.getElementById('merkle-new-leaf');
        const newLeaf = input.value.trim();
        if (!newLeaf) return;

        this.leaves = this._getLeaves();
        this.leaves.push(newLeaf);
        this._setLeaves(this.leaves);
        input.value = '';
        this._build();
    }

    _removeLastLeaf() {
        this.leaves = this._getLeaves();
        if (this.leaves.length === 0) return;
        this.leaves.pop();
        this._setLeaves(this.leaves);
        if (this.leaves.length > 0) {
            this._build();
        } else {
            this.svg.innerHTML = '';
            document.getElementById('merkle-root-display').textContent = '--';
            document.getElementById('merkle-leaf-count').textContent = '0';
        }
    }

    _renderTree() {
        const n = this.leaves.length;
        if (n === 0) {
            this.svg.innerHTML = '';
            return;
        }

        // Build a simulated 4-ary tree structure for visualization
        // Each internal node is a hash of up to 4 children
        const levels = this._buildTreeLevels(this.leaves);
        const totalLevels = levels.length;

        // Calculate SVG dimensions
        const svgRect = this.svg.getBoundingClientRect();
        const width = Math.max(svgRect.width || 600, n * 80);
        const height = Math.max(svgRect.height || 400, totalLevels * 90 + 60);

        this.svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
        this.svg.setAttribute('width', '100%');
        this.svg.setAttribute('height', '100%');

        let svgContent = '';

        // Calculate positions for each node
        const positions = [];
        for (let level = 0; level < totalLevels; level++) {
            const nodesAtLevel = levels[level].length;
            const y = 40 + (totalLevels - 1 - level) * 90;
            const levelPositions = [];

            for (let i = 0; i < nodesAtLevel; i++) {
                const x = (width / (nodesAtLevel + 1)) * (i + 1);
                levelPositions.push({ x, y });
            }
            positions.push(levelPositions);
        }

        // Draw edges first (behind nodes)
        for (let level = 1; level < totalLevels; level++) {
            const parentLevel = level;
            const childLevel = level - 1;
            for (let pi = 0; pi < levels[parentLevel].length; pi++) {
                // Each parent has up to 4 children
                const startChild = pi * 4;
                const endChild = Math.min(startChild + 4, levels[childLevel].length);
                for (let ci = startChild; ci < endChild; ci++) {
                    const parent = positions[parentLevel][pi];
                    const child = positions[childLevel][ci];
                    if (parent && child) {
                        const isHighlighted = this._isEdgeHighlighted(level, pi, childLevel, ci);
                        const edgeClass = isHighlighted ? 'edge highlighted' : 'edge';
                        svgContent += `<line class="${edgeClass}" x1="${parent.x}" y1="${parent.y}" x2="${child.x}" y2="${child.y}"/>`;
                    }
                }
            }
        }

        // Draw nodes
        for (let level = 0; level < totalLevels; level++) {
            for (let i = 0; i < levels[level].length; i++) {
                const pos = positions[level][i];
                const node = levels[level][i];
                const isRoot = (level === totalLevels - 1 && levels[level].length === 1);
                const isLeaf = (level === 0);
                const isHighlighted = this._isNodeHighlighted(level, i);

                let circleClass = 'node-circle';
                if (isRoot) circleClass += ' root-node';
                if (isHighlighted) circleClass += ' highlighted';

                const r = isRoot ? 22 : (isLeaf ? 18 : 16);

                svgContent += `<circle class="${circleClass}" cx="${pos.x}" cy="${pos.y}" r="${r}" data-level="${level}" data-index="${i}"/>`;

                // Label
                if (isLeaf) {
                    const label = node.label.length > 6 ? node.label.slice(0, 6) + '..' : node.label;
                    svgContent += `<text class="node-label" x="${pos.x}" y="${pos.y + 4}">${escapeHtml(label)}</text>`;
                } else {
                    const hashShort = node.hash ? node.hash.slice(0, 6) : '...';
                    svgContent += `<text class="node-hash" x="${pos.x}" y="${pos.y + 3}">${hashShort}</text>`;
                }

                // Root label
                if (isRoot) {
                    svgContent += `<text class="node-label" x="${pos.x}" y="${pos.y - 28}" style="font-size:10px; fill:var(--accent-bright)">root</text>`;
                }
            }
        }

        this.svg.innerHTML = svgContent;
    }

    _buildTreeLevels(leaves) {
        // Level 0: leaves
        const levels = [];
        levels.push(leaves.map((l, i) => ({
            label: l,
            hash: this._simpleHash(l),
            index: i,
        })));

        // Build upward in groups of 4
        let currentLevel = levels[0];
        while (currentLevel.length > 1) {
            const nextLevel = [];
            for (let i = 0; i < currentLevel.length; i += 4) {
                const children = currentLevel.slice(i, i + 4);
                const combinedHash = this._combineHashes(children.map(c => c.hash));
                nextLevel.push({
                    label: `node`,
                    hash: combinedHash,
                    childStart: i,
                    childEnd: Math.min(i + 4, currentLevel.length),
                });
            }
            levels.push(nextLevel);
            currentLevel = nextLevel;
        }

        return levels;
    }

    _simpleHash(str) {
        // Simple hash for display (not cryptographic — the real hashing is in WASM)
        let h = 0x811c9dc5;
        for (let i = 0; i < str.length; i++) {
            h ^= str.charCodeAt(i);
            h = Math.imul(h, 0x01000193);
        }
        return (h >>> 0).toString(16).padStart(8, '0');
    }

    _combineHashes(hashes) {
        return this._simpleHash(hashes.join(':'));
    }

    _isNodeHighlighted(level, index) {
        if (!this.highlightedPath || this.highlightedPath.type !== 'member') return false;
        // Highlight the path from the leaf upward
        let idx = this.highlightedPath.leafIdx;
        for (let l = 0; l <= level; l++) {
            if (l === level) return idx === index;
            idx = Math.floor(idx / 4);
        }
        return false;
    }

    _isEdgeHighlighted(parentLevel, parentIdx, childLevel, childIdx) {
        if (!this.highlightedPath || this.highlightedPath.type !== 'member') return false;
        return this._isNodeHighlighted(parentLevel, parentIdx) && this._isNodeHighlighted(childLevel, childIdx);
    }

    _showMessage(msg, type = 'info') {
        // Show in the info area below the tree
        const infoEl = document.getElementById('merkle-root-display');
        if (type === 'error') {
            infoEl.style.color = 'var(--danger)';
        } else if (type === 'success') {
            infoEl.style.color = 'var(--accent-bright)';
        } else {
            infoEl.style.color = '';
        }
        // Also log to console for debug
        console.log(`[merkle] ${type}: ${msg}`);
    }
}

function escapeHtml(str) {
    return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}
