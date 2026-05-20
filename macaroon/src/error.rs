//! Error types for macaroon operations.

use thiserror::Error;

/// Errors that can occur during macaroon operations.
#[derive(Debug, Error)]
pub enum MacaroonError {
  /// HMAC verification failed — the macaroon signature is invalid.
  #[error("signature verification failed")]
  SignatureInvalid,

  /// A caveat could not be cleared against the given access.
  #[error("caveat not satisfied: {0}")]
  CaveatNotSatisfied(String),

  /// A required third-party discharge was not provided.
  #[error("missing discharge for third-party caveat at {location}")]
  MissingDischarge { location: String },

  /// A discharge macaroon's signature is invalid.
  #[error("discharge verification failed for {location}")]
  DischargeInvalid { location: String },

  /// The discharge is not bound to the correct parent token.
  #[error("discharge not bound to parent token")]
  DischargeUnbound,

  /// Failed to decrypt a third-party ticket or verifier key.
  #[error("decryption failed: {0}")]
  DecryptionFailed(String),

  /// Failed to encrypt a third-party ticket or verifier key.
  #[error("encryption failed: {0}")]
  EncryptionFailed(String),

  /// Serialization/deserialization error.
  #[error("encoding error: {0}")]
  Encoding(String),

  /// The macaroon data is malformed.
  #[error("malformed macaroon: {0}")]
  Malformed(String),

  /// A caveat type is unknown or unregistered.
  #[error("unknown caveat type: {0}")]
  UnknownCaveatType(u16),

  /// The nonce or key material is invalid.
  #[error("invalid key material: {0}")]
  InvalidKeyMaterial(String),
}

/// Errors from individual caveat checks.
#[derive(Debug, Error)]
pub enum CaveatError {
  /// The access is prohibited by this caveat.
  #[error("{0}")]
  Prohibited(String),
}

/// Result type for macaroon operations.
pub type MacaroonResult<T> = Result<T, MacaroonError>;
