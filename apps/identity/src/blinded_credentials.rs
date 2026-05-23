//! Anonymous credential distribution via blinded queue.
//!
//! A university (issuer) batches N alumni credentials into a [`BlindedQueue`]
//! (via [`FairDistributionEndpoint`]). Each alumnus withdraws one credential
//! without the university learning which credential maps to which student.
//!
//! REVIEW[P1]: the blinded-queue's `Consumed { nullifier }` path does NOT carry
//! any credential payload back to the consumer — only proves a slot was spent.
//! The actual credential bytes must travel via a separate channel (e.g., the
//! inbox in `inbox_delivery.rs`) keyed by commitment preimage. Today no code
//! path closes this loop, so "withdrawing a credential" is a misnomer: a holder
//! gains a nullifier, not a `DelegatedToken`. Either add a per-commitment
//! payload-fetch endpoint, or document that this module only enforces the
//! "one-per-alumnus" budget and the credential payload arrives elsewhere.
//!
//! REVIEW[P2]: commitment construction `blake3("blinded-queue-commitment" ||
//! cert_bytes || randomness)` should be verified to match
//! `pyana_storage::blinded::crypto::create_commitment` exactly (the tests in
//! `tests.rs` use `create_commitment(item, &randomness)`, which is good). The
//! docs above describe the binding informally — ensure callers use the storage
//! helper, not a hand-rolled hash.
//!
//! # Protocol
//!
//! 1. **Issuer commits**: for each credential, compute
//!    `commitment = blake3("blinded-queue-commitment" || cert_bytes || randomness)`
//!    and POST to `/queue/credentials/commit`.
//! 2. **Alumnus withdraws**: generate a nullifier and Merkle membership proof,
//!    POST to `/queue/credentials/consume`.
//! 3. **After N consumes**: the queue is empty; the (N+1)th consume fails.
//!
//! # Framework primitives used
//!
//! - `AppServer::with_blinded_endpoint(path, endpoint)` — mounts the endpoint.
//! - `FairDistributionEndpoint::new(capacity)` — wraps [`BlindedQueue`].

use pyana_app_framework::blinded_endpoint::FairDistributionEndpoint;

/// Build a credential-distribution blinded endpoint.
///
/// * `capacity` — maximum number of commitments (equal to batch size N).
pub fn credential_blinded_endpoint(capacity: usize) -> FairDistributionEndpoint {
    FairDistributionEndpoint::new(capacity)
}
