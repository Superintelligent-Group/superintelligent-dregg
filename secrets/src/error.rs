//! Error types for secret store operations.

use thiserror::Error;

/// Errors from secret store operations.
#[derive(Debug, Error)]
pub enum SecretStoreError {
    /// I/O error (file operations).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Encryption or decryption failed.
    #[error("crypto error: {0}")]
    Crypto(String),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Keychain/OS credential store error.
    #[error("keychain error: {0}")]
    Keychain(String),

    /// The secret store path is invalid or inaccessible.
    #[error("store path error: {0}")]
    StorePath(String),

    /// A secret was not found.
    #[error("secret not found: {namespace}/{key}")]
    NotFound { namespace: String, key: String },

    /// The master key is unavailable.
    #[error("master key unavailable: {0}")]
    MasterKeyUnavailable(String),
}
