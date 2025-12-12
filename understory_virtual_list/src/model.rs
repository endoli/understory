// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Core extent model traits and helpers.

use core::cmp;

use crate::Scalar;

/// Result of a visibility query over a 1D strip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleStrip<S: Scalar> {
    /// First visible index (inclusive).
    pub start: usize,
    /// One past the last visible index (exclusive).
    pub end: usize,

    /// Total extent of items before `start`.
    pub before_extent: S,
    /// Total extent of items after `end`.
    pub after_extent: S,
    /// Total extent of the entire strip (all items `0..len`).
    pub content_extent: S,
}

impl<S: Scalar> VisibleStrip<S> {
    /// Returns `true` if there are no visible items.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// A 1D model over a dense strip of items, indexed `0..len`.
///
/// All extents and offsets are in the same coordinate space as your scroll offset,
/// viewport extent, and spacer nodes (typically logical pixels).
///
/// Methods that logically consult prefix sums take `&mut self` so implementations
/// are free to maintain internal caches without exposing interior mutability at
/// the call site.
pub trait ExtentModel {
    /// Scalar type used for extents and offsets.
    type Scalar: Scalar;

    /// Number of items in this strip.
    fn len(&self) -> usize;

    /// Returns `true` if there are no items in this strip.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total extent of the entire strip.
    fn total_extent(&mut self) -> Self::Scalar;

    /// Size of a single item.
    ///
    /// Implementations must return a non-negative value. Returning zero is
    /// allowed but may cause degenerate behavior if *all* items are zero-sized.
    fn extent_of(&mut self, index: usize) -> Self::Scalar;

    /// Offset of the start of the given item from the start of the strip.
    ///
    /// Implementations must guarantee that:
    /// - if `len() > 0`, `offset_of(0) == 0`,
    /// - for all valid `i`, `offset_of(i + 1) >= offset_of(i) + extent_of(i)`.
    fn offset_of(&mut self, index: usize) -> Self::Scalar;

