//! Third-party caveats and discharge protocol.
//!
//! Third-party caveats delegate authorization to an external service.
//! The flow:
//!
//! 1. **Adding a 3P caveat**:
//!    - Generate ephemeral discharge key `r` (32 bytes)
//!    - Encrypt `{r, caveats_for_3p}` under shared key `KA` → Ticket (CID)
//!    - Encrypt `r` under current HMAC tail → VerifierKey (VID)
//!    - Append 3P caveat with (location, VID, Ticket) to macaroon
//!    - HMAC-chain as normal
//!
//! 2. **Discharging**:
//!    - Client extracts Ticket from 3P caveat, sends to 3P location
//!    - 3P decrypts Ticket using `KA`, recovers `r` and caveats
//!    - 3P creates discharge macaroon signed with `r`
//!    - 3P may add its own caveats (e.g., ConfineUser, ValidityWindow)
//!
//! 3. **Verification**:
//!    - Verifier replays HMAC chain to derive tail at 3P caveat position
//!    - Decrypt VID using that tail → recover `r`
//!    - Verify discharge macaroon using `r` as root key
//!    - Discharge must be bound to parent (BindToParentToken caveat)
//!    - All caveats from discharge are added to the clearing set

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::caveat::{CAV_THIRD_PARTY, CaveatSet};
use crate::crypto;
use crate::error::MacaroonError;

/// The body of a third-party caveat, as stored in the macaroon.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThirdPartyCaveat {
    /// URL/identifier of the third-party service.
    pub location: String,

    /// Encrypted discharge key: `seal(current_tail, r)`.
    /// Only the verifier (who can replay the HMAC chain) can decrypt this.
    pub verifier_key: Vec<u8>,

    /// Encrypted ticket: `seal(shared_key_KA, {discharge_key: r, caveats})`.
    /// Sent to the third party to request a discharge.
    pub ticket: Vec<u8>,
}

/// The plaintext contents of a ticket, decrypted by the third party.
#[derive(Clone, Debug, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct WireTicket {
    /// The discharge key — the third party uses this to sign the discharge macaroon.
    pub discharge_key: Vec<u8>,

    /// Caveats the third party must check/enforce.
    #[zeroize(skip)]
    pub caveats: CaveatSet,
}

impl ThirdPartyCaveat {
    /// Create a new third-party caveat.
    ///
    /// # Arguments
    /// - `location`: URL of the 3P discharge service
    /// - `current_tail`: The macaroon's current HMAC tail (used to encrypt VID)
    /// - `shared_key`: Shared key between macaroon creator and the 3P (KA)
    /// - `caveats_for_3p`: Caveats the 3P must check when discharging
    ///
    /// # Returns
    /// The `ThirdPartyCaveat` body and the discharge key `r` (which the caller
    /// does NOT store — it's embedded in the encrypted ticket).
    pub fn new(
        location: String,
        current_tail: &[u8; 32],
        shared_key: &[u8; 32],
        caveats_for_3p: CaveatSet,
    ) -> Result<(Self, [u8; 32]), MacaroonError> {
        // Generate ephemeral discharge key
        let discharge_key = crypto::random_key();

        // Create the ticket: {discharge_key, caveats}
        let wire_ticket = WireTicket {
            discharge_key: discharge_key.to_vec(),
            caveats: caveats_for_3p,
        };
        let ticket_plaintext =
            rmp_serde::to_vec(&wire_ticket).map_err(|e| MacaroonError::Encoding(e.to_string()))?;

        // Encrypt ticket under shared key KA
        let ticket = crypto::seal(shared_key, &ticket_plaintext)?;

        // Encrypt discharge key under current tail (VID)
        let verifier_key = crypto::seal(current_tail, &discharge_key)?;

        Ok((
            Self {
                location,
                verifier_key,
                ticket,
            },
            discharge_key,
        ))
    }

    /// Encode this 3P caveat body for storage in a WireCaveat.
    pub fn encode_body(&self) -> Result<Vec<u8>, MacaroonError> {
        rmp_serde::to_vec(self).map_err(|e| MacaroonError::Encoding(e.to_string()))
    }

