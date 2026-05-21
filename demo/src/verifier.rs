//! Cross-silo verification logic.
//!
//! The verifier is the component that partner.org uses to validate tokens
//! presented by acme.corp's users. It performs the following checks:
//!
//! 1. **Issuer membership**: Is the token's issuer a member of our federation?
//! 2. **Derivation trace**: Is the chain of derivation steps valid?
//! 3. **Authorization**: Does the token grant the requested access?
//! 4. **Revocation**: Has the token been revoked?
//!
//! Crucially, steps 1-3 require NO communication with the issuing silo.
//! Step 4 requires only the shared revocation accumulator state, which
//! is distributed asynchronously (not on the critical path).

use crate::authority::hex_encode;
use crate::federation::{Federation, FederationRole};
use crate::revocation::{NonMembershipProof, RevocationRegistry};
use crate::token::{AuthorizationRequest, TokenState};
use crate::trace;

// =============================================================================
// Verification Result
// =============================================================================

/// The result of a cross-silo verification.
#[derive(Clone, Debug)]
pub struct VerificationResult {
    /// Overall verdict.
    pub authorized: bool,
    /// Whether the issuer is a federation member.
    pub issuer_valid: bool,
    /// Whether the derivation trace is valid.
    pub trace_valid: bool,
    /// Number of derivation steps verified.
    pub trace_steps: usize,
    /// Whether the authorization check passed.
    pub authorization_valid: bool,
    /// Whether the revocation check passed.
    pub revocation_valid: bool,
    /// If denied, the reason.
    pub denial_reason: Option<String>,
}

impl VerificationResult {
    /// Create an authorized result.
    fn authorized(trace_steps: usize) -> Self {
        VerificationResult {
            authorized: true,
            issuer_valid: true,
            trace_valid: true,
            trace_steps,
            authorization_valid: true,
            revocation_valid: true,
            denial_reason: None,
        }
    }

    /// Create a denied result.
    fn denied(reason: &str) -> Self {
        VerificationResult {
            authorized: false,
            issuer_valid: false,
            trace_valid: false,
            trace_steps: 0,
            authorization_valid: false,
            revocation_valid: false,
            denial_reason: Some(reason.to_string()),
        }
    }
}

// =============================================================================
// Verifier
// =============================================================================

/// A cross-silo token verifier.
///
/// The verifier is configured with:
/// - A reference to the federation registry (for membership checks)
/// - A reference to the revocation registry (for revocation checks)
///
/// It does NOT need access to the issuing authority's private key.
pub struct Verifier<'a> {
    /// The federation this verifier trusts.
    federation: &'a Federation,
    /// The revocation registry to check against.
    revocation_registry: &'a RevocationRegistry,
}

impl<'a> Verifier<'a> {
    /// Create a new verifier.
    pub fn new(
        _silo_name: &str,
        federation: &'a Federation,
        revocation_registry: &'a RevocationRegistry,
    ) -> Self {
        Verifier {
            federation,
            revocation_registry,
        }
    }

    /// Perform full cross-silo verification of a token.
    ///
    /// This is the main entry point for verifying a token presented from
    /// another silo. It performs all four verification steps.
    pub fn verify(
        &self,
        token: &TokenState,
        request: &AuthorizationRequest,
        non_membership_proof: Option<&NonMembershipProof>,
    ) -> VerificationResult {
        // Step 1: Check issuer federation membership.
        let issuer_valid = self.check_issuer_membership(token);
        if !issuer_valid {
            let mut result = VerificationResult::denied(&format!(
                "issuer {} is not a member of federation '{}'",
                token.issuer.short_hex(),
                self.federation.name,
            ));
            result.issuer_valid = false;
            return result;
        }

        // Step 2: Verify the derivation trace.
        let trace_result = self.check_derivation_trace(token);
        if !trace_result.valid {
            let mut result = VerificationResult::denied(&format!(
                "derivation trace invalid: {}",
                trace_result.failure_reason.unwrap_or_default(),
            ));
            result.issuer_valid = true;
            result.trace_valid = false;
            result.trace_steps = trace_result.steps_verified;
            return result;
        }

        // Step 3: Check authorization (facts + checks vs request).
        let auth_result = self.check_authorization(token, request);
        if let Err(reason) = auth_result {
            let mut result = VerificationResult::denied(&reason);
            result.issuer_valid = true;
            result.trace_valid = true;
            result.trace_steps = trace_result.steps_verified;
            result.authorization_valid = false;
            return result;
        }

        // Step 4: Check revocation.
        let revocation_valid = self.check_revocation(token, non_membership_proof);
        if !revocation_valid {
            let mut result = VerificationResult::denied("token has been revoked");
            result.issuer_valid = true;
            result.trace_valid = true;
            result.trace_steps = trace_result.steps_verified;
            result.authorization_valid = true;
            result.revocation_valid = false;
            return result;
        }

        // All checks passed.
        VerificationResult::authorized(trace_result.steps_verified)
    }

    /// Step 1: Check that the token's issuer is a federation member with Issuer role.
    fn check_issuer_membership(&self, token: &TokenState) -> bool {
        self.federation.is_member(&token.issuer)
            && self
                .federation
                .has_role(&token.issuer, &FederationRole::Issuer)
    }

    /// Step 2: Verify the complete derivation trace.
    fn check_derivation_trace(&self, token: &TokenState) -> trace::TraceVerificationResult {
        trace::verify_trace(&token.derivation_trace, &token.state_root, self.federation)
    }

    /// Step 3: Check authorization of the request against the token.
    fn check_authorization(
        &self,
        token: &TokenState,
        request: &AuthorizationRequest,
    ) -> Result<(), String> {
        token.authorize(request)
    }

