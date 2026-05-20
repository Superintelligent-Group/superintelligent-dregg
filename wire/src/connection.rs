//! Peer connection abstraction for the wire protocol.
//!
//! `PeerConnection` wraps a TCP stream and provides typed send/recv operations
//! using the length-prefixed codec. It manages connection state and provides
//! diagnostic information about the connection.

use crate::codec::{self, CodecError, FrameStats};
use crate::message::WireMessage;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;

// =============================================================================
// Connection Error
// =============================================================================

/// Errors that can occur on a peer connection.
#[derive(Debug)]
pub enum ConnectionError {
    /// Failed to establish TCP connection.
    ConnectFailed(std::io::Error),
    /// A codec-level error (framing, serialization, size limit).
    Codec(CodecError),
    /// The connection timed out.
    Timeout,
    /// The connection has been closed.
    Closed,
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectFailed(e) => write!(f, "connection failed: {e}"),
            Self::Codec(e) => write!(f, "codec error: {e}"),
            Self::Timeout => write!(f, "operation timed out"),
            Self::Closed => write!(f, "connection closed"),
        }
    }
}

impl std::error::Error for ConnectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ConnectFailed(e) => Some(e),
            Self::Codec(e) => Some(e),
            _ => None,
        }
    }
}

impl From<CodecError> for ConnectionError {
    fn from(e: CodecError) -> Self {
        match e {
            CodecError::ConnectionClosed => Self::Closed,
            other => Self::Codec(other),
        }
    }
}

// =============================================================================
// Connection Statistics
// =============================================================================

/// Accumulated statistics for a peer connection.
#[derive(Clone, Debug)]
pub struct ConnectionStats {
    /// Total messages sent.
    pub messages_sent: u64,
    /// Total messages received.
    pub messages_received: u64,
    /// Total bytes sent (including framing overhead).
    pub bytes_sent: u64,
    /// Total bytes received (including framing overhead).
    pub bytes_received: u64,
    /// When the connection was established.
    pub connected_at: Instant,
    /// Remote address.
    pub remote_addr: SocketAddr,
}

impl ConnectionStats {
    fn new(remote_addr: SocketAddr) -> Self {
        Self {
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            connected_at: Instant::now(),
            remote_addr,
        }
    }

    /// How long this connection has been alive.
    pub fn uptime(&self) -> Duration {
        self.connected_at.elapsed()
    }
}

// =============================================================================
// Peer Connection
// =============================================================================

/// A bidirectional connection to a peer silo over TCP.
///
/// Wraps a TCP stream and provides typed message send/recv using the
/// length-prefixed postcard codec.
pub struct PeerConnection {
    reader: ReadHalf<TcpStream>,
    writer: WriteHalf<TcpStream>,
    stats: ConnectionStats,
}

impl PeerConnection {
    /// Connect to a peer at the given address.
    ///
    /// Establishes a TCP connection and returns a ready-to-use PeerConnection.
    pub async fn connect(addr: &str) -> Result<Self, ConnectionError> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(ConnectionError::ConnectFailed)?;

        stream.set_nodelay(true).ok(); // Best-effort; non-critical if it fails

        let remote_addr = stream
            .peer_addr()
            .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 0)));

        let (reader, writer) = tokio::io::split(stream);

        Ok(Self {
            reader,
            writer,
            stats: ConnectionStats::new(remote_addr),
        })
    }

    /// Connect to a peer with a timeout.
    pub async fn connect_timeout(addr: &str, timeout: Duration) -> Result<Self, ConnectionError> {
        match tokio::time::timeout(timeout, Self::connect(addr)).await {
            Ok(result) => result,
            Err(_) => Err(ConnectionError::Timeout),
        }
    }

    /// Create a PeerConnection from an already-established TcpStream.
    ///
    /// Used by the server side when accepting incoming connections.
    pub fn from_stream(stream: TcpStream) -> Self {
        stream.set_nodelay(true).ok();
        let remote_addr = stream
            .peer_addr()
            .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 0)));

        let (reader, writer) = tokio::io::split(stream);

        Self {
            reader,
            writer,
            stats: ConnectionStats::new(remote_addr),
        }
    }

    /// Send a message to the peer.
    pub async fn send(&mut self, msg: WireMessage) -> Result<(), ConnectionError> {
        let bytes_written = codec::write_message(&mut self.writer, &msg).await?;
        self.stats.messages_sent += 1;
        self.stats.bytes_sent += bytes_written as u64;
        Ok(())
    }

    /// Receive a message from the peer.
    pub async fn recv(&mut self) -> Result<WireMessage, ConnectionError> {
        let msg = codec::read_message(&mut self.reader).await?;
        let stats = FrameStats::for_message(&msg).unwrap_or(FrameStats {
            total_bytes: 0,
            payload_bytes: 0,
            variant: "unknown",
        });
        self.stats.messages_received += 1;
        self.stats.bytes_received += stats.total_bytes as u64;
        Ok(msg)
    }

    /// Receive a message with a timeout.
    pub async fn recv_timeout(&mut self, timeout: Duration) -> Result<WireMessage, ConnectionError> {
        match tokio::time::timeout(timeout, self.recv()).await {
            Ok(result) => result,
            Err(_) => Err(ConnectionError::Timeout),
        }
    }

    /// Send a message and wait for a response.
    ///
    /// This is a convenience method for request-response patterns.
    pub async fn request(&mut self, msg: WireMessage) -> Result<WireMessage, ConnectionError> {
        self.send(msg).await?;
        self.recv().await
    }

    /// Send a message and wait for a response with a timeout.
    pub async fn request_timeout(
        &mut self,
        msg: WireMessage,
        timeout: Duration,
    ) -> Result<WireMessage, ConnectionError> {
        self.send(msg).await?;
        self.recv_timeout(timeout).await
    }

    /// Get the connection statistics.
    pub fn stats(&self) -> &ConnectionStats {
        &self.stats
    }

    /// Get the remote address.
    pub fn remote_addr(&self) -> SocketAddr {
        self.stats.remote_addr
    }
}

