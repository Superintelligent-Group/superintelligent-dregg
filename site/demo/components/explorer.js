// State Explorer — tree view of tokens, cells, nullifiers, federation state

export class StateExplorer {
    constructor(appState) {
        this.state = appState;
        this.tokenTree = document.getElementById('token-tree');
        this.cellTree = document.getElementById('cell-tree');
        this.nullifierTree = document.getElementById('nullifier-tree');
        this.federationTree = document.getElementById('federation-tree');
        this.tokenCount = document.getElementById('token-count');
        this.cellCount = document.getElementById('cell-count');
        this.nullifierCount = document.getElementById('nullifier-count');
        this.fedRoot = document.getElementById('fed-root');
        this.fedHeight = document.getElementById('fed-height');

        this._setupTreeToggles();
    }

    _setupTreeToggles() {
        document.querySelectorAll('.tree-node.root').forEach(node => {
            node.addEventListener('click', () => {
                node.classList.toggle('expanded');
            });
        });
    }

    update() {
        this._renderTokens();
        this._renderCells();
        this._renderNullifiers();
        this._renderFederation();
    }

    _renderTokens() {
        const tokens = this.state.tokens;
        this.tokenCount.textContent = tokens.length;

        this.tokenTree.innerHTML = '';
        tokens.forEach((token, i) => {
            const item = document.createElement('div');
            item.className = 'tree-item';
            const isAttenuated = token.attenuated;
            const badgeClass = isAttenuated ? 'item-badge attenuated' : 'item-badge';
            const badgeText = isAttenuated ? 'att' : 'root';
            const shortToken = token.encoded.slice(0, 24) + '...';

            item.innerHTML = `
                <span class="item-icon">${isAttenuated ? '&#x2192;' : '&#x2022;'}</span>
                <span class="item-label" title="${token.encoded}">#${i} ${shortToken}</span>
                <span class="${badgeClass}">${badgeText}</span>
            `;
            item.addEventListener('click', () => {
                this._showTokenDetail(token, i);
            });
            this.tokenTree.appendChild(item);
        });
    }

    _renderCells() {
        const cells = this.state.cells;
        this.cellCount.textContent = cells.length;

        this.cellTree.innerHTML = '';
        cells.forEach((cell, i) => {
            const item = document.createElement('div');
            item.className = 'tree-item';
            item.innerHTML = `
                <span class="item-icon">&#x25A3;</span>
                <span class="item-label">Cell #${i} (${cell.fields.length} fields)</span>
                <span class="item-badge">${cell.permissions}</span>
            `;
            this.cellTree.appendChild(item);
        });
    }

    _renderNullifiers() {
        const nullifiers = this.state.nullifiers;
        this.nullifierCount.textContent = nullifiers.length;

        this.nullifierTree.innerHTML = '';
        nullifiers.forEach((nul, i) => {
            const item = document.createElement('div');
            item.className = 'tree-leaf';
            item.innerHTML = `
                <span class="leaf-key">#${i}</span>
                <span class="leaf-value">${nul.slice(0, 16)}...</span>
            `;
            this.nullifierTree.appendChild(item);
        });
    }

    _renderFederation() {
        const fed = this.state.federation;
        this.fedRoot.textContent = fed.root ? fed.root.slice(0, 16) + '...' : '--';
        this.fedRoot.title = fed.root || '';
        this.fedHeight.textContent = fed.height;
    }

    _showTokenDetail(token, index) {
        // Emit event for detail view
        const event = new CustomEvent('token-inspect', { detail: { token, index } });
        document.dispatchEvent(event);
    }
}
