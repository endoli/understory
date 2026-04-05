// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Expansion state bookkeeping.

use core::hash::Hash;

use hashbrown::HashSet;

/// Tracks the set of currently expanded keys.
///
/// `ExpansionState` intentionally uses a small `Vec<K>` with uniqueness
/// enforced by equality. This keeps the type easy to integrate with existing
/// ID types without requiring `Hash` or `Ord`.
#[derive(Clone, Debug, Default)]
pub struct ExpansionState<K> {
    expanded: HashSet<K>,
    revision: u64,
}

impl<K: Eq + Hash> ExpansionState<K> {
    /// Creates an empty expansion state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            expanded: HashSet::new(),
            revision: 0,
        }
    }

    /// Returns `true` if no keys are currently expanded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.expanded.is_empty()
    }

    /// Returns the number of expanded keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.expanded.len()
    }

    /// Returns the current revision counter.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the set of expanded keys.
    #[must_use]
    pub fn expanded_keys(&self) -> &HashSet<K> {
        &self.expanded
    }

    /// Returns an iterator over expanded keys.
    pub fn iter(&self) -> hashbrown::hash_set::Iter<'_, K> {
        self.expanded.iter()
    }

    /// Returns `true` if `key` is expanded.
    #[must_use]
    pub fn is_expanded(&self, key: &K) -> bool {
        self.expanded.contains(key)
    }

    /// Sets the expansion state for `key`.
    ///
    /// Returns `true` if the semantic state changed.
    pub fn set_expanded(&mut self, key: K, expanded: bool) -> bool {
        if expanded {
            if self.expanded.insert(key) {
                self.revision = self.revision.wrapping_add(1);
                true
            } else {
                false
            }
        } else if self.expanded.remove(&key) {
            self.revision = self.revision.wrapping_add(1);
            true
        } else {
            false
        }
    }

    /// Toggles the expansion state for `key`.
    ///
    /// Returns the new expanded state.
    pub fn toggle(&mut self, key: K) -> bool
    where
        K: Clone,
    {
        let expanded = !self.is_expanded(&key);
        let _ = self.set_expanded(key, expanded);
        expanded
    }

    /// Replaces the expanded set with `keys`, deduplicating by equality.
    ///
    /// Returns `true` if the semantic state changed.
    pub fn replace_with<I>(&mut self, keys: I) -> bool
    where
        I: IntoIterator<Item = K>,
    {
        let next: HashSet<K> = keys.into_iter().collect();

        if self.expanded == next {
            return false;
        }

        self.expanded = next;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Clears all expanded keys.
    ///
    /// Returns `true` if any key was cleared.
    pub fn clear(&mut self) -> bool {
        if self.expanded.is_empty() {
            return false;
        }
        self.expanded.clear();
        self.revision = self.revision.wrapping_add(1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::ExpansionState;

    #[test]
    fn set_toggle_and_clear_update_revision() {
        let mut expansion = ExpansionState::new();

        assert!(expansion.set_expanded(1_u32, true));
        assert!(expansion.is_expanded(&1));
        let revision = expansion.revision();

        assert!(!expansion.set_expanded(1, true));
        assert_eq!(expansion.revision(), revision);

        assert!(!expansion.toggle(1));
        assert!(!expansion.is_expanded(&1));
        assert!(!expansion.clear());
    }

    #[test]
    fn replace_with_deduplicates() {
        let mut expansion = ExpansionState::new();
        assert!(expansion.replace_with([1_u32, 2, 1, 3]));
        assert_eq!(expansion.len(), 3);
        assert!(expansion.is_expanded(&1));
        assert!(expansion.is_expanded(&2));
        assert!(expansion.is_expanded(&3));
        assert!(!expansion.replace_with([1_u32, 2, 3]));
    }
}
