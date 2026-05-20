//! Error types for the pyana SDK.

use pyana_token::TokenError;
use pyana_turn::TurnError;
use pyana_bridge::AuthError;

/// Unified error type for SDK operations.
#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    /// A token operation failed (minting, attenuation, verification).
    #[error("token error: {0}")]
    Token(#[from] TokenError),

    /// A turn execution failed (precondition, authorization, budget).
    #[error("turn error: {0}")]
    Turn(#[from] TurnError),

    /// Authorization or proof generation failed.
    #[error("auth/proof error: {0}")]
    Auth(#[from] AuthError),

    /// A wire protocol operation failed.
    #[error("wire error: {0}")]
    Wire(String),

    /// The wallet has no token matching the requested operation.
    #[error("no such token: {0}")]
    TokenNotFound(String),

    /// The wallet does not have the required key material.
    #[error("missing key material: {0}")]
    MissingKey(String),

    /// A delegation or attenuation was invalid.
    #[error("invalid delegation: {0}")]
    InvalidDelegation(String),

    /// The remote silo rejected the operation.
    #[error("silo rejected: {0}")]
    Rejected(String),
}
