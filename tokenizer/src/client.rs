//! Client library for the tokenizer daemon.
//!
//! Connects to the Unix socket and provides async methods for seal/unseal/rotate.
//! Handles reconnection on socket errors.

use std::path::{Path, PathBuf};

use tokio::net::UnixStream;

use crate::error::TokenizerError;
use crate::protocol::{Request, Response};
use crate::service::{ServiceConfig, read_frame, write_frame};

/// Client for the tokenizer daemon.
pub struct TokenizerClient {
    socket_path: PathBuf,
    stream: Option<UnixStream>,
}

impl Default for TokenizerClient {
    fn default() -> Self {
        Self::new(ServiceConfig::default_socket_path())
    }
}

impl TokenizerClient {
    /// Create a new client targeting the given socket path.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            stream: None,
        }
    }

    /// Connect (or reconnect) to the daemon.
    pub async fn connect(&mut self) -> Result<(), TokenizerError> {
        let stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            TokenizerError::Protocol(format!(
                "failed to connect to {}: {}",
                self.socket_path.display(),
                e
            ))
        })?;
        self.stream = Some(stream);
        Ok(())
    }

    /// Ensure we have a live connection, reconnecting if needed.
    async fn ensure_connected(&mut self) -> Result<(), TokenizerError> {
        if self.stream.is_none() {
            self.connect().await?;
        }
        Ok(())
    }

    /// Send a request and receive a response, with automatic reconnect on failure.
    async fn request(&mut self, req: &Request) -> Result<Response, TokenizerError> {
        self.ensure_connected().await?;

        let stream = self.stream.as_mut().unwrap();
        match write_frame(stream, req).await {
            Ok(()) => {}
            Err(_) => {
                // Connection may have died; reconnect and retry once.
                self.stream = None;
                self.connect().await?;
                let stream = self.stream.as_mut().unwrap();
                write_frame(stream, req).await?;
            }
        }

        let stream = self.stream.as_mut().unwrap();
        let response: Response = read_frame(stream).await.inspect_err(|_| {
            self.stream = None;
        })?;

        // Check for error responses.
        if let Response::Error { ref message } = response {
            return Err(TokenizerError::Remote(message.clone()));
        }

        Ok(response)
    }

    /// Seal plaintext — encrypt with the daemon's current public key.
    pub async fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, TokenizerError> {
        let resp = self
            .request(&Request::Seal {
                plaintext: plaintext.to_vec(),
            })
            .await?;

        match resp {
            Response::Sealed { data } => Ok(data),
            other => Err(TokenizerError::Protocol(format!(
                "unexpected response to Seal: {:?}",
                other
            ))),
        }
    }

    /// Unseal a sealed secret — decrypt using the daemon's private key(s).
    pub async fn unseal(&mut self, sealed: &[u8]) -> Result<Vec<u8>, TokenizerError> {
        let resp = self
            .request(&Request::Unseal {
                sealed: sealed.to_vec(),
            })
            .await?;

        match resp {
            Response::Unsealed { plaintext } => Ok(plaintext),
            other => Err(TokenizerError::Protocol(format!(
                "unexpected response to Unseal: {:?}",
                other
            ))),
        }
    }

    /// Get the daemon's current public key.
    pub async fn get_public_key(&mut self) -> Result<[u8; 32], TokenizerError> {
        let resp = self.request(&Request::GetPublicKey).await?;

        match resp {
            Response::PublicKey { key } => Ok(key),
            other => Err(TokenizerError::Protocol(format!(
                "unexpected response to GetPublicKey: {:?}",
                other
            ))),
        }
    }

    /// Rotate the daemon's keypair, returning the new public key.
    pub async fn rotate(&mut self) -> Result<[u8; 32], TokenizerError> {
        let resp = self.request(&Request::Rotate).await?;

        match resp {
            Response::Rotated { new_public_key } => Ok(new_public_key),
            other => Err(TokenizerError::Protocol(format!(
                "unexpected response to Rotate: {:?}",
                other
            ))),
        }
    }

    /// Request graceful shutdown of the daemon.
    pub async fn shutdown(&mut self) -> Result<(), TokenizerError> {
        let resp = self.request(&Request::Shutdown).await?;

        match resp {
            Response::ShutdownAck => Ok(()),
            other => Err(TokenizerError::Protocol(format!(
                "unexpected response to Shutdown: {:?}",
                other
            ))),
        }
    }

    /// Get the socket path this client is configured for.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}
