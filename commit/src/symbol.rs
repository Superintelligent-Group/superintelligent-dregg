//! Symbol table: bidirectional mapping between string names and field elements.
//!
//! Symbols are interned by hashing with BLAKE3 (truncated to 253 bits).
//! The reverse mapping is maintained for debugging and serialization.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::field::FieldElement;

/// A bidirectional symbol table mapping strings to field elements.
///
/// Forward direction (string → field element) is deterministic via BLAKE3 hash.
/// Reverse direction (field element → string) requires the table to have previously
/// interned the string.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SymbolTable {
    /// Reverse mapping: field element → original string.
    reverse: HashMap<[u8; 32], String>,
}

impl SymbolTable {
    /// Create a new empty symbol table.
    pub fn new() -> Self {
        Self {
            reverse: HashMap::new(),
        }
    }

    /// Intern a string symbol, returning its field element representation.
    ///
    /// The mapping is deterministic: the same string always produces the same
    /// field element regardless of whether it was previously interned.
    /// Interning just adds it to the reverse lookup table.
    pub fn intern(&mut self, s: &str) -> FieldElement {
        let fe = FieldElement::from_symbol(s);
        self.reverse.insert(fe.0, s.to_string());
        fe
    }

    /// Resolve a field element back to its original string.
    ///
    /// Returns None if this field element was never interned in this table,
    /// or if it was created from an integer (not a symbol).
    pub fn resolve(&self, fe: FieldElement) -> Option<&str> {
        self.reverse.get(&fe.0).map(String::as_str)
    }

    /// Check if a field element has a known string representation.
    pub fn contains(&self, fe: FieldElement) -> bool {
        self.reverse.contains_key(&fe.0)
    }

    /// Number of interned symbols.
    pub fn len(&self) -> usize {
        self.reverse.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.reverse.is_empty()
    }

    /// Iterate over all interned (field_element, string) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (FieldElement, &str)> {
        self.reverse
            .iter()
            .map(|(bytes, s)| (FieldElement(*bytes), s.as_str()))
    }

    /// Merge another symbol table into this one.
    /// On conflict, existing entries are preserved (first-interned wins).
    pub fn merge(&mut self, other: &SymbolTable) {
        for (bytes, s) in &other.reverse {
            self.reverse.entry(*bytes).or_insert_with(|| s.clone());
        }
    }

    /// Intern multiple symbols at once, returning their field elements.
    pub fn intern_many(&mut self, symbols: &[&str]) -> Vec<FieldElement> {
        symbols.iter().map(|s| self.intern(s)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_and_resolve() {
        let mut table = SymbolTable::new();
        let fe = table.intern("hello");
        assert_eq!(table.resolve(fe), Some("hello"));
    }

    #[test]
    fn intern_deterministic() {
        let mut t1 = SymbolTable::new();
        let mut t2 = SymbolTable::new();
        let fe1 = t1.intern("world");
        let fe2 = t2.intern("world");
        assert_eq!(fe1, fe2);
    }

    #[test]
    fn resolve_unknown_returns_none() {
        let table = SymbolTable::new();
        let fe = FieldElement::from_u64(42);
        assert_eq!(table.resolve(fe), None);
    }

    #[test]
    fn resolve_uninterned_symbol() {
        let table = SymbolTable::new();
        let fe = FieldElement::from_symbol("never_interned");
        assert_eq!(table.resolve(fe), None);
    }

    #[test]
    fn intern_many_symbols() {
        let mut table = SymbolTable::new();
        let fes = table.intern_many(&["alpha", "beta", "gamma"]);
        assert_eq!(fes.len(), 3);
        assert_eq!(table.resolve(fes[0]), Some("alpha"));
        assert_eq!(table.resolve(fes[1]), Some("beta"));
        assert_eq!(table.resolve(fes[2]), Some("gamma"));
    }

    #[test]
    fn merge_tables() {
        let mut t1 = SymbolTable::new();
        let mut t2 = SymbolTable::new();
        t1.intern("hello");
        t2.intern("world");
        t1.merge(&t2);
        let fe_world = FieldElement::from_symbol("world");
        assert_eq!(t1.resolve(fe_world), Some("world"));
    }

    #[test]
    fn len_and_is_empty() {
        let mut table = SymbolTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
        table.intern("x");
        assert!(!table.is_empty());
        assert_eq!(table.len(), 1);
        // Re-interning same symbol doesn't change length.
        table.intern("x");
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn iter_symbols() {
        let mut table = SymbolTable::new();
        table.intern("a");
        table.intern("b");
        table.intern("c");
        let mut names: Vec<&str> = table.iter().map(|(_, s)| s).collect();
        names.sort();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn contains_check() {
        let mut table = SymbolTable::new();
        let fe = table.intern("present");
        assert!(table.contains(fe));
        let absent = FieldElement::from_symbol("absent");
        assert!(!table.contains(absent));
    }
}
