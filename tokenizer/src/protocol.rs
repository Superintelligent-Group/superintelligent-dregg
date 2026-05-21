//! Protocol messages for the tokenizer daemon IPC.
//!
//! Uses postcard (length-prefixed) for wire encoding, consistent with `pyana-wire`.
//! Frame format: `[4-byte LE payload length][postcard-encoded message]`

use serde::{Deserialize, Serialize};

/// Maximum payload size: 1 MiB (secrets should be small).
pub const MAX_PAYLOAD_SIZE: u32 = 1024 * 1024;

/// Frame header size.
pub const HEADER_SIZE: usize = 4;

/// Request from client to daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Request {
    /// Encrypt plaintext with the daemon's current public key.
    Seal { plaintext: Vec<u8> },

    /// Decrypt a sealed secret using the daemon's private key(s).
    Unseal { sealed: Vec<u8> },

    /// Get the current (newest) public key.
    GetPublicKey,

    /// Rotate: generate a new keypair, return the new public key.
    /// Old keys are retained for decryption.
    Rotate,

    /// Graceful shutdown request.
    Shutdown,
}

/// Response from daemon to client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Response {
    /// Successful seal: contains the sealed bytes.
    Sealed { data: Vec<u8> },

    /// Successful unseal: contains the plaintext.
    Unsealed { plaintext: Vec<u8> },

    /// Public key response.
    PublicKey { key: [u8; 32] },

    /// Rotation completed: new public key.
    Rotated { new_public_key: [u8; 32] },

    /// Shutdown acknowledged.
    ShutdownAck,

    /// Error response.
    Error { message: String },
}
