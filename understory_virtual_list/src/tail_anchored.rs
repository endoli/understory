// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Tail-anchored extent model wrapper (chat/log-style lists).
//!
//! [`TailAnchoredExtentModel`] wraps an existing [`ExtentModel`] and provides
//! helpers for tail-anchored scrolling patterns common in chat logs, consoles,
//! and append-only feeds:
//!
//! - The underlying model still describes items `0..len` in increasing order.
//! - The wrapper exposes helpers to:
//!   - compute the scroll offset that keeps the tail visible, and
//!   - detect whether a given scroll offset is "at the tail" within a small
//!     tolerance (`anchor_epsilon`).
//!
//! This allows hosts to:
//!
//! 1. Query whether the user is currently anchored to the tail.
//! 2. Update item extents or append new items.
//! 3. If they were anchored, restore a tail-aligned scroll offset so the
//!    visible region stays pinned to the end of the content.
//!
//! ## Minimal chat-style example
//!
//! ```rust
//! use understory_virtual_list::{PrefixSumExtentModel, TailAnchoredExtentModel, VirtualList};
//!
//! // Backing model for item extents (e.g., row heights).
//! let mut inner = PrefixSumExtentModel::<f32>::new();
//! inner.set_len(3);
//! inner.set_extent(0, 10.0);
//! inner.set_extent(1, 10.0);
//! inner.set_extent(2, 10.0);
//!
//! // Wrap with tail-anchoring helpers and drive via VirtualList.
//! let model = TailAnchoredExtentModel::with_default_epsilon(inner);
//! let mut list = VirtualList::new(model, 20.0_f32, 0.0);
//!
//! // Start anchored to the tail.
//! list.scroll_to_tail();
//! assert!(list.is_at_tail());
//!
//! // Remember whether we were anchored to the tail before mutating.
//! let was_at_tail = list.is_at_tail();
//!
//! // Append a new item and give it an extent.
//! {
//!     let inner = list.model_mut().inner_mut();
//!     inner.set_len(4);
//!     inner.set_extent(3, 10.0);
//! }
//!
//! // If we were at the tail before, keep the view pinned after the update.
//! if was_at_tail {
//!     list.scroll_to_tail();
//! }
//! assert!(list.is_at_tail());
//! ```

use crate::{ExtentModel, Scalar};

/// Wraps an [`ExtentModel`] with tail-anchoring helpers.
///
/// The `anchor_epsilon` field controls how "sticky" the tail behavior is:
/// - When [`TailAnchoredExtentModel::is_at_tail`] is called, the current scroll
///   offset is considered anchored if it is within `anchor_epsilon` *below* the
///   ideal tail-aligned offset.
/// - Larger values make the tail more forgiving (treat a wider range of offsets
///   near the end as "at the bottom").
/// - Smaller values make the notion of "at the tail" stricter.
///
/// Typical reasons to tune `anchor_epsilon`:
/// - **Coordinate scale**: if one logical unit corresponds to a large visual jump
///   (e.g., very tall rows), a tolerance of `1.0` might be too strict; conversely,
///   for tiny rows it might be too loose.
/// - **UX feel**: some UIs intentionally treat "near the bottom" as "at bottom"
///   so new messages keep auto-scrolling; others prefer precise behavior.
/// - **Non-pixel units**: when the scalar is something like time or zoomed world
///   units, it can make sense to express tolerance in those units (for example,
///   "0.5 seconds of timeline").
///
/// Callers that are happy with a sensible default can use
/// [`TailAnchoredExtentModel::with_default_epsilon`] and never touch the
/// tolerance; others can tune it via [`TailAnchoredExtentModel::set_anchor_epsilon`].
#[derive(Debug, Clone)]
pub struct TailAnchoredExtentModel<M: ExtentModel> {
    inner: M,
    anchor_epsilon: M::Scalar,
}

impl<M: ExtentModel> TailAnchoredExtentModel<M> {
    /// Creates a new wrapper with a custom anchoring tolerance.
    ///
    /// `anchor_epsilon` controls how close the scroll offset must be to the
    /// tail-aligned position to count as "at the tail". Larger values are
    /// more permissive.
    #[must_use]
    pub fn new(inner: M, anchor_epsilon: M::Scalar) -> Self {
        Self {
            inner,
            anchor_epsilon,
        }
    }

