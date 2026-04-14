// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Core outline model traits.

use core::hash::Hash;

/// A hierarchical model addressable by stable keys.
///
/// `OutlineModel` describes enough structure to traverse roots, siblings, and
/// children without assuming any particular storage representation.
pub trait OutlineModel {
    /// Stable key type used to identify rows.
    type Key: Clone + Eq + Hash;

    /// Item returned when resolving a key.
    type Item;

    /// Returns the first root key, if any.
    fn first_root_key(&self) -> Option<Self::Key>;

    /// Returns `true` if `key` is currently part of this outline.
    fn contains_key(&self, key: &Self::Key) -> bool;

    /// Returns the next sibling key after `key`, if any.
    ///
    /// This method is used for both top-level roots and nested children.
    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key>;

    /// Returns the first child key under `key`, if any.
    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key>;

    /// Resolves the item associated with `key`.
    fn item(&self, key: &Self::Key) -> Option<Self::Item>;
}

impl<M> OutlineModel for &M
where
    M: OutlineModel + ?Sized,
{
    type Key = M::Key;
    type Item = M::Item;

    fn first_root_key(&self) -> Option<Self::Key> {
        (**self).first_root_key()
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        (**self).contains_key(key)
    }

    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
        (**self).next_sibling_key(key)
    }

    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
        (**self).first_child_key(key)
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        (**self).item(key)
    }
}

impl<M> OutlineModel for &mut M
where
    M: OutlineModel + ?Sized,
{
    type Key = M::Key;
    type Item = M::Item;

    fn first_root_key(&self) -> Option<Self::Key> {
        (**self).first_root_key()
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        (**self).contains_key(key)
    }

    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
        (**self).next_sibling_key(key)
    }

    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
        (**self).first_child_key(key)
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        (**self).item(key)
    }
}
