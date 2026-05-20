//! Key management with encryption at rest.
//!
//! Signing keys are stored encrypted: the master key is used to derive an
//! encryption key via BLAKE3's keyed hash, and then XOR-based authenticated
//! encryption is applied (BLAKE3 as a stream cipher + MAC).
//!
//! This is a simplified authenticated encryption scheme suitable for 32-byte
//! keys. For production, a proper AEAD (XChaCha20-Poly1305) would be used,
//! but we keep dependencies minimal here.
//!
//! Public keys are stored in plaintext since they are not secret.

use redb::ReadableTable;

use crate::tables;
use crate::{PersistentStore, Result, StoreError};

/// Size of the nonce used for key encryption.
const NONCE_SIZE: usize = 16;

/// Size of the authentication tag.
const TAG_SIZE: usize = 32;

/// Total size of an encrypted key blob: nonce + ciphertext(32) + tag.
const ENCRYPTED_BLOB_SIZE: usize = NONCE_SIZE + 32 + TAG_SIZE;

impl PersistentStore {
    /// Store a signing key encrypted with the given master key.
    ///
    /// The key is encrypted using a BLAKE3-derived keystream and authenticated
    /// with a BLAKE3-keyed MAC. The nonce is randomly generated and stored
    /// alongside the ciphertext.
    pub fn store_signing_key(
        &self,
        name: &str,
        key: &[u8; 32],
        master_key: &[u8; 32],
    ) -> Result<()> {
        let encrypted = encrypt_key(key, master_key)?;

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(tables::SIGNING_KEYS)?;
            table.insert(name, encrypted.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Load and decrypt a signing key using the given master key.
    ///
    /// Returns `None` if no key exists with the given name.
    /// Returns `Err(Crypto)` if decryption/authentication fails (wrong master key).
    pub fn load_signing_key(
        &self,
        name: &str,
        master_key: &[u8; 32],
    ) -> Result<Option<[u8; 32]>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(tables::SIGNING_KEYS)?;

        match table.get(name)? {
            Some(value) => {
                let blob = value.value();
                let key = decrypt_key(blob, master_key)?;
                Ok(Some(key))
            }
            None => Ok(None),
        }
    }

    /// Delete a signing key by name.
    ///
    /// Returns true if a key was removed.
    pub fn delete_signing_key(&self, name: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let removed = {
            let mut table = write_txn.open_table(tables::SIGNING_KEYS)?;
            table.remove(name)?.is_some()
        };
        write_txn.commit()?;
        Ok(removed)
    }

    /// List all signing key names.
    pub fn list_signing_keys(&self) -> Result<Vec<String>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(tables::SIGNING_KEYS)?;

        let mut names = Vec::new();
        let iter = table.iter()?;
        for entry in iter {
            let entry =
                entry.map_err(|e: redb::StorageError| StoreError::Database(e.to_string()))?;
            names.push(entry.0.value().to_string());
        }
        Ok(names)
    }

    /// Store a public key (plaintext, not encrypted).
    pub fn store_public_key(&self, name: &str, key: &[u8; 32]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(tables::PUBLIC_KEYS)?;
            table.insert(name, key)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Load a public key by name.
    ///
    /// Returns `None` if no key exists with the given name.
    pub fn load_public_key(&self, name: &str) -> Result<Option<[u8; 32]>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(tables::PUBLIC_KEYS)?;

        match table.get(name)? {
            Some(value) => Ok(Some(*value.value())),
            None => Ok(None),
        }
    }

    /// Delete a public key by name.
    pub fn delete_public_key(&self, name: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let removed = {
            let mut table = write_txn.open_table(tables::PUBLIC_KEYS)?;
            table.remove(name)?.is_some()
        };
        write_txn.commit()?;
        Ok(removed)
    }

    /// List all public key names.
    pub fn list_public_keys(&self) -> Result<Vec<String>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(tables::PUBLIC_KEYS)?;

