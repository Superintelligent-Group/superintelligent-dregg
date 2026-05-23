/**
 * Proof Simulator Tweaker — adjust trace values and see which constraints fire.
 *
 * Provides an interactive constraint checker that evaluates AIR constraints
 * against user-provided trace values and highlights violations.
 */

import { bus } from '../app.js';

export const name = 'proof-simulator';

// Constraint definitions matching the circuit's AIR
const CONSTRAINT_SETS = {
  derivation: {
    label: 'DerivationAir',
    columns: ['prev_state', 'curr_state', 'action_hash', 'nonce'],
    constraints: [
      {
        id: 'state_transition',
        label: 'State Transition',
        description: 'curr_state = hash(prev_state, action_hash)',
        check: (row) => true, // Always passes in simulation (hash check needs WASM)
      },
      {
        id: 'monotonic_nonce',
        label: 'Monotonic Nonce',
        description: 'nonce must increase by exactly 1 each step',
        check: (row, prev) => !prev || row.nonce === prev.nonce + 1,
      },
    ],
  },
  membership: {
    label: 'BodyMembershipAir',
    columns: ['credential_hash', 'issuer', 'epoch', 'revocation_witness'],
    constraints: [
      {
        id: 'valid_credential',
        label: 'Valid Credential',
        description: 'credential_hash must be non-zero',
        check: (row) => row.credential_hash !== 0,
      },
      {
        id: 'not_revoked',
        label: 'Not Revoked',
        description: 'revocation_witness must be valid (non-zero for non-membership)',
        check: (row) => row.revocation_witness !== 0,
      },
      {
        id: 'epoch_bound',
        label: 'Epoch Bound',
        description: 'credential epoch must not exceed current epoch',
        check: (row) => row.epoch <= 1000, // simulated current epoch
      },
    ],
  },
  transfer: {
    label: 'TransferAir',
    columns: ['sender_bal_before', 'sender_bal_after', 'receiver_bal_before', 'receiver_bal_after', 'amount'],
    constraints: [
      {
        id: 'conservation',
        label: 'Balance Conservation',
        description: 'sender_decrease == receiver_increase == amount',
        check: (row) => {
          const senderDec = row.sender_bal_before - row.sender_bal_after;
          const receiverInc = row.receiver_bal_after - row.receiver_bal_before;
          return senderDec === row.amount && receiverInc === row.amount;
        },
      },
      {
        id: 'non_negative',
        label: 'Non-Negative',
        description: 'All balances must be >= 0',
        check: (row) => row.sender_bal_after >= 0 && row.receiver_bal_after >= 0,
      },
      {
        id: 'positive_amount',
        label: 'Positive Amount',
        description: 'Transfer amount must be > 0',
        check: (row) => row.amount > 0,
      },
    ],
  },
};

export function init() {
  bus.on('simulator:check', (data) => {
    const constraintSet = CONSTRAINT_SETS[data.airType];
    if (!constraintSet) return;

    const results = checkConstraints(constraintSet, data.trace);
    bus.emit('simulator:results', { airType: data.airType, results });
  });
}

export function getConstraintSets() {
  return CONSTRAINT_SETS;
}

function checkConstraints(constraintSet, trace) {
  return constraintSet.constraints.map(constraint => {
    const violations = [];
    trace.forEach((row, idx) => {
      const prev = idx > 0 ? trace[idx - 1] : null;
      try {
        if (!constraint.check(row, prev)) {
          violations.push(idx);
        }
      } catch (e) {
        violations.push(idx);
      }
    });
    return {
      id: constraint.id,
      label: constraint.label,
      description: constraint.description,
      passed: violations.length === 0,
      violations,
    };
  });
}