    /// Creates a new wrapper with a reasonable default anchoring tolerance.
    ///
    /// The default uses a tolerance of 1 unit in the model's scalar space.
    #[must_use]
    pub fn with_default_epsilon(inner: M) -> Self {
        Self::new(inner, M::Scalar::from_usize(1))
    }

    /// Returns a shared reference to the underlying model.
    #[must_use]
    pub fn inner(&self) -> &M {
        &self.inner
    }

    /// Returns a mutable reference to the underlying model.
    pub fn inner_mut(&mut self) -> &mut M {
        &mut self.inner
    }

    /// Returns the anchoring tolerance.
    #[must_use]
    pub fn anchor_epsilon(&self) -> M::Scalar {
        self.anchor_epsilon
    }

    /// Sets the anchoring tolerance.
    pub fn set_anchor_epsilon(&mut self, epsilon: M::Scalar) {
        self.anchor_epsilon = epsilon;
    }

    /// Computes the scroll offset that keeps the tail visible for a viewport.
    ///
    /// The returned offset is clamped to `>= 0` and is `0` when content fits
    /// entirely inside the viewport.
    #[must_use]
    pub fn tail_scroll_offset(&mut self, viewport_extent: M::Scalar) -> M::Scalar {
        let total = self.inner.total_extent().max(M::Scalar::zero());
        let viewport = viewport_extent.max(M::Scalar::zero());
        if total <= viewport {
            M::Scalar::zero()
        } else {
            total - viewport
        }
    }

    /// Returns `true` if `scroll_offset` is considered anchored to the tail.
    ///
    /// The check is asymmetric: it returns `true` when the scroll offset is
    /// within `anchor_epsilon` *below* the tail-aligned offset. This matches
    /// the common behavior of treating positions near the bottom as "at the
    /// bottom" for chat/log-style views.
    #[must_use]
    pub fn is_at_tail(&mut self, scroll_offset: M::Scalar, viewport_extent: M::Scalar) -> bool {
        let tail = self.tail_scroll_offset(viewport_extent);
        let offset = scroll_offset.max(M::Scalar::zero());
        offset + self.anchor_epsilon >= tail
    }
}

impl<M: ExtentModel> ExtentModel for TailAnchoredExtentModel<M> {
    type Scalar = M::Scalar;

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn total_extent(&mut self) -> Self::Scalar {
        self.inner.total_extent()
    }

    fn extent_of(&mut self, index: usize) -> Self::Scalar {
        self.inner.extent_of(index)
    }

    fn offset_of(&mut self, index: usize) -> Self::Scalar {
        self.inner.offset_of(index)
    }

    fn index_at_offset(&mut self, offset: Self::Scalar) -> usize {
        self.inner.index_at_offset(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::TailAnchoredExtentModel;
    use crate::FixedExtentModel;

    #[test]
    fn tail_scroll_offset_matches_content_minus_viewport() {
        // 10 items * 10 = 100 total extent.
        let inner = FixedExtentModel::new(10, 10.0_f32);
        let mut model = TailAnchoredExtentModel::with_default_epsilon(inner);

        let viewport = 30.0_f32;
        let tail = model.tail_scroll_offset(viewport);
        // Content 100, viewport 30 → tail offset 70.
        assert!((tail - 70.0_f32).abs() < f32::EPSILON);

        // When content fits in viewport, tail offset is 0.
        let viewport_large = 200.0_f32;
        let tail_large = model.tail_scroll_offset(viewport_large);
        assert!((tail_large - 0.0_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn is_at_tail_detects_near_tail_offsets() {
        let inner = FixedExtentModel::new(10, 10.0_f32); // total 100
        let mut model = TailAnchoredExtentModel::new(inner, 1.0_f32);
        let viewport = 30.0_f32;
        let tail = model.tail_scroll_offset(viewport);

        // Exactly at tail is anchored.
        assert!(model.is_at_tail(tail, viewport));
        // Slightly below tail (within epsilon) is anchored.
        assert!(model.is_at_tail(tail - 0.5, viewport));
        // Far above tail is not anchored.
        assert!(!model.is_at_tail(tail - 10.0, viewport));
    }
}
