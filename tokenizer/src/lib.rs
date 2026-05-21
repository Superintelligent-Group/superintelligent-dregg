//! Tokenizer — sealed secret encryption for credential isolation.
//!
//! Secrets are encrypted with X25519 + ChaCha20-Poly1305 (NaCl box).
//! The tokenizer holds a Curve25519 private key; the runtime holds only
//! the public key and encrypts secrets before storage. Plaintext
//! credentials are never exposed to guest code.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────┐          ┌───────────────────────┐
//! │ Guest runtime    │          │ Tokenizer (daemon)    │
//! │                  │          │                       │
//! │ Has: capability  │──IPC────►│ 1. Verify cap token   │
//! │      token only  │          │ 2. Decrypt secret     │
//! │                  │◄─────────│ 3. Return plaintext   │
//! │ Never sees:      │          │                       │
//! │   private key    │          │ Holds: private key    │
//! └──────────────────┘          └───────────────────────┘
//! ```

pub mod client;
pub mod encrypt;
pub mod error;
pub mod protocol;
pub mod service;

pub use client::TokenizerClient;
pub use encrypt::{SealedSecret, TokenizerKeypair};
pub use error::TokenizerError;
pub use service::TokenizerService;
