// Datalog Evaluator — step-by-step derivation animation

export class DatalogEvaluator {
    constructor(wasm) {
        this.wasm = wasm;
        this._bind();
    }

    _bind() {
        document.getElementById('btn-datalog-eval').addEventListener('click', () => this._evaluate());
    }

    _evaluate() {
        const factsStr = document.getElementById('datalog-facts').value.trim();
        const requestStr = document.getElementById('datalog-request').value.trim();
        const traceEl = document.getElementById('datalog-trace');

        // Validate JSON
        try {
            JSON.parse(factsStr);
        } catch (e) {
            this._showError(traceEl, 'Invalid facts JSON: ' + e.message);
            return;
        }
        try {
            JSON.parse(requestStr);
        } catch (e) {
            this._showError(traceEl, 'Invalid request JSON: ' + e.message);
            return;
        }

        try {
            const result = this.wasm.evaluate_datalog(factsStr, requestStr);
            this._renderDerivation(traceEl, result, JSON.parse(requestStr));
        } catch (e) {
            this._showError(traceEl, 'Evaluation error: ' + (e.message || String(e)));
        }
    }

    _renderDerivation(container, result, request) {
        const isAllow = result.conclusion === 'allow';
        const conclusionClass = isAllow ? 'allow' : 'deny';
        const conclusionText = isAllow
            ? `ALLOW (rule #${result.policy_rule_id})`
            : 'DENY (no matching policy)';

        let html = `
            <div class="derivation-result ${conclusionClass}">
                ${conclusionText}
            </div>
        `;

        // Request summary
        html += `
            <div style="margin-bottom: var(--gap-lg); font-family: var(--mono); font-size: 11px; color: var(--text-dim);">
                <strong style="color: var(--text);">Request:</strong>
                app_id=${request.app_id || '*'}, action=${request.action || '*'}, service=${request.service || '*'}
            </div>
        `;

        // Steps
        if (result.steps && result.steps.length > 0) {
            html += '<div class="derivation-steps">';
            result.steps.forEach((step, i) => {
                const delay = i * 100; // stagger animation
                html += `
                    <div class="derivation-step" style="animation-delay: ${delay}ms">
                        <span class="step-num">${i + 1}.</span>
                        <div class="step-content">
                            <div class="step-rule">rule[${step.rule_id}] fired</div>
                            <div class="step-derived">derived: ${step.derived_predicate_hex.slice(0, 32)}...</div>
                            <div class="step-bindings">${step.num_bindings} binding${step.num_bindings !== 1 ? 's' : ''} matched</div>
                        </div>
                    </div>
                `;
            });
            html += '</div>';
        } else {
            html += `
                <div style="font-family: var(--mono); font-size: 11px; color: var(--text-muted); padding: var(--gap-md);">
                    No derivation steps (${isAllow ? 'directly matched' : 'no rules fired'})
                </div>
            `;
        }

        // Total steps summary
        html += `
            <div style="margin-top: var(--gap-lg); padding-top: var(--gap-md); border-top: 1px solid var(--border); font-family: var(--mono); font-size: 10px; color: var(--text-muted);">
                Total derivation steps: ${result.num_derivation_steps}
            </div>
        `;

        container.innerHTML = html;
    }

    _showError(container, msg) {
        container.innerHTML = `
            <div class="derivation-result deny">
                ERROR
            </div>
            <div style="font-family: var(--mono); font-size: 11px; color: var(--danger); padding: var(--gap-md);">
                ${msg}
            </div>
        `;
    }
}