        let mut names = Vec::new();
        let iter = table.iter()?;
        for entry in iter {
            let entry =
                entry.map_err(|e: redb::StorageError| StoreError::Database(e.to_string()))?;
            names.push(entry.0.value().to_string());
        }
        Ok(names)
    }

    /// Check if a signing key exists.
    pub fn has_signing_key(&self, name: &str) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(tables::SIGNING_KEYS)?;
        Ok(table.get(name)?.is_some())
    }

    /// Check if a public key exists.
    pub fn has_public_key(&self, name: &str) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(tables::PUBLIC_KEYS)?;
        Ok(table.get(name)?.is_some())
    }
}

// =============================================================================
// Key Encryption/Decryption
// =============================================================================

/// Encrypt a 32-byte key using BLAKE3-based authenticated encryption.
///
/// Format: nonce (16 bytes) || ciphertext (32 bytes) || tag (32 bytes)
///
/// The encryption key is derived as: BLAKE3-keyed(master_key, "pyana-store key-enc v1" || nonce)
/// The keystream is: first 32 bytes of the derived output.
/// The tag is: BLAKE3-keyed(master_key, "pyana-store key-mac v1" || nonce || ciphertext)
fn encrypt_key(key: &[u8; 32], master_key: &[u8; 32]) -> Result<Vec<u8>> {
    // Generate random nonce.
    let mut nonce = [0u8; NONCE_SIZE];
    getrandom::fill(&mut nonce).map_err(|e| StoreError::Crypto(e.to_string()))?;

    // Derive encryption keystream.
    let keystream = derive_keystream(master_key, &nonce);

    // XOR to encrypt.
    let mut ciphertext = [0u8; 32];
    for i in 0..32 {
        ciphertext[i] = key[i] ^ keystream[i];
    }

    // Compute authentication tag.
    let tag = compute_tag(master_key, &nonce, &ciphertext);

    // Assemble blob.
    let mut blob = Vec::with_capacity(ENCRYPTED_BLOB_SIZE);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    blob.extend_from_slice(&tag);
    Ok(blob)
}

/// Decrypt a 32-byte key from an encrypted blob.
fn decrypt_key(blob: &[u8], master_key: &[u8; 32]) -> Result<[u8; 32]> {
    if blob.len() != ENCRYPTED_BLOB_SIZE {
        return Err(StoreError::Crypto(format!(
            "invalid encrypted blob size: expected {ENCRYPTED_BLOB_SIZE}, got {}",
            blob.len()
        )));
    }

    let nonce = &blob[..NONCE_SIZE];
    let ciphertext = &blob[NONCE_SIZE..NONCE_SIZE + 32];
    let stored_tag = &blob[NONCE_SIZE + 32..];

    // Verify authentication tag first.
    let nonce_arr: [u8; NONCE_SIZE] = nonce.try_into().unwrap();
    let ct_arr: [u8; 32] = ciphertext.try_into().unwrap();
    let expected_tag = compute_tag(master_key, &nonce_arr, &ct_arr);

    if !constant_time_eq(&expected_tag, stored_tag) {
        return Err(StoreError::Crypto(
            "authentication failed: invalid master key or corrupted data".to_string(),
        ));
    }

    // Derive keystream and decrypt.
    let keystream = derive_keystream(master_key, &nonce_arr);
    let mut plaintext = [0u8; 32];
    for i in 0..32 {
        plaintext[i] = ciphertext[i] ^ keystream[i];
    }

    Ok(plaintext)
}

/// Derive a 32-byte keystream from master key and nonce.
fn derive_keystream(master_key: &[u8; 32], nonce: &[u8; NONCE_SIZE]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key("pyana-store key-enc v1");
    hasher.update(master_key);
    hasher.update(nonce);
    *hasher.finalize().as_bytes()
}

/// Compute an authentication tag over nonce + ciphertext.
fn compute_tag(master_key: &[u8; 32], nonce: &[u8; NONCE_SIZE], ciphertext: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key("pyana-store key-mac v1");
    hasher.update(master_key);
    hasher.update(nonce);
    hasher.update(ciphertext);
    *hasher.finalize().as_bytes()
}

/// Constant-time comparison of two byte slices.
fn constant_time_eq(a: &[u8; 32], b: &[u8]) -> bool {
    if b.len() != 32 {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}