// =============================================================================
// Connection Pool (simple)
// =============================================================================

/// A simple connection pool that maintains connections to known peers.
///
/// This is a basic implementation; a production system would add reconnection
/// logic, health checking, and connection limits.
pub struct ConnectionPool {
    connections: Vec<(String, PeerConnection)>,
}

impl ConnectionPool {
    /// Create a new empty connection pool.
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
        }
    }

    /// Add an already-established connection to the pool.
    pub fn add(&mut self, name: String, conn: PeerConnection) {
        self.connections.push((name, conn));
    }

    /// Connect to a peer and add it to the pool.
    pub async fn connect(&mut self, name: String, addr: &str) -> Result<(), ConnectionError> {
        let conn = PeerConnection::connect(addr).await?;
        self.connections.push((name, conn));
        Ok(())
    }

    /// Get a mutable reference to a connection by name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut PeerConnection> {
        self.connections
            .iter_mut()
            .find(|(n, _)| n == name)
            .map(|(_, conn)| conn)
    }

    /// Number of connections in the pool.
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    /// Whether the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Iterate over all connections.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut PeerConnection)> {
        self.connections.iter_mut().map(|(n, c)| (n.as_str(), c))
    }
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::PROTOCOL_VERSION;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn connect_and_exchange() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut conn = PeerConnection::from_stream(stream);
            let msg = conn.recv().await.unwrap();
            conn.send(WireMessage::Pong { seq: 1, timestamp: 200 }).await.unwrap();
            msg
        });

        let mut client = PeerConnection::connect(&addr.to_string()).await.unwrap();
        client.send(WireMessage::Ping { seq: 1, timestamp: 100 }).await.unwrap();
        let response = client.recv().await.unwrap();

        assert_eq!(response, WireMessage::Pong { seq: 1, timestamp: 200 });

        let received = server_task.await.unwrap();
        assert_eq!(received, WireMessage::Ping { seq: 1, timestamp: 100 });
    }

    #[tokio::test]
    async fn request_response_pattern() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut conn = PeerConnection::from_stream(stream);
            let _msg = conn.recv().await.unwrap();
            conn.send(WireMessage::AttestedRoot {
                root: [0xab; 32],
                height: 10,
                timestamp: 1700000000,
                signatures: vec![],
                threshold_qc: None,
            })
            .await
            .unwrap();
        });

        let mut client = PeerConnection::connect(&addr.to_string()).await.unwrap();
        let response = client.request(WireMessage::RequestAttestedRoot).await.unwrap();

        match response {
            WireMessage::AttestedRoot { root, height, .. } => {
                assert_eq!(root, [0xab; 32]);
                assert_eq!(height, 10);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn stats_tracked() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut conn = PeerConnection::from_stream(stream);
            let _ = conn.recv().await;
            conn.send(WireMessage::Pong { seq: 1, timestamp: 0 }).await.unwrap();
        });

        let mut client = PeerConnection::connect(&addr.to_string()).await.unwrap();
        assert_eq!(client.stats().messages_sent, 0);
        assert_eq!(client.stats().messages_received, 0);

        client.send(WireMessage::Ping { seq: 1, timestamp: 0 }).await.unwrap();
        assert_eq!(client.stats().messages_sent, 1);
        assert!(client.stats().bytes_sent > 0);

        let _ = client.recv().await.unwrap();
        assert_eq!(client.stats().messages_received, 1);
        assert!(client.stats().bytes_received > 0);
    }

    #[tokio::test]
    async fn connection_closed_detection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            drop(stream); // Close immediately
        });

        // Small delay to let the server accept and close
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = PeerConnection::connect(&addr.to_string()).await.unwrap();
        let result = client.recv().await;
        assert!(matches!(result, Err(ConnectionError::Closed)));
    }

    #[test]
    fn connection_pool_basics() {
        let pool = ConnectionPool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }
}
