//! Field element representation.
//!
//! A field element is a 253-bit value stored as 32 bytes (big-endian, top 3 bits always zero).
//! This is compatible with BN254/BLS12-381 scalar fields. Symbols are mapped to field elements
//! via BLAKE3 hash truncated to 253 bits. Integers map directly.

use serde::{Deserialize, Serialize};

/// A field element: 253-bit value in 32 bytes (big-endian).
///
/// The top 3 bits of byte 0 are always cleared, ensuring the value fits within
/// any 253+ bit prime field.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FieldElement(pub [u8; 32]);

impl FieldElement {
    /// The zero element.
    pub const ZERO: Self = Self([0u8; 32]);

    /// Create a field element from raw bytes, masking to 253 bits.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        let mut fe = Self(bytes);
        fe.truncate_to_253();
        fe
    }

    /// Create a field element from an integer (little-endian encoding into the field).
    pub fn from_u64(val: u64) -> Self {
        let mut bytes = [0u8; 32];
        // Store as big-endian in the last 8 bytes.
        bytes[24..32].copy_from_slice(&val.to_be_bytes());
        Self(bytes)
    }

    /// Create a field element from a signed integer.
    pub fn from_i64(val: i64) -> Self {
        if val >= 0 {
            Self::from_u64(val as u64)
        } else {
            // Negative values: store two's complement representation truncated to 253 bits.
            let mut bytes = [0xFF; 32];
            bytes[24..32].copy_from_slice(&val.to_be_bytes());
            let mut fe = Self(bytes);
            fe.truncate_to_253();
            fe
        }
    }

    /// Create a field element by hashing a string symbol with BLAKE3, truncated to 253 bits.
    pub fn from_symbol(s: &str) -> Self {
        let hash = blake3::hash(s.as_bytes());
        Self::from_bytes(*hash.as_bytes())
    }

    /// Return the raw 32-byte big-endian representation.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Check if this is the zero element.
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }

    /// Truncate to 253 bits by clearing the top 3 bits.
    fn truncate_to_253(&mut self) {
        self.0[0] &= 0x1F; // 0b0001_1111 — clears top 3 bits
    }
}

impl core::fmt::Debug for FieldElement {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "FE(0x")?;
        for byte in &self.0[..4] {
            write!(f, "{byte:02x}")?;
        }
        write!(f, "...)")
    }
}

impl core::fmt::Display for FieldElement {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x")?;
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl Default for FieldElement {
    fn default() -> Self {
        Self::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_zero() {
        assert!(FieldElement::ZERO.is_zero());
        assert_eq!(FieldElement::ZERO.0, [0u8; 32]);
    }

    #[test]
    fn from_u64_roundtrip() {
        let fe = FieldElement::from_u64(42);
        assert!(!fe.is_zero());
        // Big-endian: 42 should be in the last byte.
        assert_eq!(fe.0[31], 42);
        assert_eq!(fe.0[30], 0);
    }

    #[test]
    fn from_symbol_deterministic() {
        let a = FieldElement::from_symbol("hello");
        let b = FieldElement::from_symbol("hello");
        let c = FieldElement::from_symbol("world");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn truncation_clears_top_bits() {
        let bytes = [0xFF; 32];
        let fe = FieldElement::from_bytes(bytes);
        // Top 3 bits should be zero.
        assert_eq!(fe.0[0] & 0xE0, 0);
    }

    #[test]
    fn from_i64_negative() {
        let fe = FieldElement::from_i64(-1);
        assert!(!fe.is_zero());
        // Should have top 3 bits cleared.
        assert_eq!(fe.0[0] & 0xE0, 0);
    }
}
