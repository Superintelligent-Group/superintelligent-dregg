//! Length-prefixed framing codec for the wire protocol.
//!
//! Each message on the wire is framed as:
//!   [4-byte LE length][postcard-encoded payload]
//!
//! The length prefix encodes the payload size (not including the 4-byte header itself).
//! Maximum message size is 16 MiB to prevent memory exhaustion from malicious peers.

use crate::message::WireMessage;
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Maximum allowed message payload size: 16 MiB.
///
/// A presentation proof is ~24 KiB, so 16 MiB provides ample headroom for
/// batch operations while preventing memory exhaustion attacks.
pub const MAX_MESSAGE_SIZE: u32 = 16 * 1024 * 1024;

/// Framing header size: 4 bytes (little-endian u32).
pub const HEADER_SIZE: usize = 4;

/// Errors that can occur during codec operations.
#[derive(Debug)]
pub enum CodecError {
    /// An I/O error occurred on the underlying transport.
    Io(io::Error),
    /// The message payload exceeds the maximum allowed size.
    MessageTooLarge { size: u32, max: u32 },
    /// The message payload could not be deserialized.
    DeserializationFailed(postcard::Error),
    /// The message payload could not be serialized.
    SerializationFailed(postcard::Error),
    /// The connection was closed by the peer (EOF).
    ConnectionClosed,
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::MessageTooLarge { size, max } => {
                write!(f, "message too large: {size} bytes (max {max})")
            }
            Self::DeserializationFailed(e) => write!(f, "deserialization failed: {e}"),
            Self::SerializationFailed(e) => write!(f, "serialization failed: {e}"),
            Self::ConnectionClosed => write!(f, "connection closed by peer"),
        }
    }
}

impl std::error::Error for CodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::DeserializationFailed(e) => Some(e),
            Self::SerializationFailed(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for CodecError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            Self::ConnectionClosed
        } else {
            Self::Io(e)
        }
    }
}

// =============================================================================
// Encoding
// =============================================================================

/// Encode a WireMessage into a length-prefixed frame.
///
/// Returns the complete frame (header + payload) ready for writing to the wire.
pub fn encode(msg: &WireMessage) -> Result<Vec<u8>, CodecError> {
    let payload = postcard::to_stdvec(msg).map_err(CodecError::SerializationFailed)?;

    let payload_len = payload.len() as u32;
    if payload_len > MAX_MESSAGE_SIZE {
        return Err(CodecError::MessageTooLarge {
            size: payload_len,
            max: MAX_MESSAGE_SIZE,
        });
    }

    let mut frame = Vec::with_capacity(HEADER_SIZE + payload.len());
    frame.extend_from_slice(&payload_len.to_le_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Decode a WireMessage from a payload buffer (without the length prefix).
pub fn decode(payload: &[u8]) -> Result<WireMessage, CodecError> {
    postcard::from_bytes(payload).map_err(CodecError::DeserializationFailed)
}

// =============================================================================
// Async Stream Operations
// =============================================================================

/// Write a framed message to an async writer.
///
/// This performs a single logical write of the complete frame (header + payload).
/// Uses `write_all` to ensure the entire frame is transmitted.
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &WireMessage,
) -> Result<usize, CodecError> {
    let frame = encode(msg)?;
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(frame.len())
}

/// Read a framed message from an async reader.
///
/// Reads the 4-byte length prefix, validates it, then reads exactly that many
/// bytes of payload and deserializes the message.
pub async fn read_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<WireMessage, CodecError> {
    // Read the 4-byte length header
    let mut header = [0u8; HEADER_SIZE];
    match reader.read_exact(&mut header).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
            return Err(CodecError::ConnectionClosed);
        }
        Err(e) => return Err(CodecError::Io(e)),
    }

    let payload_len = u32::from_le_bytes(header);

    // Validate size
    if payload_len > MAX_MESSAGE_SIZE {
        return Err(CodecError::MessageTooLarge {
            size: payload_len,
            max: MAX_MESSAGE_SIZE,
        });
    }

    // Read the payload
    let mut payload = vec![0u8; payload_len as usize];
    reader.read_exact(&mut payload).await?;

    decode(&payload)
}

