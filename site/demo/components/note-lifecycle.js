// Note Lifecycle Demo — mint, transfer, double-spend visualization

export class NoteLifecycle {
    constructor(wasm) {
        this.wasm = wasm;
        this.notes = [];       // { id, asset, amount, commitment, nullifier, owner, spent }
        this.nullifiers = new Set();
        this.timeline = [];    // { type, time, data }
        this.nextId = 0;

        this._bind();
    }

    _bind() {
        document.getElementById('btn-note-mint').addEventListener('click', () => this._mint());
        document.getElementById('btn-note-transfer').addEventListener('click', () => this._transfer());
        document.getElementById('btn-note-doublespend').addEventListener('click', () => this._doubleSpend());
    }

    _mint() {
        const asset = document.getElementById('note-asset').value.trim() || 'token';
        const amount = parseInt(document.getElementById('note-amount').value) || 100;

        // Generate commitment using Merkle root as a proxy for real note commitment
        const noteData = `note:${this.nextId}:${asset}:${amount}:${Date.now()}`;
        const commitment = this._generateCommitment(noteData);
        const nullifier = this._generateNullifier(noteData);

        const note = {
            id: this.nextId++,
            asset,
            amount,
            commitment,
            nullifier,
            owner: 'self',
            spent: false,
        };

        this.notes.push(note);
        this._addTimelineEvent('mint', {
            noteId: note.id,
            asset,
            amount,
            commitment,
        });

        this._updateState();
        this._renderTimeline();
    }

    _transfer() {
        const recipient = document.getElementById('note-recipient').value.trim() || 'bob';
        const transferAmount = parseInt(document.getElementById('note-transfer-amount').value) || 50;

        // Find first unspent note with sufficient balance
        const sourceNote = this.notes.find(n => !n.spent && n.amount >= transferAmount);
        if (!sourceNote) {
            this._addTimelineEvent('error', {
                reason: 'No unspent note with sufficient balance',
                requested: transferAmount,
            });
            this._renderTimeline();
            return;
        }

        // Spend the source note (add nullifier)
        sourceNote.spent = true;
        this.nullifiers.add(sourceNote.nullifier);

        // Create new note for recipient
        const recipientData = `note:${this.nextId}:${sourceNote.asset}:${transferAmount}:${Date.now()}:${recipient}`;
        const newCommitment = this._generateCommitment(recipientData);
        const newNullifier = this._generateNullifier(recipientData);

        const newNote = {
            id: this.nextId++,
            asset: sourceNote.asset,
            amount: transferAmount,
            commitment: newCommitment,
            nullifier: newNullifier,
            owner: recipient,
            spent: false,
        };
        this.notes.push(newNote);

        // If there's change, create a change note
        const change = sourceNote.amount - transferAmount;
        if (change > 0) {
            const changeData = `note:${this.nextId}:${sourceNote.asset}:${change}:${Date.now()}:self`;
            const changeCommitment = this._generateCommitment(changeData);
            const changeNullifier = this._generateNullifier(changeData);

            const changeNote = {
                id: this.nextId++,
                asset: sourceNote.asset,
                amount: change,
                commitment: changeCommitment,
                nullifier: changeNullifier,
                owner: 'self',
                spent: false,
            };
            this.notes.push(changeNote);
        }

        this._addTimelineEvent('transfer', {
            sourceNoteId: sourceNote.id,
            nullifierUsed: sourceNote.nullifier,
            recipient,
            amount: transferAmount,
            newCommitment,
            change,
        });

        this._updateState();
        this._renderTimeline();
    }

    _doubleSpend() {
        // Find the first spent note
        const spentNote = this.notes.find(n => n.spent);
        if (!spentNote) {
            this._addTimelineEvent('error', {
                reason: 'No spent notes to double-spend. Mint and transfer first.',
            });
            this._renderTimeline();
            return;
        }

        // Attempt to use the same nullifier again
        const alreadySpent = this.nullifiers.has(spentNote.nullifier);

        this._addTimelineEvent('error', {
            reason: 'DOUBLE-SPEND REJECTED',
            noteId: spentNote.id,
            nullifier: spentNote.nullifier,
            detail: alreadySpent
                ? `Nullifier ${spentNote.nullifier.slice(0, 16)}... is already in the nullifier set. Transaction rejected.`
                : 'Nullifier check failed.',
        });

        this._renderTimeline();
    }

