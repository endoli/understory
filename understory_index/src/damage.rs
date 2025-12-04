// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Batched damage structures returned by [`Index::commit`](crate::Index::commit).

use alloc::vec::Vec;

use crate::types::Aabb2D;

/// Batched damage summary returned by [`Index::commit`](crate::Index::commit).
#[derive(Clone, Debug)]
pub struct Damage<T> {
    /// Newly added AABBs since last commit.
    pub added: Vec<Aabb2D<T>>,
    /// Removed AABBs since last commit.
    pub removed: Vec<Aabb2D<T>>,
    /// Moved AABBs since last commit: (old, new).
    pub moved: Vec<(Aabb2D<T>, Aabb2D<T>)>,
}

impl<T> Default for Damage<T> {
    fn default() -> Self {
        Self {
            added: Vec::new(),
            removed: Vec::new(),
            moved: Vec::new(),
        }
    }
}

impl<T: Copy + PartialOrd> Damage<T> {
    /// True if no damage entries recorded.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.moved.is_empty()
    }

    /// Union of all AABBs affected. Returns `None` if empty.
    pub fn union(&self) -> Option<Aabb2D<T>> {
        let mut it = self
            .added
            .iter()
            .copied()
            .chain(self.removed.iter().copied())
            .chain(self.moved.iter().flat_map(|(a, b)| [*a, *b]));
        let first = it.next()?;
        Some(it.fold(first, |acc, r| acc.union(r)))
    }
}