    /// Decode a 3P caveat body from bytes.
    pub fn decode_body(body: &[u8]) -> Result<Self, MacaroonError> {
        rmp_serde::from_slice(body).map_err(|e| MacaroonError::Encoding(e.to_string()))
    }

    /// Decrypt the ticket using the shared key (called by the third party).
    ///
    /// Returns the discharge key and the caveats to check.
    pub fn decrypt_ticket(
        ticket: &[u8],
        shared_key: &[u8; 32],
    ) -> Result<WireTicket, MacaroonError> {
        let plaintext = crypto::unseal(shared_key, ticket)?;
        rmp_serde::from_slice(&plaintext).map_err(|e| MacaroonError::Encoding(e.to_string()))
    }

    /// Decrypt the verifier key using the current HMAC tail (called by the verifier).
    ///
    /// Returns the discharge key `r`.
    pub fn decrypt_verifier_key(
        verifier_key: &[u8],
        current_tail: &[u8; 32],
    ) -> Result<[u8; 32], MacaroonError> {
        let plaintext = crypto::unseal(current_tail, verifier_key)?;
        if plaintext.len() != 32 {
            return Err(MacaroonError::DecryptionFailed(
                "discharge key must be 32 bytes".into(),
            ));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&plaintext);
        Ok(key)
    }

    /// Check if a wire caveat is a third-party caveat.
    pub fn is_third_party(caveat_type: u16) -> bool {
        caveat_type == CAV_THIRD_PARTY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_3p_caveat_roundtrip() {
        let current_tail = crypto::random_key();
        let shared_key = crypto::random_key();
        let caveats = CaveatSet::new();

        let (tp, _discharge_key) = ThirdPartyCaveat::new(
            "https://auth.example.com".into(),
            &current_tail,
            &shared_key,
            caveats.clone(),
        )
        .unwrap();

        // Third party can decrypt the ticket
        let wire_ticket = ThirdPartyCaveat::decrypt_ticket(&tp.ticket, &shared_key).unwrap();
        assert_eq!(wire_ticket.discharge_key.len(), 32);
        assert!(wire_ticket.caveats.is_empty());

        // Verifier can decrypt the verifier key
        let recovered_key =
            ThirdPartyCaveat::decrypt_verifier_key(&tp.verifier_key, &current_tail).unwrap();
        assert_eq!(
            recovered_key.as_slice(),
            wire_ticket.discharge_key.as_slice()
        );
    }

    #[test]
    fn test_3p_wrong_shared_key_fails() {
        let current_tail = crypto::random_key();
        let shared_key = crypto::random_key();
        let wrong_key = crypto::random_key();

        let (tp, _) = ThirdPartyCaveat::new(
            "https://auth.example.com".into(),
            &current_tail,
            &shared_key,
            CaveatSet::new(),
        )
        .unwrap();

        assert!(ThirdPartyCaveat::decrypt_ticket(&tp.ticket, &wrong_key).is_err());
    }

    #[test]
    fn test_3p_wrong_tail_fails() {
        let current_tail = crypto::random_key();
        let wrong_tail = crypto::random_key();
        let shared_key = crypto::random_key();

        let (tp, _) = ThirdPartyCaveat::new(
            "https://auth.example.com".into(),
            &current_tail,
            &shared_key,
            CaveatSet::new(),
        )
        .unwrap();

        assert!(ThirdPartyCaveat::decrypt_verifier_key(&tp.verifier_key, &wrong_tail).is_err());
    }

    #[test]
    fn test_3p_encode_decode_roundtrip() {
        let current_tail = crypto::random_key();
        let shared_key = crypto::random_key();

        let (tp, _) = ThirdPartyCaveat::new(
            "https://auth.example.com".into(),
            &current_tail,
            &shared_key,
            CaveatSet::new(),
        )
        .unwrap();

        let encoded = tp.encode_body().unwrap();
        let decoded = ThirdPartyCaveat::decode_body(&encoded).unwrap();

        assert_eq!(tp.location, decoded.location);
        assert_eq!(tp.verifier_key, decoded.verifier_key);
        assert_eq!(tp.ticket, decoded.ticket);
    }
}
