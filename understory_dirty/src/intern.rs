// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Interning helper for non-`Copy` keys.
//!
//! `understory_dirty` intentionally keeps the core APIs keyed by `K: Copy` to
//! protect hot paths from accidental cloning and allocation. Many embedders,
//! however, naturally have structured/owned keys (strings, compound IDs, etc.).
//!
//! This module provides a small, `no_std + alloc` interner that maps owned keys
//! to a compact [`InternId`] that can be used with [`crate::DirtyTracker`].
//!
//! ## Example
//!
//! ```rust
//! use understory_dirty::{intern::Interner, Channel, DirtyTracker, InternId, LazyPolicy};
//!
//! const LAYOUT: Channel = Channel::new(0);
//!
//! #[derive(Clone, Debug, Eq, PartialEq, Hash)]
//! struct ResourceKey(&'static str);
//!
//! let mut ids = Interner::<ResourceKey>::new();
//! let a: InternId = ids.intern(ResourceKey("a"));
//! let b: InternId = ids.intern(ResourceKey("b"));
//!
//! let mut tracker = DirtyTracker::<InternId>::new();
//! tracker.add_dependency(b, a, LAYOUT).unwrap();
//! tracker.mark_with(a, LAYOUT, &LazyPolicy);
//!
//! let order: Vec<_> = tracker.drain_affected_sorted(LAYOUT).collect();
//! assert_eq!(order, vec![a, b]);
//!
//! // Best-effort debug lookup:
//! assert_eq!(ids.get(a).unwrap().0, "a");
//! ```

use alloc::vec::Vec;
use core::hash::{BuildHasher, Hash};

use hashbrown::DefaultHashBuilder;
use hashbrown::HashMap;

/// A compact, interned identifier.
///
/// This is intended to be used as the `K` parameter for `understory_dirty`
/// core types like [`DirtyTracker`](crate::DirtyTracker).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(transparent)]
pub struct InternId(u32);

impl InternId {
    /// Returns this id as a `usize` index (for tables keyed by intern ids).
    #[inline]
    #[must_use]
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }

    /// Returns the raw numeric id.
    #[inline]
    #[must_use]
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// Interns owned keys into compact [`InternId`] handles.
///
/// Keys are stored once in an internal table. Lookups use a hash-bucket index
/// (hash -> small list of candidate ids) to avoid storing duplicate key copies.
#[derive(Debug, Clone)]
pub struct Interner<K> {
    keys: Vec<K>,
    buckets: HashMap<u64, Vec<InternId>>,
    build_hasher: DefaultHashBuilder,
}

impl<K> Default for Interner<K>
where
    K: Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K> Interner<K>
where
    K: Eq + Hash,
{
    /// Creates an empty interner.
    #[must_use]
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            buckets: HashMap::new(),
            build_hasher: DefaultHashBuilder::default(),
        }
    }

    /// Returns the number of interned keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Returns `true` if the interner contains no keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Returns the key for an interned id, if the id is in-range.
    #[must_use]
    pub fn get(&self, id: InternId) -> Option<&K> {
        self.keys.get(id.as_usize())
    }

    /// Interns `key` and returns its [`InternId`].
    ///
    /// If an equal key was already interned, this returns the existing id and
    /// drops `key`.
    pub fn intern(&mut self, key: K) -> InternId {
        let hash = self.hash(&key);
        if let Some(ids) = self.buckets.get(&hash) {
            for &id in ids {
                if self.keys[id.as_usize()] == key {
                    return id;
                }
            }
        }

        let id = InternId(
            u32::try_from(self.keys.len()).expect("too many interned keys for InternId (u32)"),
        );
        self.keys.push(key);
        self.buckets.entry(hash).or_default().push(id);
        id
    }

    /// Clears all interned keys.
    ///
    /// This drops all stored keys and invalidates any previously returned ids.
    pub fn clear(&mut self) {
        self.keys.clear();
        self.buckets.clear();
    }

    fn hash<Q>(&self, key: &Q) -> u64
    where
        Q: Hash + ?Sized,
    {
        self.build_hasher.hash_one(key)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[derive(Debug, Eq, PartialEq, Hash)]
    struct Key(&'static str);

    #[test]
    fn interns_duplicates_to_same_id() {
        let mut i = Interner::<Key>::new();
        let a0 = i.intern(Key("a"));
        let a1 = i.intern(Key("a"));
        let b = i.intern(Key("b"));

        assert_eq!(a0, a1);
        assert_ne!(a0, b);
        assert_eq!(i.get(a0).unwrap(), &Key("a"));
        assert_eq!(i.get(b).unwrap(), &Key("b"));
    }
}
