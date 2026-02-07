// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reusable scratch buffers for graph traversals.

use alloc::vec::Vec;
use core::hash::Hash;

use hashbrown::HashSet;

/// Reusable scratch storage for graph traversals.
///
/// This is useful in tight loops (many marks per frame) to avoid allocating
/// temporary `Vec`/`HashSet` state on every traversal.
///
/// The scratch buffers retain capacity across calls. Callers should reuse a
/// single scratch instance per thread / update pass.
///
/// # See Also
///
/// - [`DirtyGraph::for_each_transitive_dependent`](crate::DirtyGraph::for_each_transitive_dependent):
///   Scratch-powered traversal.
/// - [`EagerPolicy::propagate_with_scratch`](crate::EagerPolicy::propagate_with_scratch):
///   Scratch-powered eager propagation.
#[derive(Debug, Default)]
pub struct TraversalScratch<K>
where
    K: Copy + Eq + Hash,
{
    pub(crate) stack: Vec<K>,
    pub(crate) visited: HashSet<K>,
}

impl<K> TraversalScratch<K>
where
    K: Copy + Eq + Hash,
{
    /// Creates an empty scratch buffer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            visited: HashSet::new(),
        }
    }

    /// Creates an empty scratch buffer with pre-allocated capacity.
    ///
    /// `capacity` is a best-effort hint for both the internal stack and the
    /// visited set.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            stack: Vec::with_capacity(capacity),
            visited: HashSet::with_capacity(capacity),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.stack.clear();
        self.visited.clear();
    }
}
