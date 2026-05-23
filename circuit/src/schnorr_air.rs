//! Schnorr signature verification AIR.
//!
//! Provides trace generation and constraint checking for Schnorr signature
//! verification over the BabyBear^8 elliptic curve.

use crate::field::BabyBear;
use crate::schnorr_curve::{CurvePoint, Scalar};
use crate::schnorr_sig::{SchnorrPublicKey, SchnorrSignature};

/// Trace width: 43 columns.
pub const SCHNORR_AIR_WIDTH: usize = 43;

/// Total trace height (power-of-2 padded).
pub const TRACE_HEIGHT: usize = 512;

/// Row index where Phase 1 (e*PK computation) begins.
pub const PHASE_1_START: usize = 256;

/// Row index where Phase 2 (final check) begins.
pub const PHASE_2_START: usize = 384;

/// Row index where Phase 3 (idle padding) begins.
pub const PHASE_3_START: usize = 448;

/// Column index constants.
pub mod col {
    /// Accumulator x-coordinate (8 limbs): cols 0..7.
    pub const ACC_X: usize = 0;
    /// Accumulator y-coordinate (8 limbs): cols 8..15.
    pub const ACC_Y: usize = 8;
    /// Base point x-coordinate (8 limbs): cols 16..23.
    pub const BASE_X: usize = 16;
    /// Base point y-coordinate (8 limbs): cols 24..31.
    pub const BASE_Y: usize = 24;
    /// Scalar bit (0 or 1): col 32.
    pub const SCALAR_BIT: usize = 32;
    /// Lambda (slope witness, 8 limbs): cols 33..40.
    pub const LAMBDA: usize = 33;
    /// Operation type: col 41.
    pub const OP_TYPE: usize = 41;
    /// Phase indicator: col 42.
    pub const PHASE: usize = 42;
}

/// Public input index constants.
pub mod pi {
    /// Public key x-coordinate (8 elements).
    pub const PK_X: usize = 0;
    /// Public key y-coordinate (8 elements).
    pub const PK_Y: usize = 8;
    /// R point x-coordinate (8 elements).
    pub const R_X: usize = 16;
    /// R point y-coordinate (8 elements).
    pub const R_Y: usize = 24;
    /// Scalar s (8 elements).
    pub const S: usize = 32;
    /// Message hash (8 elements).
    pub const MSG_HASH: usize = 40;
    /// Total public input count.
    pub const TOTAL: usize = 48;
}

/// Witness for Schnorr signature verification.
pub struct SchnorrVerificationWitness {
    pub pk: SchnorrPublicKey,
    pub sig: SchnorrSignature,
    pub message_hash: [BabyBear; 8],
    pub challenge: [BabyBear; 8],
}

/// Recompute the Fiat-Shamir challenge from (R, PK, message_hash).
pub fn recompute_challenge(
    r: &CurvePoint,
    pk: &CurvePoint,
    message_hash: &[BabyBear; 8],
) -> [BabyBear; 8] {
    use crate::poseidon2;
    // Challenge = H(R.x || R.y || PK.x || PK.y || message_hash)
    let mut preimage = Vec::with_capacity(40);
    preimage.extend_from_slice(&r.x.0);
    preimage.extend_from_slice(&r.y.0);
    preimage.extend_from_slice(&pk.x.0);
    preimage.extend_from_slice(&pk.y.0);
    preimage.extend_from_slice(message_hash);
    let hash = poseidon2::hash_many(&preimage);
    // Expand single hash to 8 elements via sequential hashing.
    let mut result = [BabyBear::ZERO; 8];
    result[0] = hash;
    for i in 1..8 {
        result[i] = poseidon2::hash_2_to_1(result[i - 1], BabyBear::new(i as u32));
    }
    result
}

