// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Explainability helpers for dirty propagation.
//!
//! The core dirty-tracking structures intentionally do not store provenance for
//! why a key became dirty. For many embedders, it is useful to answer questions
//! like: “Why is this key dirty?”.
//!
//! This module provides a minimal, additive hook for eager propagation:
//! [`EagerPolicy::propagate_with_trace`](crate::EagerPolicy::propagate_with_trace),
//! plus a small recorder, [`OneParentRecorder`], which stores **one
//! plausible cause path** per key (a spanning forest).
//!
//! If you need “all roots / all paths”, that is a separate, explicitly-scoped
//! feature: it can be much more expensive in both time and memory.

use alloc::vec::Vec;
use core::hash::Hash;

use hashbrown::{HashMap, HashSet};

use crate::Channel;

/// The recorded cause of dirtiness for a `(key, channel)` pair.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DirtyCause<K> {
    /// The key was explicitly marked dirty (a root).
    Root,
    /// The key was marked dirty because it depends on `because`.
    Because {
        /// The immediate upstream key that caused this key to become dirty.
        because: K,
    },
}

/// A callback sink for eager propagation tracing.
///
/// See [`EagerPolicy::propagate_with_trace`](crate::EagerPolicy::propagate_with_trace).
pub trait DirtyTrace<K> {
    /// Called for the explicit root key that was marked dirty.
    ///
    /// `newly_dirty` indicates whether the key was newly inserted into the
    /// dirty set, or was already dirty.
    fn root(&mut self, key: K, channel: Channel, newly_dirty: bool);

    /// Called when `key` is reached from `because` during propagation.
    ///
    /// `newly_dirty` indicates whether `key` was newly inserted into the dirty
    /// set, or was already dirty.
    fn caused_by(&mut self, key: K, because: K, channel: Channel, newly_dirty: bool);
}

/// Records one parent pointer per dirty key (a spanning forest).
///
/// This stores a best-effort explanation path for *some* cause chain. When a
/// key has multiple possible upstream causes, the first one observed wins.
#[derive(Debug, Default, Clone)]
pub struct OneParentRecorder<K>
where
    K: Copy + Eq + Hash,
{
    causes: HashMap<(K, Channel), DirtyCause<K>>,
}

impl<K> OneParentRecorder<K>
where
    K: Copy + Eq + Hash,
{
    /// Creates an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            causes: HashMap::new(),
        }
    }

    /// Clears all recorded causes.
    pub fn clear(&mut self) {
        self.causes.clear();
    }

    /// Returns the recorded cause for `(key, channel)`, if any.
    #[must_use]
    pub fn cause(&self, key: K, channel: Channel) -> Option<DirtyCause<K>> {
        self.causes.get(&(key, channel)).copied()
    }

    /// Returns one plausible path from a dirty root to `key`.
    ///
    /// The returned vector is ordered from root to `key` (inclusive).
    #[must_use]
    pub fn explain_path(&self, key: K, channel: Channel) -> Option<Vec<K>> {
        let mut out = Vec::new();
        let mut seen: HashSet<K> = HashSet::new();

        let mut current = key;
        loop {
            if !seen.insert(current) {
                return None;
            }
            out.push(current);

            match self.cause(current, channel)? {
                DirtyCause::Root => break,
                DirtyCause::Because { because } => current = because,
            }
        }

        out.reverse();
        Some(out)
    }
}

impl<K> DirtyTrace<K> for OneParentRecorder<K>
where
    K: Copy + Eq + Hash,
{
    fn root(&mut self, key: K, channel: Channel, _newly_dirty: bool) {
        self.causes
            .entry((key, channel))
            .or_insert(DirtyCause::Root);
    }

    fn caused_by(&mut self, key: K, because: K, channel: Channel, _newly_dirty: bool) {
        self.causes
            .entry((key, channel))
            .or_insert(DirtyCause::Because { because });
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::{CycleHandling, DirtyGraph, DirtySet, EagerPolicy, TraversalScratch};
    use alloc::vec;

    const LAYOUT: Channel = Channel::new(0);

    #[test]
    fn records_one_parent_path() {
        // 1 <- 2 <- 3
        let mut g = DirtyGraph::<u32>::new();
        g.add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        g.add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();

        let mut dirty = DirtySet::<u32>::new();
        let mut scratch = TraversalScratch::new();
        let mut rec = OneParentRecorder::new();

        EagerPolicy.propagate_with_trace(1, LAYOUT, &g, &mut dirty, &mut scratch, &mut rec);

        assert_eq!(rec.explain_path(3, LAYOUT).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn can_fill_in_missing_causes_for_already_dirty_keys() {
        // 1 <- 2 <- 3
        let mut g = DirtyGraph::<u32>::new();
        g.add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        g.add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();

        let mut dirty = DirtySet::<u32>::new();
        // Pretend this was dirtied without tracing.
        dirty.mark(2, LAYOUT);

        let mut scratch = TraversalScratch::new();
        let mut rec = OneParentRecorder::new();
        EagerPolicy.propagate_with_trace(1, LAYOUT, &g, &mut dirty, &mut scratch, &mut rec);

        assert_eq!(rec.explain_path(2, LAYOUT).unwrap(), vec![1, 2]);
    }
}