    _generateCommitment(data) {
        // Use the WASM merkle root as a commitment proxy
        try {
            const result = this.wasm.compute_merkle_root(JSON.stringify([data]));
            return result.root_hex;
        } catch {
            // Fallback: simple local hash
            return this._localHash(data);
        }
    }

    _generateNullifier(data) {
        // Derive nullifier from commitment data + secret
        const nullifierInput = 'nullifier:' + data;
        try {
            const result = this.wasm.compute_merkle_root(JSON.stringify([nullifierInput]));
            return result.root_hex;
        } catch {
            return this._localHash(nullifierInput);
        }
    }

    _localHash(str) {
        let h = 0x811c9dc5;
        for (let i = 0; i < str.length; i++) {
            h ^= str.charCodeAt(i);
            h = Math.imul(h, 0x01000193);
        }
        return (h >>> 0).toString(16).padStart(8, '0').repeat(8);
    }

    _addTimelineEvent(type, data) {
        this.timeline.push({
            type,
            time: new Date().toLocaleTimeString(),
            data,
        });
    }

    _updateState() {
        const unspent = this.notes.filter(n => !n.spent);
        const totalValue = unspent.reduce((sum, n) => sum + n.amount, 0);

        document.getElementById('ns-count').textContent = this.notes.length;
        document.getElementById('ns-nullifiers').textContent = this.nullifiers.size;
        document.getElementById('ns-value').textContent = totalValue;

        // Also update the main state explorer nullifiers
        document.dispatchEvent(new CustomEvent('notes-updated', {
            detail: {
                nullifiers: [...this.nullifiers],
            }
        }));
    }

    _renderTimeline() {
        const container = document.getElementById('note-timeline');

        if (this.timeline.length === 0) {
            container.innerHTML = `
                <div class="timeline-empty">
                    <p>Mint a note to begin the lifecycle demo.</p>
                    <p class="dim">You will see commitments, nullifiers, and transfers visualized step-by-step.</p>
                </div>
            `;
            return;
        }

        let html = '<div class="timeline-items">';

        // Render newest first
        for (let i = this.timeline.length - 1; i >= 0; i--) {
            const event = this.timeline[i];
            html += this._renderEvent(event);
        }

        html += '</div>';
        container.innerHTML = html;
    }

    _renderEvent(event) {
        const { type, time, data } = event;

        switch (type) {
            case 'mint':
                return `
                    <div class="timeline-item mint">
                        <div class="tl-header">
                            <span class="tl-type">MINT</span>
                            <span class="tl-time">${time}</span>
                        </div>
                        <div class="tl-body">
                            Note #${data.noteId}: <span class="tl-amount">${data.amount} ${data.asset}</span>
                            <br>Commitment: <span class="tl-hash">${data.commitment.slice(0, 32)}...</span>
                        </div>
                    </div>
                `;

            case 'transfer':
                return `
                    <div class="timeline-item transfer">
                        <div class="tl-header">
                            <span class="tl-type">TRANSFER</span>
                            <span class="tl-time">${time}</span>
                        </div>
                        <div class="tl-body">
                            <span class="tl-amount">${data.amount}</span> to ${data.recipient}
                            <br>Nullifier spent: <span class="tl-hash">${data.nullifierUsed.slice(0, 24)}...</span>
                            <br>New commitment: <span class="tl-hash">${data.newCommitment.slice(0, 24)}...</span>
                            ${data.change > 0 ? `<br>Change returned: <span class="tl-amount">${data.change}</span>` : ''}
                        </div>
                    </div>
                `;

            case 'error':
                return `
                    <div class="timeline-item error">
                        <div class="tl-header">
                            <span class="tl-type">REJECTED</span>
                            <span class="tl-time">${time}</span>
                        </div>
                        <div class="tl-body">
                            <span class="tl-reject">${data.reason}</span>
                            ${data.detail ? `<br>${data.detail}` : ''}
                            ${data.nullifier ? `<br>Nullifier: <span class="tl-hash">${data.nullifier.slice(0, 24)}...</span>` : ''}
                        </div>
                    </div>
                `;

            default:
                return '';
        }
    }
}
