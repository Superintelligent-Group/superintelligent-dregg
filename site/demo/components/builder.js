// Action Builder — token minting, attenuation, verification, fold operations

export class ActionBuilder {
    constructor(appState, wasm, outputLog) {
        this.state = appState;
        this.wasm = wasm;
        this.log = outputLog;

        this.attTokenSelect = document.getElementById('att-token-select');
        this.verTokenSelect = document.getElementById('ver-token-select');

        this._bind();
    }

    _bind() {
        document.getElementById('btn-gen-key').addEventListener('click', () => this._generateKey());
        document.getElementById('btn-mint').addEventListener('click', () => this._mint());
        document.getElementById('btn-attenuate').addEventListener('click', () => this._attenuate());
        document.getElementById('btn-verify').addEventListener('click', () => this._verify());
        document.getElementById('btn-fold').addEventListener('click', () => this._fold());
        document.getElementById('btn-reset-state').addEventListener('click', () => this._reset());
    }

    _generateKey() {
        try {
            const result = this.wasm.generate_root_key();
            this.state.rootKey = new Uint8Array(result.key_bytes);
            this.state.rootKeyHex = result.key_hex;
            document.getElementById('root-key-input').value = result.key_hex;
            this.log.info('Key Generated', `Root key: ${result.key_hex.slice(0, 16)}...${result.key_hex.slice(-8)}`);
        } catch (e) {
            this.log.error('Key Generation Failed', e.message || String(e));
        }
    }

    _mint() {
        const keyHex = document.getElementById('root-key-input').value.trim();
        const location = document.getElementById('mint-location').value.trim() || 'pyana.dev';

        if (keyHex.length !== 64) {
            this.log.error('Mint Failed', 'Root key must be 64 hex characters (32 bytes). Click "Random" to generate one.');
            return;
        }

        const keyBytes = hexToBytes(keyHex);
        this.state.rootKey = keyBytes;
        this.state.rootKeyHex = keyHex;

        try {
            const result = this.wasm.mint_token(keyBytes, location);
            const tokenEntry = {
                encoded: result.token,
                location: result.location,
                format: result.format,
                attenuated: false,
                service: null,
                actions: null,
            };
            this.state.tokens.push(tokenEntry);
            this._updateTokenSelects();
            this._emitUpdate();

            this.log.success('Token Minted',
                `Format: ${result.format}\nLocation: ${result.location}\nToken: <span class="highlight">${result.token.slice(0, 40)}...</span>`);
        } catch (e) {
            this.log.error('Mint Failed', e.message || String(e));
        }
    }

    _attenuate() {
        const tokenIdx = parseInt(this.attTokenSelect.value);
        const service = document.getElementById('att-service').value.trim();
        const actions = document.getElementById('att-actions').value.trim();
        const expires = parseInt(document.getElementById('att-expires').value) || 0;

        if (isNaN(tokenIdx) || !this.state.tokens[tokenIdx]) {
            this.log.error('Attenuate Failed', 'Select a token first');
            return;
        }

        if (!this.state.rootKey) {
            this.log.error('Attenuate Failed', 'No root key available. Mint a token first.');
            return;
        }

        try {
            const tokenStr = this.state.tokens[tokenIdx].encoded;
            const result = this.wasm.attenuate_token(tokenStr, this.state.rootKey, service, actions, BigInt(expires));

            const tokenEntry = {
                encoded: result.token,
                location: this.state.tokens[tokenIdx].location,
                format: 'macaroon',
                attenuated: true,
                service: result.service,
                actions: result.actions,
            };
            this.state.tokens.push(tokenEntry);
            this._updateTokenSelects();
            this._emitUpdate();

            this.log.success('Token Attenuated',
                `Service: ${result.service}\nActions: ${result.actions}\nExpires: ${result.expires_secs}s\nToken: <span class="highlight">${result.token.slice(0, 40)}...</span>`);
        } catch (e) {
            this.log.error('Attenuate Failed', e.message || String(e));
        }
    }

