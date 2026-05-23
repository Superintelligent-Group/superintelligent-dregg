/**
 * Effect Builder Tweaker — build custom effect sequences, see trace layout.
 *
 * Allows users to construct an effect sequence step-by-step and preview
 * the resulting execution trace that would be proved by the STARK.
 */

import { bus } from '../app.js';

export const name = 'effect-builder';

// Effect type definitions with their state columns and transition rules
const EFFECT_CATALOG = {
  transfer: {
    label: 'Transfer',
    description: 'Move tokens between two cells',
    inputs: ['sender', 'receiver', 'amount'],
    columns: ['sender_balance', 'receiver_balance', 'amount', 'nonce'],
    transition: (state, inputs) => ({
      sender_balance: state.sender_balance - inputs.amount,
      receiver_balance: state.receiver_balance + inputs.amount,
      amount: inputs.amount,
      nonce: state.nonce + 1,
    }),
  },
  mint: {
    label: 'Mint',
    description: 'Create new tokens (authority required)',
    inputs: ['receiver', 'amount', 'authority_proof'],
    columns: ['total_supply', 'receiver_balance', 'amount', 'authority_hash'],
    transition: (state, inputs) => ({
      total_supply: state.total_supply + inputs.amount,
      receiver_balance: state.receiver_balance + inputs.amount,
      amount: inputs.amount,
      authority_hash: state.authority_hash,
    }),
  },
  burn: {
    label: 'Burn',
    description: 'Destroy tokens, producing a nullifier',
    inputs: ['sender', 'amount'],
    columns: ['total_supply', 'sender_balance', 'amount', 'nullifier'],
    transition: (state, inputs) => ({
      total_supply: state.total_supply - inputs.amount,
      sender_balance: state.sender_balance - inputs.amount,
      amount: inputs.amount,
      nullifier: Math.floor(Math.random() * 0xFFFFFFFF),
    }),
  },
  delegate: {
    label: 'Delegate',
    description: 'Grant capability to another cell',
    inputs: ['delegator', 'delegate', 'capability_id'],
    columns: ['delegator_caps', 'delegate_caps', 'cap_id', 'expiry_height'],
    transition: (state, inputs) => ({
      delegator_caps: state.delegator_caps,
      delegate_caps: state.delegate_caps + 1,
      cap_id: inputs.capability_id,
      expiry_height: state.expiry_height || 999999,
    }),
  },
  create_note: {
    label: 'Create Note',
    description: 'Produce a private note commitment',
    inputs: ['value', 'asset_type', 'blinding'],
    columns: ['note_count', 'commitment', 'tree_root', 'value'],
    transition: (state, inputs) => ({
      note_count: state.note_count + 1,
      commitment: Math.floor(Math.random() * 0xFFFFFFFF),
      tree_root: Math.floor(Math.random() * 0xFFFFFFFF),
      value: inputs.value,
    }),
  },
};

export function init() {
  // Register with event bus so the effects view can find us
  bus.on('tweaker:build-effect', (data) => {
    const type = EFFECT_CATALOG[data.type];
    if (type) {
      const result = type.transition(data.state || getDefaultState(data.type), data.inputs || {});
      bus.emit('tweaker:effect-result', { type: data.type, result });
    }
  });
}

export function getEffectCatalog() {
  return EFFECT_CATALOG;
}

function getDefaultState(type) {
  switch (type) {
    case 'transfer': return { sender_balance: 1000, receiver_balance: 0, amount: 0, nonce: 0 };
    case 'mint': return { total_supply: 10000, receiver_balance: 0, amount: 0, authority_hash: 0xDEADBEEF };
    case 'burn': return { total_supply: 10000, sender_balance: 1000, amount: 0, nullifier: 0 };
    case 'delegate': return { delegator_caps: 3, delegate_caps: 0, cap_id: 0, expiry_height: 999999 };
    case 'create_note': return { note_count: 0, commitment: 0, tree_root: 0, value: 0 };
    default: return {};
  }
}