    /// Given a scroll offset, find an index `i` such that:
    ///
    /// - the item at `i` is at or before that offset,
    /// - and `i` is clamped into `0..=len()`.
    ///
    /// Typical implementations use prefix sums plus a binary search.
    fn index_at_offset(&mut self, offset: Self::Scalar) -> usize;
}

/// An [`ExtentModel`] whose logical length can be resized.
///
/// Implementations are expected to ensure storage for `len` items and treat
/// any newly added items as having extent `0.0` until explicitly updated.
pub trait ResizableExtentModel: ExtentModel {
    /// Ensures that the model can represent `len` items.
    ///
    /// Implementations typically grow internal storage and treat new items as
    /// zero-sized until their extents are set by the caller.
    fn set_len(&mut self, len: usize);
}

/// Compute the visible slice of a strip, given scroll position, viewport size, and overscan.
///
/// - `scroll_offset`: top of the viewport in strip coordinates (`>= 0`).
/// - `viewport_extent`: size of the viewport in strip coordinates (`>= 0`).
/// - `overscan_before`: extra margin *before* the viewport to reduce popping.
/// - `overscan_after`: extra margin *after* the viewport to reduce popping.
///
/// The returned [`VisibleStrip`] tells you:
/// - Which indices to materialize: `[start, end)`.
/// - How much padding to place before/after the realized chunk.
/// - The total content extent.
pub fn compute_visible_strip<M>(
    model: &mut M,
    scroll_offset: M::Scalar,
    viewport_extent: M::Scalar,
    overscan_before: M::Scalar,
    overscan_after: M::Scalar,
) -> VisibleStrip<M::Scalar>
where
    M: ExtentModel,
{
    type S<M> = <M as ExtentModel>::Scalar;
    let len = model.len();
    if len == 0 {
        return VisibleStrip {
            start: 0,
            end: 0,
            before_extent: S::<M>::zero(),
            after_extent: S::<M>::zero(),
            content_extent: S::<M>::zero(),
        };
    }

    let mut content_extent = model.total_extent().max(S::<M>::zero());
    if content_extent == S::<M>::zero() {
        // All items collapsed; treat as empty strip.
        return VisibleStrip {
            start: 0,
            end: 0,
            before_extent: S::<M>::zero(),
            after_extent: S::<M>::zero(),
            content_extent: S::<M>::zero(),
        };
    }

    let scroll_offset = scroll_offset.max(S::<M>::zero());
    let viewport_extent = viewport_extent.max(S::<M>::zero());
    let overscan_before = overscan_before.max(S::<M>::zero());
    let overscan_after = overscan_after.max(S::<M>::zero());

    let min = (scroll_offset - overscan_before).max(S::<M>::zero());
    let max = (scroll_offset + viewport_extent + overscan_after).min(content_extent);

    if max <= min {
        // Very small viewport / overscan, or near-zero content.
        return VisibleStrip {
            start: 0,
            end: 0,
            before_extent: min,
            after_extent: (content_extent - min).max(S::<M>::zero()),
            content_extent,
        };
    }

    // Start from the item whose start is at or before `min`.
    let mut start = {
        let idx = model.index_at_offset(min);
        cmp::min(idx, len.saturating_sub(1))
    };

    // Walk backwards to make sure item_at(start) actually starts <= min.
    while start > 0 && model.offset_of(start) > min {
        start -= 1;
    }

    // Walk forwards until we pass `max`.
    let mut end = start;
    while end < len && model.offset_of(end) < max {
        end += 1;
    }

    let before_extent = model.offset_of(start);
    content_extent = model.total_extent().max(content_extent);

    let end_start = if end < len {
        model.offset_of(end)
    } else {
        content_extent
    };
    let after_extent = (content_extent - end_start).max(S::<M>::zero());

    VisibleStrip {
        start,
        end,
        before_extent,
        after_extent,
        content_extent,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{ExtentModel, VisibleStrip, compute_visible_strip};
    use crate::Scalar;

    #[derive(Clone, Debug)]
    struct SimpleModel {
        extents: Vec<f32>,
    }

    impl SimpleModel {
        fn new(extents: &[f32]) -> Self {
            Self {
                extents: extents.to_vec(),
            }
        }
    }

    impl ExtentModel for SimpleModel {
        type Scalar = f32;

        fn len(&self) -> usize {
            self.extents.len()
        }

        fn total_extent(&mut self) -> Self::Scalar {
            self.extents.iter().copied().sum()
        }

        fn extent_of(&mut self, index: usize) -> Self::Scalar {
            self.extents.get(index).copied().unwrap_or(0.0)
        }

        fn offset_of(&mut self, index: usize) -> Self::Scalar {
            self.extents.iter().take(index).copied().sum()
        }

        fn index_at_offset(&mut self, offset: Self::Scalar) -> usize {
            let mut pos = 0.0;
            for (i, extent) in self.extents.iter().copied().enumerate() {
                if pos + extent > offset {
                    return i;
                }
                pos += extent;
            }
            self.extents.len().saturating_sub(1)
        }
    }

    #[test]
    fn empty_model_yields_empty_strip() {
        let mut model = SimpleModel::new(&[]);
        let strip = compute_visible_strip(&mut model, 0.0, 100.0, 10.0, 10.0);
        assert_eq!(
            strip,
            VisibleStrip {
                start: 0,
                end: 0,
                before_extent: <f32 as Scalar>::zero(),
                after_extent: <f32 as Scalar>::zero(),
                content_extent: <f32 as Scalar>::zero(),
            }
        );
    }

    #[test]
    fn simple_visible_range() {
        // Three items, each 10 units tall.
        let mut model = SimpleModel::new(&[10.0, 10.0, 10.0]);
        let strip = compute_visible_strip(&mut model, 5.0, 10.0, 0.0, 0.0);
        assert_eq!(strip.start, 0);
        assert_eq!(strip.end, 2);
        assert_eq!(strip.before_extent, 0.0);
        assert_eq!(strip.after_extent, 10.0);
        assert_eq!(strip.content_extent, 30.0);
    }

    #[test]
    fn asymmetric_overscan_extends_in_one_direction() {
        let mut model = SimpleModel::new(&[10.0, 10.0, 10.0, 10.0]);
        // Viewport covers roughly items 1 and 2 (offset 10..30). Overscan only after.
        let strip = compute_visible_strip(&mut model, 10.0, 20.0, 0.0, 10.0);
        // We should still start at item 1, but extend end to include item 3.
        assert_eq!(strip.start, 1);
        assert_eq!(strip.end, 4);
    }
}
