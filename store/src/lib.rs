//! `pyana-store`: Persistent storage for the pyana token system.
//!
//! This crate provides durable storage for token chains, federation state
//! (revocation trees, attested roots), key management, and audit logs using
//! `redb` as the embedded key-value store backend.
//!
//! # Design
//!
//! All state that was previously in-memory (in `pyana-commit`, `pyana-federation`,
//! and `pyana-audit`) can be persisted and recovered across restarts. The store
//! is designed to be crash-safe: `redb` uses write-ahead logging to ensure
//! atomicity.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     PersistentStore                           │
//! │                                                              │
//! │  ┌─────────────┐ ┌──────────────┐ ┌────────────────────┐   │
//! │  │ Token Chains │ │  Federation  │ │    Key Management   │   │
//! │  │             │ │  State       │ │                     │   │
//! │  │ store/load  │ │ revocations  │ │  signing keys       │   │
//! │  │ list        │ │ attested     │ │  (encrypted)        │   │
//! │  │             │ │ roots        │ │  public keys        │   │
//! │  └─────────────┘ └──────────────┘ └────────────────────┘   │
//! │                                                              │
//! │  ┌─────────────────────────────────────────────────────┐    │
//! │  │                   Audit Log                          │    │
//! │  │  append / retrieve / query by token                  │    │
//! │  └─────────────────────────────────────────────────────┘    │
//! │                                                              │
//! │  ┌─────────────────────────────────────────────────────┐    │
//! │  │                   Recovery                           │    │
//! │  │  recover_federation_state() → RecoveredState         │    │
//! │  └─────────────────────────────────────────────────────┘    │
//! │                                                              │
//! │                    redb (embedded KV)                         │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Encryption
//!
//! Signing keys are encrypted at rest using XChaCha20-Poly1305 (via BLAKE3
//! key derivation from the master key). Public keys are stored in plaintext.

pub mod audit;
pub mod federation;
pub mod keys;
pub mod recovery;
pub mod tables;
pub mod tokens;

#[cfg(test)]
mod tests;

use std::path::Path;

use redb::Database;

pub use audit::StoredAuditEvent;
pub use federation::StoredAttestedRoot;
pub use recovery::RecoveredState;
pub use tokens::{StoredFoldStep, TokenChain};

/// Errors that can occur during store operations.
#[derive(Debug)]
pub enum StoreError {
    /// The underlying database returned an error.
    Database(String),
    /// Serialization/deserialization failure.
    Serialization(String),
    /// Encryption or decryption failure.
    Crypto(String),
    /// The requested item was not found.
    NotFound,
    /// Data integrity check failed.
    Integrity(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(msg) => write!(f, "database error: {msg}"),
            Self::Serialization(msg) => write!(f, "serialization error: {msg}"),
            Self::Crypto(msg) => write!(f, "crypto error: {msg}"),
            Self::NotFound => write!(f, "not found"),
            Self::Integrity(msg) => write!(f, "integrity error: {msg}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<redb::DatabaseError> for StoreError {
    fn from(e: redb::DatabaseError) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<redb::TableError> for StoreError {
    fn from(e: redb::TableError) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<redb::TransactionError> for StoreError {
    fn from(e: redb::TransactionError) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<redb::CommitError> for StoreError {
    fn from(e: redb::CommitError) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<redb::StorageError> for StoreError {
    fn from(e: redb::StorageError) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<postcard::Error> for StoreError {
    fn from(e: postcard::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

/// Result type alias for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;

/// The persistent store for all pyana state.
///
/// Backed by `redb`, an embedded ACID key-value store. All operations are
/// crash-safe through redb's write-ahead logging.
pub struct PersistentStore {
    db: Database,
}

impl PersistentStore {
    /// Open a persistent store backed by a file on disk.
    ///
    /// Creates the file and all necessary tables if they don't exist.
    pub fn open(path: &Path) -> Result<Self> {
        let db = Database::create(path).map_err(|e| StoreError::Database(e.to_string()))?;
        let store = Self { db };
        store.initialize_tables()?;
        Ok(store)
    }

    /// Open an in-memory store (useful for testing).
    ///
    /// Data is lost when the store is dropped.
    pub fn open_in_memory() -> Result<Self> {
        let backend = redb::backends::InMemoryBackend::new();
        let db =
            Database::builder().create_with_backend(backend).map_err(|e| StoreError::Database(e.to_string()))?;
        let store = Self { db };
        store.initialize_tables()?;
        Ok(store)
    }

    /// Initialize all tables in the database.
    fn initialize_tables(&self) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            // Token chain tables.
            let _ = write_txn.open_table(tables::TOKEN_CHAINS)?;
            // Federation tables.
            let _ = write_txn.open_table(tables::REVOCATIONS)?;
            let _ = write_txn.open_table(tables::ATTESTED_ROOTS)?;
            // Key management tables.
            let _ = write_txn.open_table(tables::SIGNING_KEYS)?;
            let _ = write_txn.open_table(tables::PUBLIC_KEYS)?;
            // Audit log tables.
            let _ = write_txn.open_table(tables::AUDIT_LOG)?;
            let _ = write_txn.open_table(tables::AUDIT_TOKEN_INDEX)?;
            // Metadata table.
            let _ = write_txn.open_table(tables::METADATA)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Compact the database file, reclaiming unused space.
    pub fn compact(&mut self) -> Result<bool> {
        self.db.compact().map_err(|e| StoreError::Database(e.to_string()))
    }
}
