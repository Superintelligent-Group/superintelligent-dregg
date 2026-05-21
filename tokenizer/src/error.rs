//! Tokenizer error types.

/// Errors from tokenizer operations.
#[derive(Debug, thiserror::Error)]
pub enum TokenizerError {
    #[error("encryption error: {0}")]
    Encryption(String),

    #[error("decryption error: {0}")]
    Decryption(String),

    #[error("encoding error: {0}")]
    Encoding(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("connection closed")]
    ConnectionClosed,

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("remote error: {0}")]
    Remote(String),
}