    /// Step 4: Check revocation status.
    ///
    /// If a non-membership proof is provided, verify it against the current
    /// accumulator state. If no proof is provided, check the registry directly.
    fn check_revocation(
        &self,
        token: &TokenState,
        non_membership_proof: Option<&NonMembershipProof>,
    ) -> bool {
        match non_membership_proof {
            Some(proof) => {
                // Verify the non-membership proof against the current accumulator.
                self.revocation_registry
                    .verify_non_membership(proof, &token.issuer)
            }
            None => {
                // Direct check: is the token in the revocation registry?
                !self.revocation_registry.is_revoked(&token.id)
            }
        }
    }

    /// Verify only the issuer membership (for demo display).
    pub fn verify_issuer_membership(&self, token: &TokenState) -> bool {
        self.check_issuer_membership(token)
    }

    /// Verify only the trace (for demo display).
    pub fn verify_trace_only(&self, token: &TokenState) -> trace::TraceVerificationResult {
        self.check_derivation_trace(token)
    }
}

// =============================================================================
// Presentation
// =============================================================================

/// A token presentation for cross-silo verification.
///
/// This bundles the token with any additional proofs needed for
/// verification at the receiving silo.
#[derive(Clone)]
pub struct TokenPresentation {
    /// The token being presented.
    pub token: TokenState,
    /// Non-membership proof from the revocation accumulator.
    pub non_membership_proof: Option<NonMembershipProof>,
    /// The state commitment (for the receiving silo to verify).
    pub state_commitment: [u8; 32],
}

impl TokenPresentation {
    /// Create a new presentation from a token.
    pub fn new(token: TokenState, non_membership_proof: Option<NonMembershipProof>) -> Self {
        let state_commitment = token.state_root;
        TokenPresentation {
            token,
            non_membership_proof,
            state_commitment,
        }
    }

    /// Get the state commitment as short hex.
    pub fn commitment_hex(&self) -> String {
        hex_encode(&self.state_commitment[..4])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::Authority;
    use crate::federation::FederationRole;
    use crate::token::{Check, Fact, Rule};

    fn setup() -> (Authority, Authority, Federation, RevocationRegistry) {
        let auth1 = Authority::new("acme.corp");
        let auth2 = Authority::new("partner.org");
        let mut fed = Federation::new("test-federation");
        fed.add_member(
            "acme.corp",
            &auth1,
            vec![
                FederationRole::Issuer,
                FederationRole::Verifier,
                FederationRole::Revoker,
            ],
        );
        fed.add_member("partner.org", &auth2, vec![FederationRole::Verifier]);
        fed.compute_root();

        let mut registry = RevocationRegistry::new();
        registry.register(&auth1.public_key);
        registry.register(&auth2.public_key);

        (auth1, auth2, fed, registry)
    }

    #[test]
    fn test_full_verification_success() {
        let (auth1, _auth2, fed, registry) = setup();

        let facts = vec![Fact::app("frontend", "rw")];
        let rules = vec![Rule::allow_app(), Rule::deny_default()];
        let token = auth1.mint_token(facts, rules);

        let verifier = Verifier::new("partner.org", &fed, &registry);
        let request = AuthorizationRequest {
            app: "frontend".to_string(),
            service: None,
            action: "r".to_string(),
        };

        let result = verifier.verify(&token, &request, None);
        assert!(result.authorized);
    }

    #[test]
    fn test_verification_with_attenuation() {
        let (auth1, _auth2, fed, registry) = setup();

        let facts = vec![Fact::app("frontend", "rwcd"), Fact::app("backend", "rw")];
        let rules = vec![Rule::allow_app(), Rule::deny_default()];
        let token = auth1.mint_token(facts, rules);

        let remove = vec![Fact::app("backend", "rw")];
        let checks = vec![Check::read_only()];
        let attenuated = token.attenuate(&remove, checks, &auth1);

        let verifier = Verifier::new("partner.org", &fed, &registry);

        // Read should work.
        let request_read = AuthorizationRequest {
            app: "frontend".to_string(),
            service: None,
            action: "r".to_string(),
        };
        let result = verifier.verify(&attenuated, &request_read, None);
        assert!(result.authorized);

        // Write should fail.
        let request_write = AuthorizationRequest {
            app: "frontend".to_string(),
            service: None,
            action: "w".to_string(),
        };
        let result = verifier.verify(&attenuated, &request_write, None);
        assert!(!result.authorized);
    }

    #[test]
    fn test_verification_revoked_token() {
        let (auth1, _auth2, fed, registry) = setup();

        let facts = vec![Fact::app("frontend", "rw")];
        let rules = vec![Rule::allow_app(), Rule::deny_default()];
        let token = auth1.mint_token(facts, rules);

        // Revoke the token.
        registry.get(&auth1.public_key).unwrap().revoke(&token.id);

        let verifier = Verifier::new("partner.org", &fed, &registry);
        let request = AuthorizationRequest {
            app: "frontend".to_string(),
            service: None,
            action: "r".to_string(),
        };

        let result = verifier.verify(&token, &request, None);
        assert!(!result.authorized);
        assert!(!result.revocation_valid);
    }

    #[test]
    fn test_verification_non_member_issuer() {
        let (_auth1, _auth2, fed, registry) = setup();
        let outsider = Authority::new("evil.org");

        let facts = vec![Fact::app("frontend", "rw")];
        let rules = vec![Rule::allow_app()];
        let token = outsider.mint_token(facts, rules);

        let verifier = Verifier::new("partner.org", &fed, &registry);
        let request = AuthorizationRequest {
            app: "frontend".to_string(),
            service: None,
            action: "r".to_string(),
        };

        let result = verifier.verify(&token, &request, None);
        assert!(!result.authorized);
        assert!(!result.issuer_valid);
    }
}
