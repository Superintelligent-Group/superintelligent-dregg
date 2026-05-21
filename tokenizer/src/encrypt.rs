//! X25519 + ChaCha20-Poly1305 encryption for secret tokenization.
//!
//! Implements sealed-box semantics: the sender encrypts with the recipient's
//! public key + an ephemeral keypair. The recipient decrypts with their
//! private key. The sender's identity is not revealed.
//!
//! **Note:** This is NOT NaCl/libsodium-compatible — it uses raw X25519 DH
//! output as the ChaCha20 key (NaCl uses HSalsa20-derived keys). This is
//! intentional; we only interop with ourselves.
//!
//! Wire format: `[32-byte ephemeral public key][12-byte nonce][ciphertext + 16-byte tag]`

use base64::Engine;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::error::TokenizerError;

/// Overhead added to ciphertext: 32 (ephemeral pk) + 12 (nonce) + 16 (tag).
pub const SEALED_OVERHEAD: usize = 32 + 12 + 16;

/// A Curve25519 keypair for the tokenizer.
pub struct TokenizerKeypair {
    secret: StaticSecret,
    public: PublicKey,
}

impl TokenizerKeypair {
    /// Generate a new random keypair.
    pub fn generate() -> Self {
        let mut key_bytes = Zeroizing::new([0u8; 32]);
        getrandom::fill(&mut *key_bytes).expect("getrandom failed");
        let secret = StaticSecret::from(*key_bytes);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Reconstruct from existing private key bytes.
    pub fn from_bytes(secret_bytes: [u8; 32]) -> Self {
        let secret = StaticSecret::from(secret_bytes);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Get the public key (safe to share).
    pub fn public_key(&self) -> &PublicKey {
        &self.public
    }

    /// Get the public key bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        *self.public.as_bytes()
    }

    /// Decrypt a sealed secret.
    pub fn unseal(&self, sealed: &SealedSecret) -> Result<Vec<u8>, TokenizerError> {
        sealed.open(&self.secret)
    }

    /// Decrypt from raw sealed bytes.
    pub fn unseal_bytes(&self, data: &[u8]) -> Result<Vec<u8>, TokenizerError> {
        let sealed = SealedSecret::from_bytes(data)?;
        self.unseal(&sealed)
    }
}

/// An encrypted secret (sealed box).
///
/// Wire format: `[32-byte ephemeral pk][12-byte nonce][ciphertext + tag]`
pub struct SealedSecret {
    /// The ephemeral public key used for this encryption.
    ephemeral_pk: [u8; 32],
    /// The 12-byte nonce.
    nonce: [u8; 12],
    /// The ciphertext + 16-byte Poly1305 tag.
    ciphertext: Vec<u8>,
}

impl SealedSecret {
    /// Encrypt plaintext to the given recipient public key.
    pub fn seal(plaintext: &[u8], recipient: &PublicKey) -> Result<Self, TokenizerError> {
        // Generate ephemeral keypair (StaticSecret used here because
        // EphemeralSecret can't be constructed from raw bytes — we generate
        // and immediately consume it, so the distinction is only semantic).
        let mut eph_bytes = Zeroizing::new([0u8; 32]);
        getrandom::fill(&mut *eph_bytes).map_err(|e| TokenizerError::Encryption(e.to_string()))?;
        let ephemeral_secret = StaticSecret::from(*eph_bytes);
        let ephemeral_pk = PublicKey::from(&ephemeral_secret);

        // Derive shared secret
        let shared = ephemeral_secret.diffie_hellman(recipient);

        // Use shared secret as ChaCha20-Poly1305 key
        let cipher = ChaCha20Poly1305::new(shared.as_bytes().into());

        // Random nonce
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).map_err(|e| TokenizerError::Encryption(e.to_string()))?;

        let ciphertext = cipher
            .encrypt((&nonce).into(), plaintext)
            .map_err(|e| TokenizerError::Encryption(e.to_string()))?;

        Ok(Self {
            ephemeral_pk: *ephemeral_pk.as_bytes(),
            nonce,
            ciphertext,
        })
    }

    /// Decrypt with the recipient's static secret key.
    pub fn open(&self, recipient_secret: &StaticSecret) -> Result<Vec<u8>, TokenizerError> {
        let ephemeral_pk = PublicKey::from(self.ephemeral_pk);
        let shared = recipient_secret.diffie_hellman(&ephemeral_pk);

        let cipher = ChaCha20Poly1305::new(shared.as_bytes().into());

        cipher
            .decrypt((&self.nonce).into(), self.ciphertext.as_ref())
            .map_err(|_| TokenizerError::Decryption("authentication failed".into()))
    }

