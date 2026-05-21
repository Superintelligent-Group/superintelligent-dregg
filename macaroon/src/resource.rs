//! Generic resource set for mapping resource IDs to action bitmasks.
//!
//! This follows the fly.io `resset` pattern: a `ResourceSet<I, M>` maps
//! typed resource identifiers to action bitmasks, supporting intersection
//! semantics for caveat stacking.

use std::collections::HashMap;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::action::BitMask;
use crate::error::CaveatError;

/// Maps resource identifiers to allowed action bitmasks.
///
/// When multiple caveats constrain the same resource, their action masks
/// are intersected — each caveat can only narrow permissions.
///
/// A resource ID of the type's default value (e.g., 0 for integers, "" for strings)
/// is treated as a wildcard matching any resource.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "I: Serialize, M: Serialize",
    deserialize = "I: serde::de::DeserializeOwned + Hash + Eq, M: serde::de::DeserializeOwned"
))]
pub struct ResourceSet<I: Hash + Eq, M: BitMask> {
    resources: HashMap<I, M>,
}

impl<I: Hash + Eq + Clone + Default + PartialEq, M: BitMask> ResourceSet<I, M> {
    /// Create an empty resource set.
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
        }
    }

    /// Create a resource set with a single entry.
    pub fn with(id: I, mask: M) -> Self {
        let mut set = Self::new();
        set.resources.insert(id, mask);
        set
    }

    /// Add a resource with its action mask.
    pub fn insert(&mut self, id: I, mask: M) {
        self.resources.insert(id, mask);
    }

    /// Get the action mask for a resource, if present.
    pub fn get(&self, id: &I) -> Option<&M> {
        self.resources.get(id)
    }

    /// Check if this resource set is empty.
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.resources.len()
    }

    /// Iterate over entries.
    pub fn iter(&self) -> impl Iterator<Item = (&I, &M)> {
        self.resources.iter()
    }

    /// Resolve the effective action mask for a given resource and requested action.
    ///
    /// Resolution logic:
    /// 1. Check for an exact match on the resource ID
    /// 2. Check for a wildcard entry (default ID)
    /// 3. Intersect all matching masks
    ///
    /// Returns the effective mask, or `None` if no entry matches.
    pub fn resolve(&self, id: &I) -> Option<M> {
        let exact = self.resources.get(id).copied();
        let wildcard = if *id != I::default() {
            self.resources.get(&I::default()).copied()
        } else {
            None
        };

        match (exact, wildcard) {
            (Some(e), Some(w)) => Some(e.intersect(w)),
            (Some(e), None) => Some(e),
            (None, Some(w)) => Some(w),
            (None, None) => None,
        }
    }

    /// Check if the requested action on the given resource is prohibited.
    ///
    /// Returns `Ok(())` if allowed, or `Err(CaveatError)` if denied.
    pub fn prohibits(&self, id: &I, action: M, resource_name: &str) -> Result<(), CaveatError>
    where
        I: std::fmt::Debug,
    {
        match self.resolve(id) {
            Some(mask) if mask.contains(action) => Ok(()),
            Some(_) => Err(CaveatError::Prohibited(format!(
                "{resource_name} access denied: insufficient permissions"
            ))),
            None => Err(CaveatError::Prohibited(format!(
                "{resource_name} access denied: resource not in set"
            ))),
        }
    }
}

impl<I: Hash + Eq + Clone + Default + PartialEq, M: BitMask> Default for ResourceSet<I, M> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;

    #[test]
    fn test_exact_match() {
        let mut set = ResourceSet::<u64, Action>::new();
        set.insert(42, Action::READ.union(Action::WRITE));

        assert_eq!(set.resolve(&42), Some(Action::READ.union(Action::WRITE)));
        assert_eq!(set.resolve(&99), None);
    }

    #[test]
    fn test_wildcard_match() {
        let mut set = ResourceSet::<u64, Action>::new();
        set.insert(0, Action::READ); // 0 is Default for u64 → wildcard

        assert_eq!(set.resolve(&42), Some(Action::READ));
        assert_eq!(set.resolve(&99), Some(Action::READ));
    }

    #[test]
    fn test_exact_and_wildcard_intersect() {
        let mut set = ResourceSet::<u64, Action>::new();
        set.insert(0, Action::ALL); // wildcard: all
        set.insert(42, Action::READ); // exact: read only

        // For resource 42: intersect(READ, ALL) = READ
        assert_eq!(set.resolve(&42), Some(Action::READ));
        // For resource 99: just wildcard = ALL
        assert_eq!(set.resolve(&99), Some(Action::ALL));
    }

    #[test]
    fn test_string_resource() {
        let mut set = ResourceSet::<String, Action>::new();
        set.insert("my-app".to_string(), Action::READ.union(Action::WRITE));

        assert!(
            set.prohibits(&"my-app".to_string(), Action::READ, "app")
                .is_ok()
        );
        assert!(
            set.prohibits(&"my-app".to_string(), Action::DELETE, "app")
                .is_err()
        );
        assert!(
            set.prohibits(&"other-app".to_string(), Action::READ, "app")
                .is_err()
        );
    }
}
