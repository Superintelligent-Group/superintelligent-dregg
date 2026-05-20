//! Action bitmask for resource-level permissions.
//!
//! Follows the fly.io pattern: a compact bitmask where each bit represents
//! a permission level. Actions compose via bitwise AND (intersection) when
//! multiple caveats constrain the same resource.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A bitmask of allowed actions on a resource.
///
/// Actions are intersected (AND) across caveats — each caveat can only
/// narrow the set of allowed actions, never expand it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Action(u8);

impl Action {
  /// No actions permitted.
  pub const NONE: Action = Action(0);

  /// Read access.
  pub const READ: Action = Action(1 << 0);

  /// Write/update access.
  pub const WRITE: Action = Action(1 << 1);

  /// Create new resources.
  pub const CREATE: Action = Action(1 << 2);

  /// Delete resources.
  pub const DELETE: Action = Action(1 << 3);

  /// Control operations (start/stop services, admin actions).
  pub const CONTROL: Action = Action(1 << 4);

  /// All actions.
  pub const ALL: Action = Action(0x1f);

  /// Create an action from a raw bitmask value.
  #[inline]
  pub const fn from_bits(bits: u8) -> Self {
    Action(bits & 0x1f)
  }

  /// Get the raw bitmask value.
  #[inline]
  pub const fn bits(self) -> u8 {
    self.0
  }

  /// Check if this action set contains the given action.
  #[inline]
  pub const fn contains(self, other: Action) -> bool {
    (self.0 & other.0) == other.0
  }

  /// Intersect two action sets (AND). Used when multiple caveats
  /// constrain the same resource.
  #[inline]
  pub const fn intersect(self, other: Action) -> Action {
    Action(self.0 & other.0)
  }

  /// Union two action sets (OR).
  #[inline]
  pub const fn union(self, other: Action) -> Action {
    Action(self.0 | other.0)
  }

  /// Check if no actions are permitted.
  #[inline]
  pub const fn is_empty(self) -> bool {
    self.0 == 0
  }

  /// Parse from a string like "rwcd" or "*" (all).
  pub fn parse(s: &str) -> Self {
    if s == "*" {
      return Self::ALL;
    }
    let mut bits = 0u8;
    for c in s.chars() {
      match c {
        'r' => bits |= Self::READ.0,
        'w' => bits |= Self::WRITE.0,
        'c' => bits |= Self::CREATE.0,
        'd' => bits |= Self::DELETE.0,
        'C' => bits |= Self::CONTROL.0,
        _ => {}
      }
    }
    Action(bits)
  }
}

impl fmt::Debug for Action {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Action(")?;
    fmt::Display::fmt(self, f)?;
    write!(f, ")")
  }
}

impl fmt::Display for Action {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if *self == Self::ALL {
      return write!(f, "*");
    }
    if self.contains(Self::READ) {
      write!(f, "r")?;
    }
    if self.contains(Self::WRITE) {
      write!(f, "w")?;
    }
    if self.contains(Self::CREATE) {
      write!(f, "c")?;
    }
    if self.contains(Self::DELETE) {
      write!(f, "d")?;
    }
    if self.contains(Self::CONTROL) {
      write!(f, "C")?;
    }
    Ok(())
  }
}

/// Trait for types that can be used as action bitmasks in a `ResourceSet`.
pub trait BitMask: Copy + Clone + PartialEq + Eq {
  /// The identity element for intersection (all bits set).
  fn all() -> Self;

  /// The zero element (no bits set).
  fn none() -> Self;

  /// Intersect with another mask.
  fn intersect(self, other: Self) -> Self;

  /// Check if no bits are set.
  fn is_empty(self) -> bool;

  /// Check if self contains all bits in other.
  fn contains(self, other: Self) -> bool;
}

impl BitMask for Action {
  #[inline]
  fn all() -> Self {
    Action::ALL
  }
  #[inline]
  fn none() -> Self {
    Action::NONE
  }
  #[inline]
  fn intersect(self, other: Self) -> Self {
    self.intersect(other)
  }
  #[inline]
  fn is_empty(self) -> bool {
    self.is_empty()
  }
  #[inline]
  fn contains(self, other: Self) -> bool {
    self.contains(other)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_action_contains() {
    let rw = Action::READ.union(Action::WRITE);
    assert!(rw.contains(Action::READ));
    assert!(rw.contains(Action::WRITE));
    assert!(!rw.contains(Action::DELETE));
    assert!(Action::ALL.contains(rw));
  }

  #[test]
  fn test_action_intersect() {
    let rw = Action::READ.union(Action::WRITE);
    let rd = Action::READ.union(Action::DELETE);
    let result = rw.intersect(rd);
    assert!(result.contains(Action::READ));
    assert!(!result.contains(Action::WRITE));
    assert!(!result.contains(Action::DELETE));
  }

  #[test]
  fn test_action_parse_display_roundtrip() {
    // Parse
    assert_eq!(Action::parse("r"), Action::READ);
    assert_eq!(Action::parse("rw"), Action::READ.union(Action::WRITE));
    assert_eq!(Action::parse("*"), Action::ALL);
    assert_eq!(Action::parse("rwcdC"), Action::ALL);

    // Display
    assert_eq!(format!("{}", Action::READ), "r");
    assert_eq!(format!("{}", Action::READ.union(Action::WRITE)), "rw");
    assert_eq!(format!("{}", Action::ALL), "*");

    // Round-trip: parse → display → parse
    for input in &["r", "rw", "*", "rwd"] {
      let parsed = Action::parse(input);
      let displayed = format!("{}", parsed);
      assert_eq!(Action::parse(&displayed), parsed);
    }
  }
}