    /// Serialize to wire format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SEALED_OVERHEAD + self.ciphertext.len());
        out.extend_from_slice(&self.ephemeral_pk);
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.ciphertext);
        out
    }

    /// Deserialize from wire format.
    pub fn from_bytes(data: &[u8]) -> Result<Self, TokenizerError> {
        if data.len() < SEALED_OVERHEAD {
            return Err(TokenizerError::Encoding(format!(
                "sealed secret too short: {} bytes (need at least {})",
                data.len(),
                SEALED_OVERHEAD
            )));
        }

        let mut ephemeral_pk = [0u8; 32];
        ephemeral_pk.copy_from_slice(&data[..32]);

        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&data[32..44]);

        let ciphertext = data[44..].to_vec();

        Ok(Self {
            ephemeral_pk,
            nonce,
            ciphertext,
        })
    }

    /// Encode to base64 for transport.
    pub fn to_base64(&self) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(self.to_bytes())
    }

    /// Decode from base64.
    pub fn from_base64(encoded: &str) -> Result<Self, TokenizerError> {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|e| TokenizerError::Encoding(e.to_string()))?;
        Self::from_bytes(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seal_unseal_roundtrip() {
        let kp = TokenizerKeypair::generate();
        let plaintext = b"sk-secret-api-key-12345";

        let sealed = SealedSecret::seal(plaintext, kp.public_key()).unwrap();
        let decrypted = kp.unseal(&sealed).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_bytes_roundtrip() {
        let kp = TokenizerKeypair::generate();
        let plaintext = b"hello tokenizer";

        let sealed = SealedSecret::seal(plaintext, kp.public_key()).unwrap();
        let bytes = sealed.to_bytes();

        assert!(bytes.len() >= SEALED_OVERHEAD);
        assert_eq!(bytes.len(), SEALED_OVERHEAD + plaintext.len());

        let decoded = SealedSecret::from_bytes(&bytes).unwrap();
        let decrypted = kp.unseal(&decoded).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_base64_roundtrip() {
        let kp = TokenizerKeypair::generate();
        let plaintext = b"base64-test-secret";

        let sealed = SealedSecret::seal(plaintext, kp.public_key()).unwrap();
        let b64 = sealed.to_base64();

        let decoded = SealedSecret::from_base64(&b64).unwrap();
        let decrypted = kp.unseal(&decoded).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let kp1 = TokenizerKeypair::generate();
        let kp2 = TokenizerKeypair::generate();

        let sealed = SealedSecret::seal(b"secret", kp1.public_key()).unwrap();
        let result = kp2.unseal(&sealed);
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let kp = TokenizerKeypair::generate();
        let sealed = SealedSecret::seal(b"secret", kp.public_key()).unwrap();
        let mut bytes = sealed.to_bytes();

        // Tamper with the last byte of ciphertext
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;

        let tampered = SealedSecret::from_bytes(&bytes).unwrap();
        assert!(kp.unseal(&tampered).is_err());
    }

    #[test]
    fn test_from_bytes_keypair() {
        let kp1 = TokenizerKeypair::generate();
        let pk_bytes = kp1.public_key_bytes();

        // Reconstruct from secret bytes (we can't easily extract them from StaticSecret,
        // but we can test from_bytes with known bytes)
        let mut key_bytes = [0u8; 32];
        getrandom::fill(&mut key_bytes).unwrap();
        let kp2 = TokenizerKeypair::from_bytes(key_bytes);

        // Keys should be valid
        assert_ne!(pk_bytes, kp2.public_key_bytes());

        // Should be able to seal/unseal with reconstructed keys
        let sealed = SealedSecret::seal(b"test", kp2.public_key()).unwrap();
        let decrypted = kp2.unseal(&sealed).unwrap();
        assert_eq!(decrypted, b"test");
    }

    #[test]
    fn test_empty_plaintext() {
        let kp = TokenizerKeypair::generate();
        let sealed = SealedSecret::seal(b"", kp.public_key()).unwrap();
        let decrypted = kp.unseal(&sealed).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_large_plaintext() {
        let kp = TokenizerKeypair::generate();
        let plaintext = vec![0xABu8; 4096];
        let sealed = SealedSecret::seal(&plaintext, kp.public_key()).unwrap();
        let decrypted = kp.unseal(&sealed).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_unseal_bytes_shortcut() {
        let kp = TokenizerKeypair::generate();
        let plaintext = b"shortcut-test";
        let sealed = SealedSecret::seal(plaintext, kp.public_key()).unwrap();
        let bytes = sealed.to_bytes();

        let decrypted = kp.unseal_bytes(&bytes).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_too_short_bytes_fails() {
        let result = SealedSecret::from_bytes(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_unique_sealing() {
        let kp = TokenizerKeypair::generate();
        let plaintext = b"same-plaintext";

        let sealed1 = SealedSecret::seal(plaintext, kp.public_key()).unwrap();
        let sealed2 = SealedSecret::seal(plaintext, kp.public_key()).unwrap();

        // Same plaintext, different ephemeral keys → different ciphertext
        assert_ne!(sealed1.to_bytes(), sealed2.to_bytes());

        // Both decrypt to same plaintext
        assert_eq!(kp.unseal(&sealed1).unwrap(), plaintext);
        assert_eq!(kp.unseal(&sealed2).unwrap(), plaintext);
    }
}