    _verify() {
        const tokenIdx = parseInt(this.verTokenSelect.value);
        const appId = document.getElementById('ver-app').value.trim();
        const action = document.getElementById('ver-action').value.trim();

        if (isNaN(tokenIdx) || !this.state.tokens[tokenIdx]) {
            this.log.error('Verify Failed', 'Select a token first');
            return;
        }

        if (!this.state.rootKey) {
            this.log.error('Verify Failed', 'No root key available.');
            return;
        }

        try {
            const tokenStr = this.state.tokens[tokenIdx].encoded;
            const result = this.wasm.verify_token(tokenStr, this.state.rootKey, appId, action);

            if (result.allowed) {
                this.log.success('Verification: ALLOWED',
                    `Policy: ${result.policy || 'default'}\nApp: ${appId || '(any)'}\nAction: ${action || '(any)'}`);
            } else {
                this.log.error('Verification: DENIED',
                    `Reason: <span class="danger">${result.error || 'no matching policy'}</span>\nApp: ${appId || '(any)'}\nAction: ${action || '(any)'}`);
            }
        } catch (e) {
            this.log.error('Verify Error', e.message || String(e));
        }
    }

    _fold() {
        const factsText = document.getElementById('fold-facts').value.trim();
        const removeText = document.getElementById('fold-remove').value.trim();

        const facts = factsText.split('\n').map(s => s.trim()).filter(s => s.length > 0);
        const remove = removeText.split('\n').map(s => s.trim()).filter(s => s.length > 0);

        if (facts.length === 0) {
            this.log.error('Fold Failed', 'Add at least one fact');
            return;
        }

        try {
            const result = this.wasm.demonstrate_fold(JSON.stringify(facts), JSON.stringify(remove));
            const status = result.verified ? 'success' : 'error';
            const method = result.verified ? 'success' : 'error';

            this.log[method](`Fold ${result.verified ? 'VERIFIED' : 'FAILED'}`,
                `Old root: <span class="highlight">${result.old_root_hex.slice(0, 32)}...</span>\nNew root: <span class="highlight">${result.new_root_hex.slice(0, 32)}...</span>\n\nTotal: ${result.total_facts} | Removed: ${result.removed_facts} | Remaining: ${result.remaining_facts}\n\nThe cryptographic delta proves capabilities can only be narrowed, never expanded.`);

            // Update federation state
            this.state.federation.root = result.new_root_hex;
            this.state.federation.height++;
            this._emitUpdate();
        } catch (e) {
            this.log.error('Fold Error', e.message || String(e));
        }
    }

    _reset() {
        this.state.tokens = [];
        this.state.cells = [];
        this.state.nullifiers = [];
        this.state.rootKey = null;
        this.state.rootKeyHex = null;
        this.state.federation = { root: null, height: 0 };
        document.getElementById('root-key-input').value = '';
        this._updateTokenSelects();
        this._emitUpdate();
        this.log.info('State Reset', 'All tokens, cells, and nullifiers cleared.');
    }

    _updateTokenSelects() {
        const options = this.state.tokens.map((t, i) => {
            const label = t.attenuated ? `#${i} (attenuated: ${t.service})` : `#${i} (root: ${t.location})`;
            return `<option value="${i}">${label}</option>`;
        }).join('');

        const base = '<option value="">-- select token --</option>';
        this.attTokenSelect.innerHTML = base + options;
        this.verTokenSelect.innerHTML = base + options;

        // Auto-select latest
        if (this.state.tokens.length > 0) {
            const last = this.state.tokens.length - 1;
            this.attTokenSelect.value = last;
            this.verTokenSelect.value = last;
        }
    }

    _emitUpdate() {
        document.dispatchEvent(new CustomEvent('state-updated'));
    }
}

function hexToBytes(hex) {
    const bytes = new Uint8Array(hex.length / 2);
    for (let i = 0; i < hex.length; i += 2) {
        bytes[i / 2] = parseInt(hex.substr(i, 2), 16);
    }
    return bytes;
}