// =============================================================================
// Frame Statistics
// =============================================================================

/// Statistics about a frame for diagnostics.
#[derive(Clone, Debug)]
pub struct FrameStats {
    /// Total frame size (header + payload).
    pub total_bytes: usize,
    /// Payload size (without header).
    pub payload_bytes: usize,
    /// The message variant name.
    pub variant: &'static str,
}

impl FrameStats {
    /// Compute stats for a message.
    pub fn for_message(msg: &WireMessage) -> Result<Self, CodecError> {
        let payload = postcard::to_stdvec(msg).map_err(CodecError::SerializationFailed)?;
        Ok(Self {
            total_bytes: HEADER_SIZE + payload.len(),
            payload_bytes: payload.len(),
            variant: msg.variant_name(),
        })
    }

    /// Human-readable display of the payload size.
    pub fn size_display(&self) -> String {
        let bytes = self.payload_bytes;
        if bytes < 1024 {
            format!("{bytes} B")
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KiB", bytes as f64 / 1024.0)
        } else {
            format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{AuthorizationRequest, PROTOCOL_VERSION};

    #[test]
    fn encode_decode_roundtrip() {
        let msg = WireMessage::Hello {
            node_id: [0xaa; 32],
            node_name: "codec-test".to_string(),
            protocol_version: PROTOCOL_VERSION,
            capabilities: vec!["present".to_string()],
        };

        let frame = encode(&msg).unwrap();
        // First 4 bytes are the length
        let payload_len = u32::from_le_bytes(frame[..4].try_into().unwrap());
        assert_eq!(payload_len as usize, frame.len() - HEADER_SIZE);

        // Decode payload
        let decoded = decode(&frame[HEADER_SIZE..]).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn message_too_large_rejected() {
        // Create a message with a huge payload
        let msg = WireMessage::PresentToken {
            proof: vec![0u8; (MAX_MESSAGE_SIZE + 1) as usize],
            request: AuthorizationRequest::new("x", "y", "z"),
            federation_root: [0; 32],
        };

        let result = encode(&msg);
        assert!(matches!(result, Err(CodecError::MessageTooLarge { .. })));
    }

    #[tokio::test]
    async fn async_write_read_roundtrip() {
        let msg = WireMessage::RevocationAck {
            new_root: [0xbb; 32],
            height: 999,
        };

        // Use a duplex stream for testing
        let (mut client, mut server) = tokio::io::duplex(65536);

        // Write from one side
        let bytes_written = write_message(&mut client, &msg).await.unwrap();
        assert!(bytes_written > HEADER_SIZE);

        // Read from the other
        let decoded = read_message(&mut server).await.unwrap();
        assert_eq!(msg, decoded);
    }

    #[tokio::test]
    async fn multiple_messages_sequential() {
        let messages = vec![
            WireMessage::Ping { seq: 1, timestamp: 100 },
            WireMessage::Pong { seq: 1, timestamp: 101 },
            WireMessage::RequestAttestedRoot,
        ];

        let (mut client, mut server) = tokio::io::duplex(65536);

        for msg in &messages {
            write_message(&mut client, msg).await.unwrap();
        }

        for expected in &messages {
            let decoded = read_message(&mut server).await.unwrap();
            assert_eq!(*expected, decoded);
        }
    }

    #[tokio::test]
    async fn connection_closed_on_eof() {
        let (mut _client, mut server) = tokio::io::duplex(65536);
        drop(_client);

        let result = read_message(&mut server).await;
        assert!(matches!(result, Err(CodecError::ConnectionClosed)));
    }

    #[test]
    fn frame_stats_computation() {
        let msg = WireMessage::Hello {
            node_id: [0; 32],
            node_name: "stats-test".to_string(),
            protocol_version: 1,
            capabilities: vec![],
        };

        let stats = FrameStats::for_message(&msg).unwrap();
        assert_eq!(stats.variant, "Hello");
        assert!(stats.payload_bytes > 0);
        assert_eq!(stats.total_bytes, stats.payload_bytes + HEADER_SIZE);
    }
}
