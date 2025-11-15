// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Backend trait and subtree summary/filter abstractions for spatial indexing implementations.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::types::Aabb2D;
use core::fmt::Debug;

/// Summary value cached per subtree for pruning.
///
/// Backends can use this to maintain per-node aggregates (e.g. bitmasks, layer sets, statistics)
/// that allow early-out decisions during traversal. The default implementation for `()` is
/// zero-cost: it stores no data in nodes, and [`SubtreeFilter`] for `()` always keeps subtrees.
pub trait SubtreeSummary: Copy {
    /// Identity summary for an empty subtree.
    fn empty() -> Self;

    /// Combine summaries of two child subtrees.
    fn combine(left: Self, right: Self) -> Self;
}

impl SubtreeSummary for () {
    #[inline]
    fn empty() -> Self {}

    #[inline]
    fn combine(_: Self, _: Self) -> Self {}
}

impl SubtreeSummary for u64 {
    #[inline]
    fn empty() -> Self {
        0
    }

    #[inline]
    fn combine(left: Self, right: Self) -> Self {
        left | right
    }
}

/// Query-dependent filter for subtree summaries.
///
/// A filter decides whether a subtree *may* contain relevant content for a given query. It should
/// be conservative: returning `false` must guarantee that no descendant slot is relevant.
pub trait SubtreeFilter<S, Q> {
    /// Returns `true` if the subtree described by `summary` might contain matches for `query`.
    fn may_contain(&self, summary: &S, query: &Q) -> bool;
}

/// Trivial filter for the zero-sized `()` summary: never prunes.
impl<Q> SubtreeFilter<(), Q> for () {
    #[inline]
    fn may_contain(&self, _summary: &(), _query: &Q) -> bool {
        true
    }
}

/// Convenience filter for `u64` summaries interpreted as bitmasks.
///
/// The query `u64` is treated as a "required bits" mask; a subtree is considered relevant if its
/// summary shares any bits with the query.
impl SubtreeFilter<u64, u64> for () {
    #[inline]
    fn may_contain(&self, summary: &u64, query: &u64) -> bool {
        (*summary & *query) != 0
    }
}

/// Spatial backend abstraction used by `IndexGeneric`.
pub trait Backend<T, S>
where
    T: Copy + PartialOrd + Debug,
    S: SubtreeSummary,
{
    /// Insert a new slot into the spatial structure.
    fn insert(&mut self, slot: usize, aabb: Aabb2D<T>);

    /// Update an existing slot's AABB.
    fn update(&mut self, slot: usize, aabb: Aabb2D<T>);

    /// Remove a slot from the spatial structure.
    fn remove(&mut self, slot: usize);

    /// Clear all spatial structures.
    fn clear(&mut self);

    /// Optionally associate a subtree summary with a slot.
    ///
    /// Backends that want to support subtree-level pruning can override this and cache aggregate
    /// summaries in their internal nodes. The default implementation ignores summaries.
    fn set_summary(&mut self, _slot: usize, _summary: S) {}

    /// Visit slots whose AABB contains the point.
    fn visit_point<F: FnMut(usize)>(&self, x: T, y: T, f: F);

    /// Visit slots whose AABB contains the point, with an optional summary-based filter.
    ///
    /// The default implementation ignores the filter and calls [`Backend::visit_point`].
    fn visit_point_filtered<Q, Filt, F>(&self, x: T, y: T, query: &Q, _filter: &Filt, f: F)
    where
        Filt: SubtreeFilter<S, Q>,
        F: FnMut(usize),
    {
        let _ = query;
        self.visit_point(x, y, f);
    }

    /// Visit slots whose AABB intersects the rectangle.
    fn visit_rect<F: FnMut(usize)>(&self, rect: Aabb2D<T>, f: F);

    /// Visit slots whose AABB intersects the rectangle, with an optional summary-based filter.
    ///
    /// The default implementation ignores the filter and calls [`Backend::visit_rect`].
    fn visit_rect_filtered<Q, Filt, F>(&self, rect: Aabb2D<T>, query: &Q, _filter: &Filt, f: F)
    where
        Filt: SubtreeFilter<S, Q>,
        F: FnMut(usize),
    {
        let _ = query;
        self.visit_rect(rect, f);
    }

    /// Query slots whose AABB contains the point. Default: collects `visit_point`.
    fn query_point<'a>(&'a self, x: T, y: T) -> Box<dyn Iterator<Item = usize> + 'a> {
        let mut out = Vec::new();
        self.visit_point(x, y, |i| out.push(i));
        Box::new(out.into_iter())
    }

    /// Query slots whose AABB intersects the rectangle. Default: collects `visit_rect`.
    fn query_rect<'a>(&'a self, rect: Aabb2D<T>) -> Box<dyn Iterator<Item = usize> + 'a> {
        let mut out = Vec::new();
        self.visit_rect(rect, |i| out.push(i));
        Box::new(out.into_iter())
    }
}
