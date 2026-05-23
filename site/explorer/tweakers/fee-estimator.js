/**
 * Fee Estimator Tweaker — estimate computron cost for different turn types.
 *
 * Provides a calculator that estimates gas/computron costs based on:
 * - Turn type (hosted, sovereign, atomic multi-party)
 * - Number of effects
 * - Proof overhead (STARK proof size)
 * - Cross-federation bridging costs
 */

import { bus } from '../app.js';

export const name = 'fee-estimator';

// Cost model (computrons per operation)
const COST_MODEL = {
  base_turn: 1000,           // Base cost for any turn submission
  per_effect: 200,           // Per-effect execution cost
  per_byte: 2,              // Per-byte of turn payload
  signature_verify: 500,    // Signature verification
  proof_verify_stark: 5000, // STARK proof verification
  proof_verify_kimchi: 8000, // Kimchi proof verification (more expensive)
  merkle_path: 100,         // Per-level Merkle path verification
  capability_check: 300,    // Capability token validation
  delegation_chain: 150,    // Per-hop delegation chain traversal
  nullifier_insert: 400,    // Nullifier set insertion
  note_create: 600,         // Private note creation
  cross_fed_bridge: 3000,   // Cross-federation message relay
  conditional_register: 800, // Conditional turn registration
  bearer_auth: 200,         // Bearer token auth (cheaper than sig)
};

// Turn type presets
const TURN_PRESETS = {
  simple_transfer: {
    label: 'Simple Transfer',
    effects: 1,
    bytes: 128,
    auth: 'signature',
    proofs: 0,
    description: 'Single token transfer between two hosted cells',
  },
  sovereign_update: {
    label: 'Sovereign Cell Update',
    effects: 1,
    bytes: 256,
    auth: 'signature',
    proofs: 1,
    proof_type: 'stark',
    description: 'Sovereign cell state transition with proof',
  },
  private_transfer: {
    label: 'Private Transfer',
    effects: 3,
    bytes: 512,
    auth: 'signature',
    proofs: 1,
    proof_type: 'stark',
    nullifiers: 1,
    notes: 2,
    description: 'Private note-based transfer (spend + create)',
  },
  atomic_swap: {
    label: 'Atomic Multi-Party',
    effects: 4,
    bytes: 768,
    auth: 'multi_sig',
    proofs: 2,
    proof_type: 'stark',
    description: 'Atomic swap between two parties',
  },
  credential_present: {
    label: 'Credential Presentation',
    effects: 1,
    bytes: 384,
    auth: 'bearer',
    proofs: 1,
    proof_type: 'stark',
    capability_checks: 2,
    description: 'ZK credential presentation with cap check',
  },
  cross_fed: {
    label: 'Cross-Federation Bridge',
    effects: 2,
    bytes: 512,
    auth: 'signature',
    proofs: 1,
    proof_type: 'stark',
    bridge: true,
    description: 'Cross-federation asset bridge relay',
  },
};

export function init() {
  bus.on('estimator:calculate', (params) => {
    const estimate = calculateFee(params);
    bus.emit('estimator:result', estimate);
  });

  bus.on('estimator:preset', (presetName) => {
    const preset = TURN_PRESETS[presetName];
    if (preset) {
      const estimate = calculateFee(preset);
      bus.emit('estimator:result', { ...estimate, preset: presetName, description: preset.description });
    }
  });
}

export function getPresets() {
  return TURN_PRESETS;
}

export function getCostModel() {
  return COST_MODEL;
}

export function calculateFee(params) {
  let total = COST_MODEL.base_turn;
  const breakdown = [{ component: 'Base turn cost', cost: COST_MODEL.base_turn }];

  // Effects
  const effectCost = (params.effects || 1) * COST_MODEL.per_effect;
  total += effectCost;
  breakdown.push({ component: `${params.effects || 1} effect(s)`, cost: effectCost });

  // Payload size
  const byteCost = (params.bytes || 128) * COST_MODEL.per_byte;
  total += byteCost;
  breakdown.push({ component: `${params.bytes || 128} bytes payload`, cost: byteCost });

  // Auth
  if (params.auth === 'bearer') {
    total += COST_MODEL.bearer_auth;
    breakdown.push({ component: 'Bearer auth', cost: COST_MODEL.bearer_auth });
  } else if (params.auth === 'multi_sig') {
    const sigCost = COST_MODEL.signature_verify * 2;
    total += sigCost;
    breakdown.push({ component: 'Multi-sig verify (2)', cost: sigCost });
  } else {
    total += COST_MODEL.signature_verify;
    breakdown.push({ component: 'Signature verify', cost: COST_MODEL.signature_verify });
  }

  // Proofs
  if (params.proofs > 0) {
    const proofCostPer = params.proof_type === 'kimchi' ? COST_MODEL.proof_verify_kimchi : COST_MODEL.proof_verify_stark;
    const proofCost = params.proofs * proofCostPer;
    total += proofCost;
    breakdown.push({ component: `${params.proofs} ${params.proof_type || 'STARK'} proof(s)`, cost: proofCost });
  }

  // Capability checks
  if (params.capability_checks) {
    const capCost = params.capability_checks * COST_MODEL.capability_check;
    total += capCost;
    breakdown.push({ component: `${params.capability_checks} capability check(s)`, cost: capCost });
  }

  // Nullifiers
  if (params.nullifiers) {
    const nullCost = params.nullifiers * COST_MODEL.nullifier_insert;
    total += nullCost;
    breakdown.push({ component: `${params.nullifiers} nullifier insert(s)`, cost: nullCost });
  }

  // Notes
  if (params.notes) {
    const noteCost = params.notes * COST_MODEL.note_create;
    total += noteCost;
    breakdown.push({ component: `${params.notes} note creation(s)`, cost: noteCost });
  }

  // Bridge
  if (params.bridge) {
    total += COST_MODEL.cross_fed_bridge;
    breakdown.push({ component: 'Cross-federation bridge', cost: COST_MODEL.cross_fed_bridge });
  }

  // Conditional
  if (params.conditional) {
    total += COST_MODEL.conditional_register;
    breakdown.push({ component: 'Conditional registration', cost: COST_MODEL.conditional_register });
  }

  return {
    total,
    breakdown,
    label: params.label || 'Custom Turn',
  };
}