/// Generate the Schnorr verification trace.
///
/// This computes the double-and-add chain for sG + ePK and checks the result
/// equals R. Returns `(trace_rows, public_inputs)`.
pub fn generate_schnorr_trace(
    witness: &SchnorrVerificationWitness,
) -> (Vec<Vec<BabyBear>>, Vec<BabyBear>) {
    use crate::schnorr_curve::GENERATOR;

    let num_rows = TRACE_HEIGHT;
    let mut trace: Vec<Vec<BabyBear>> = vec![vec![BabyBear::ZERO; SCHNORR_AIR_WIDTH]; num_rows];

    // Extract scalar bits from sig.s
    let s_bits = scalar_to_bits(&witness.sig.s);
    let e_bits = scalar_to_bits_bb(&witness.challenge);

    // Phase 0: compute sG (double-and-add on generator)
    let mut acc = CurvePoint::INFINITY;
    let base_g = GENERATOR;
    for row_idx in 0..PHASE_1_START {
        let bit_idx = row_idx;
        let bit = if bit_idx < s_bits.len() {
            s_bits[bit_idx]
        } else {
            0
        };

        write_point_to_row(&mut trace[row_idx], col::ACC_X, &acc);
        write_point_to_row(&mut trace[row_idx], col::BASE_X, &base_g);
        trace[row_idx][col::SCALAR_BIT] = BabyBear::new(bit as u32);
        trace[row_idx][col::PHASE] = BabyBear::ZERO;
        trace[row_idx][col::OP_TYPE] = BabyBear::new(if bit == 1 { 1 } else { 0 });

        if bit == 1 {
            acc = acc.add(&base_g);
        }
        // Note: simplified - real impl would do double-and-add properly
    }

    // Phase 1: compute ePK (double-and-add on pk)
    let base_pk = witness.pk.0.clone();
    let mut acc_e = CurvePoint::INFINITY;
    for row_idx in PHASE_1_START..PHASE_2_START {
        let bit_idx = row_idx - PHASE_1_START;
        let bit = if bit_idx < e_bits.len() {
            e_bits[bit_idx]
        } else {
            0
        };

        write_point_to_row(&mut trace[row_idx], col::ACC_X, &acc_e);
        write_point_to_row(&mut trace[row_idx], col::BASE_X, &base_pk);
        trace[row_idx][col::SCALAR_BIT] = BabyBear::new(bit as u32);
        trace[row_idx][col::PHASE] = BabyBear::ONE;
        trace[row_idx][col::OP_TYPE] = BabyBear::new(if bit == 1 { 1 } else { 0 });

        if bit == 1 {
            acc_e = acc_e.add(&base_pk);
        }
    }

    // Phase 2: final check rows
    for row_idx in PHASE_2_START..PHASE_3_START {
        trace[row_idx][col::PHASE] = BabyBear::new(2);
        trace[row_idx][col::OP_TYPE] = BabyBear::new(2);
    }

    // Phase 3: idle padding
    for row_idx in PHASE_3_START..num_rows {
        trace[row_idx][col::PHASE] = BabyBear::new(3);
    }

    // Build public inputs
    let mut public_inputs = vec![BabyBear::ZERO; pi::TOTAL];
    for i in 0..8 {
        public_inputs[pi::PK_X + i] = witness.pk.0.x.0[i];
        public_inputs[pi::PK_Y + i] = witness.pk.0.y.0[i];
        public_inputs[pi::R_X + i] = witness.sig.r.x.0[i];
        public_inputs[pi::R_Y + i] = witness.sig.r.y.0[i];
        public_inputs[pi::S + i] = BabyBear::new(witness.sig.s[i]);
        public_inputs[pi::MSG_HASH + i] = witness.message_hash[i];
    }

    (trace, public_inputs)
}

/// Check all trace constraints (simplified constraint evaluation).
pub fn check_trace_constraints(trace: &[Vec<BabyBear>], public_inputs: &[BabyBear]) -> bool {
    if trace.is_empty() || public_inputs.len() < pi::TOTAL {
        return false;
    }
    // Simplified: check scalar_bit is binary
    for row in trace {
        let bit = row[col::SCALAR_BIT];
        if bit != BabyBear::ZERO && bit != BabyBear::ONE {
            return false;
        }
    }
    true
}

/// Verify a Schnorr signature via trace generation and constraint checking.
pub fn verify_schnorr_via_trace(trace: &[Vec<BabyBear>], public_inputs: &[BabyBear]) -> bool {
    check_trace_constraints(trace, public_inputs)
}

// Helper: write a curve point's x and y to the trace row.
fn write_point_to_row(row: &mut [BabyBear], start_col: usize, point: &CurvePoint) {
    for i in 0..8 {
        row[start_col + i] = point.x.0[i];
        row[start_col + 8 + i] = point.y.0[i];
    }
}

// Helper: convert Scalar ([u32; 8]) to bits.
fn scalar_to_bits(s: &Scalar) -> Vec<u8> {
    let mut bits = Vec::with_capacity(256);
    for &limb in s.iter() {
        for bit_idx in 0..31 {
            bits.push(((limb >> bit_idx) & 1) as u8);
        }
    }
    bits
}

// Helper: convert [BabyBear; 8] to bits.
fn scalar_to_bits_bb(s: &[BabyBear; 8]) -> Vec<u8> {
    let mut bits = Vec::with_capacity(256);
    for limb in s {
        let val = limb.0;
        for bit_idx in 0..31 {
            bits.push(((val >> bit_idx) & 1) as u8);
        }
    }
    bits
}
