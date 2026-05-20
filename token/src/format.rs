//! Token format detection and wire encoding.

use crate::error::TokenError;

/// Supported token formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TokenFormat {
  /// HMAC-SHA256 chained macaroon (symmetric, ~0.5μs verify).
  Macaroon,
  /// Ed25519/P-256 Biscuit with Datalog authorization (asymmetric, ~50-80μs verify).
  Biscuit,
}

impl TokenFormat {
  /// Wire prefix for encoded tokens.
  pub fn prefix(&self) -> &'static str {
    match self {
      TokenFormat::Macaroon => "em2_",
      TokenFormat::Biscuit => "eb2_",
    }
  }

  /// Detect format from an encoded token string.
  pub fn detect(encoded: &str) -> Result<TokenFormat, TokenError> {
    if encoded.starts_with("em2_") {
      Ok(TokenFormat::Macaroon)
    } else if encoded.starts_with("eb2_") || encoded.starts_with("biscuit:") {
      Ok(TokenFormat::Biscuit)
    } else {
      Err(TokenError::Malformed(
        "unrecognized token prefix (expected em2_ or eb2_)".into(),
      ))
    }
  }

  /// Detect format from raw bytes (for binary-encoded tokens).
  ///
  /// Macaroon tokens start with MsgPack array marker.
  /// Biscuit tokens start with Protobuf varint.
  pub fn detect_bytes(data: &[u8]) -> Result<TokenFormat, TokenError> {
    if data.is_empty() {
      return Err(TokenError::Malformed("empty token".into()));
    }
    // MsgPack fixarray or array16/array32 markers
    if (data[0] & 0xf0) == 0x90 || data[0] == 0xdc || data[0] == 0xdd {
      Ok(TokenFormat::Macaroon)
    } else {
      // Default to biscuit for protobuf-encoded data
      Ok(TokenFormat::Biscuit)
    }
  }
}

impl std::fmt::Display for TokenFormat {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      TokenFormat::Macaroon => write!(f, "macaroon"),
      TokenFormat::Biscuit => write!(f, "biscuit"),
    }
  }
}

impl std::str::FromStr for TokenFormat {
  type Err = TokenError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "macaroon" | "mac" => Ok(TokenFormat::Macaroon),
      "biscuit" | "bisc" => Ok(TokenFormat::Biscuit),
      _ => Err(TokenError::UnsupportedFormat(s.to_string())),
    }
  }
}

/// Authorization header scheme.
pub const AUTH_HEADER_SCHEME: &str = "PyanaV1";

/// Format tokens for an Authorization header.
///
/// `PyanaV1 <token>[,<discharge>...]`
pub fn format_auth_header(tokens: &[&str]) -> String {
  format!("{} {}", AUTH_HEADER_SCHEME, tokens.join(","))
}

/// Parse tokens from an Authorization header.
pub fn parse_auth_header(header: &str) -> Result<Vec<String>, TokenError> {
  let stripped = header
    .strip_prefix(AUTH_HEADER_SCHEME)
    .ok_or_else(|| TokenError::Malformed("missing PyanaV1 scheme".into()))?
    .trim_start();
  Ok(
    stripped
      .split(',')
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
      .collect(),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_detect_macaroon() {
    assert_eq!(
      TokenFormat::detect("em2_abc123").unwrap(),
      TokenFormat::Macaroon
    );
  }

  #[test]
  fn test_detect_biscuit() {
    assert_eq!(
      TokenFormat::detect("eb2_abc123").unwrap(),
      TokenFormat::Biscuit
    );
    assert_eq!(
      TokenFormat::detect("biscuit:abc123").unwrap(),
      TokenFormat::Biscuit
    );
  }

  #[test]
  fn test_detect_unknown() {
    assert!(TokenFormat::detect("xyz_abc").is_err());
  }

  #[test]
  fn test_parse_str() {
    assert_eq!(
      "macaroon".parse::<TokenFormat>().unwrap(),
      TokenFormat::Macaroon
    );
    assert_eq!(
      "biscuit".parse::<TokenFormat>().unwrap(),
      TokenFormat::Biscuit
    );
  }

  #[test]
  fn test_auth_header_roundtrip() {
    let header = format_auth_header(&["em2_token1", "em2_discharge1"]);
    let parsed = parse_auth_header(&header).unwrap();
    assert_eq!(parsed, vec!["em2_token1", "em2_discharge1"]);
  }
}
