// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A small controller that owns an [`ExtentModel`] and scroll state.

use core::ops::Range;

use crate::{
    ExtentModel, ResizableExtentModel, Scalar, TailAnchoredExtentModel, VisibleStrip,
    compute_visible_strip,
};

/// Alignment mode when scrolling a specific index into view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAlign {
    /// Align the start (top/leading edge) of the item with the viewport.
    Start,
    /// Center the item within the viewport.
    Center,
    /// Align the end (bottom/trailing edge) of the item with the viewport.
    End,
    /// Move just enough to make the item fully visible, preferring the
    /// smallest change from the current scroll offset.
    Nearest,
}

/// Controller for a virtualized list/stack over a dense index strip.
///
/// This type:
/// - stores scroll offset, viewport extent, and asymmetric overscan,
/// - owns an [`ExtentModel`],
/// - caches the last computed [`VisibleStrip`],
/// - exposes helpers for visibility queries and index-aligned scrolling.
///
/// It does *not* know about any widget/view system; host frameworks are expected
/// to wrap this and drive child creation/removal and spacer nodes.
#[derive(Debug, Clone)]
pub struct VirtualList<M: ExtentModel> {
    model: M,
    scroll_offset: M::Scalar,
    viewport_extent: M::Scalar,
    overscan_before: M::Scalar,
    overscan_after: M::Scalar,

    dirty: bool,
    last_strip: VisibleStrip<M::Scalar>,
}

impl<M: ExtentModel> VirtualList<M> {
    /// Creates a new [`VirtualList`] with the given `model`, `viewport_extent`, and symmetric `overscan`.
    #[must_use]
    pub fn new(model: M, viewport_extent: M::Scalar, overscan: M::Scalar) -> Self {
        Self {
            model,
            scroll_offset: M::Scalar::zero(),
            viewport_extent: viewport_extent.max(M::Scalar::zero()),
            overscan_before: overscan.max(M::Scalar::zero()),
            overscan_after: overscan.max(M::Scalar::zero()),
            dirty: true,
            last_strip: VisibleStrip {
                start: 0,
                end: 0,
                before_extent: M::Scalar::zero(),
                after_extent: M::Scalar::zero(),
                content_extent: M::Scalar::zero(),
            },
        }
    }

    /// Returns a shared reference to the underlying model.
    #[must_use]
    pub fn model(&self) -> &M {
        &self.model
    }

    /// Returns a mutable reference to the underlying model, marking the cached strip dirty.
    pub fn model_mut(&mut self) -> &mut M {
        self.dirty = true;
        &mut self.model
    }

    /// Returns the number of items in the underlying model.
    #[must_use]
    pub fn len(&self) -> usize {
        self.model.len()
    }

    /// Returns `true` if the underlying model contains no items.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.model.is_empty()
    }

    /// Returns the total extent of the underlying model.
    ///
    /// Unlike [`VirtualList::model_mut`], this may update model-internal
    /// caches without marking the cached materialized strip dirty.
    #[must_use]
    pub fn total_extent(&mut self) -> M::Scalar {
        self.model.total_extent()
    }

    /// Returns the extent of `index` in the underlying model.
    ///
    /// Unlike [`VirtualList::model_mut`], this may update model-internal
    /// caches without marking the cached materialized strip dirty.
    #[must_use]
    pub fn extent_of(&mut self, index: usize) -> M::Scalar {
        self.model.extent_of(index)
    }

    /// Returns the offset of `index` in the underlying model.
    ///
    /// Unlike [`VirtualList::model_mut`], this may update model-internal
    /// caches without marking the cached materialized strip dirty.
    #[must_use]
    pub fn offset_of(&mut self, index: usize) -> M::Scalar {
        self.model.offset_of(index)
    }

    /// Finds the greatest index whose item starts at or before `offset`.
    ///
    /// This mirrors [`ExtentModel::index_at_offset`], including returning `0`
    /// as a sentinel when the model is empty. Prefer
    /// [`VirtualList::try_index_at_offset`] when the model may be empty.
    ///
    /// Unlike [`VirtualList::model_mut`], this may update model-internal
    /// caches without marking the cached materialized strip dirty.
    #[must_use]
    pub fn index_at_offset(&mut self, offset: M::Scalar) -> usize {
        self.model.index_at_offset(offset)
    }

    /// Like [`VirtualList::index_at_offset`], but returns `None` when the model is empty.
    ///
    /// Unlike [`VirtualList::model_mut`], this may update model-internal
    /// caches without marking the cached materialized strip dirty.
    #[must_use]
    pub fn try_index_at_offset(&mut self, offset: M::Scalar) -> Option<usize> {
        self.model.try_index_at_offset(offset)
    }

    /// Returns the current scroll offset.
    #[must_use]
    pub const fn scroll_offset(&self) -> M::Scalar {
        self.scroll_offset
    }

    /// Sets the scroll offset.
    pub fn set_scroll_offset(&mut self, offset: M::Scalar) {
        let offset = offset.max(M::Scalar::zero());
        if offset != self.scroll_offset {
            self.scroll_offset = offset;
            self.dirty = true;
        }
    }

    /// Adjusts the scroll offset by `delta`.
    pub fn scroll_by(&mut self, delta: M::Scalar) {
        self.set_scroll_offset(self.scroll_offset + delta);
    }

    /// Returns the current viewport extent.
    #[must_use]
    pub const fn viewport_extent(&self) -> M::Scalar {
        self.viewport_extent
    }

    /// Sets the viewport extent.
    pub fn set_viewport_extent(&mut self, extent: M::Scalar) {
        let extent = extent.max(M::Scalar::zero());
        if extent != self.viewport_extent {
            self.viewport_extent = extent;
            self.dirty = true;
        }
    }

    /// Sets the overscan extents applied before and after the viewport.
    pub fn set_overscan(&mut self, overscan_before: M::Scalar, overscan_after: M::Scalar) {
        let before = overscan_before.max(M::Scalar::zero());
        let after = overscan_after.max(M::Scalar::zero());
        if before != self.overscan_before || after != self.overscan_after {
            self.overscan_before = before;
            self.overscan_after = after;
            self.dirty = true;
        }
    }

    /// Returns the overscan extent applied before the viewport.
    #[must_use]
    pub const fn overscan_before(&self) -> M::Scalar {
        self.overscan_before
    }

    /// Returns the overscan extent applied after the viewport.
    #[must_use]
    pub const fn overscan_after(&self) -> M::Scalar {
        self.overscan_after
    }

    /// Computes or returns the cached materialized strip.
    ///
    /// This strip includes the configured overscan before and after the viewport.
    #[must_use]
    pub fn visible_strip(&mut self) -> VisibleStrip<M::Scalar> {
        if self.dirty {
            self.last_strip = compute_visible_strip(
                &mut self.model,
                self.scroll_offset,
                self.viewport_extent,
                self.overscan_before,
                self.overscan_after,
            );
            self.dirty = false;
        }
        self.last_strip
    }

    /// Returns the half-open materialized index range, including overscan.
    ///
    /// This is equivalent to `self.visible_strip().range()`.
    #[must_use]
    pub fn visible_range(&mut self) -> Range<usize> {
        self.visible_strip().range()
    }

    /// Computes the viewport strip without overscan.
    ///
    /// Use this when host code needs the indices that overlap the viewport
    /// itself, rather than the larger materialized range. The returned strip
    /// uses the same half-open `[start, end)` convention as
    /// [`VirtualList::visible_strip`].
    #[must_use]
    pub fn viewport_strip(&mut self) -> VisibleStrip<M::Scalar> {
        compute_visible_strip(
            &mut self.model,
            self.scroll_offset,
            self.viewport_extent,
            M::Scalar::zero(),
            M::Scalar::zero(),
        )
    }

    /// Returns the half-open index range that overlaps the viewport.
    ///
    /// This excludes overscan and includes partially visible items. Empty
    /// viewports are represented as `idx..idx`, matching Rust collection APIs.
    #[must_use]
    pub fn viewport_range(&mut self) -> Range<usize> {
        self.viewport_strip().range()
    }

    /// Convenience iterator over materialized indices, including overscan.
    pub fn visible_indices(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = usize> + ExactSizeIterator + use<M> {
        self.visible_range()
    }

    /// Returns the first visible index, if any.
    #[must_use]
    pub fn first_visible_index(&mut self) -> Option<usize> {
        let strip = self.visible_strip();
        if strip.is_empty() {
            None
        } else {
            Some(strip.start)
        }
    }

    /// Returns the last visible index, if any.
    #[must_use]
    pub fn last_visible_index(&mut self) -> Option<usize> {
        let strip = self.visible_strip();
        if strip.is_empty() {
            None
        } else {
            Some(strip.end - 1)
        }
    }

    /// Returns `true` if the given index is fully visible within the viewport.
    #[must_use]
    pub fn is_index_fully_visible(&mut self, index: usize) -> bool {
        let len = self.model.len();
        if index >= len {
            return false;
        }
        let item_start = self.model.offset_of(index);
        let item_end = item_start + self.model.extent_of(index);
        let view_start = self.scroll_offset;
        let view_end = self.scroll_offset + self.viewport_extent;
        item_start >= view_start && item_end <= view_end
    }

    /// Returns `true` if the given index overlaps the viewport at all.
    #[must_use]
    pub fn is_index_partially_visible(&mut self, index: usize) -> bool {
        let len = self.model.len();
        if index >= len {
            return false;
        }
        let item_start = self.model.offset_of(index);
        let item_end = item_start + self.model.extent_of(index);
        let view_start = self.scroll_offset;
        let view_end = self.scroll_offset + self.viewport_extent;
        item_end > view_start && item_start < view_end
    }

    /// Clamps the current scroll offset so that the viewport stays within the content extent.
    ///
    /// This is useful for hosts that want to hard-cap scrolling at the start/end of content.
    pub fn clamp_scroll_to_content(&mut self) {
        let strip = self.visible_strip();
        let content = strip.content_extent;
        let max_offset = if content > self.viewport_extent {
            content - self.viewport_extent
        } else {
            M::Scalar::zero()
        };
        let clamped = if self.scroll_offset > max_offset {
            max_offset
        } else {
            self.scroll_offset
        };
        self.set_scroll_offset(clamped);
    }

    /// Computes the scroll offset needed to bring item `index` into view using
    /// the given alignment, without actually scrolling.
    ///
    /// Returns `None` when the model is empty.
    ///
    /// This is useful for animated/smooth scrolling: query the target, then
    /// interpolate toward it over time with [`set_scroll_offset`](Self::set_scroll_offset).
    #[must_use]
    pub fn target_offset_for_index(
        &mut self,
        index: usize,
        align: ScrollAlign,
    ) -> Option<M::Scalar> {
        let len = self.model.len();
        if len == 0 {
            return None;
        }
        let idx = index.min(len.saturating_sub(1));
        let offset = self.model.offset_of(idx);
        let extent = self.model.extent_of(idx);
        let item_start = offset;
        let item_end = item_start + extent;
        let viewport = self.viewport_extent;

        let new_offset = match align {
            ScrollAlign::Start => item_start,
            ScrollAlign::End => (item_end - viewport).max(M::Scalar::zero()),
            ScrollAlign::Center => {
                let half = M::Scalar::from_usize(2);
                ((item_start + item_end) / half - viewport / half).max(M::Scalar::zero())
            }
            ScrollAlign::Nearest => {
                let current = self.scroll_offset;
                let viewport_start = current;
                let viewport_end = current + viewport;

                // If the item is already fully visible, keep the current offset.
                if item_start >= viewport_start && item_end <= viewport_end {
                    current
                } else if item_start < viewport_start {
                    // Item is above the viewport: align start.
                    item_start
                } else {
                    // Item is below the viewport: align end.
                    (item_end - viewport).max(M::Scalar::zero())
                }
            }
        };

        Some(new_offset)
    }

    /// Scrolls so that item `index` is brought into view using the given alignment.
    ///
    /// Uses [`target_offset_for_index`](Self::target_offset_for_index) to compute the
    /// destination offset, then applies it via [`set_scroll_offset`](Self::set_scroll_offset).
    ///
    /// - [`ScrollAlign::Start`] aligns the start of the item with the start of the viewport.
    /// - [`ScrollAlign::End`] aligns the end of the item with the end of the viewport.
    /// - [`ScrollAlign::Center`] centers the item within the viewport.
    /// - [`ScrollAlign::Nearest`] moves just enough to make the item fully visible, preferring
    ///   the smallest change from the current scroll offset.
    pub fn scroll_to_index(&mut self, index: usize, align: ScrollAlign) {
        if let Some(offset) = self.target_offset_for_index(index, align) {
            self.set_scroll_offset(offset);
        } else {
            self.set_scroll_offset(M::Scalar::zero());
        }
    }
}

impl<M: ResizableExtentModel> VirtualList<M> {
    /// Sets the logical length of the underlying model and invalidates the cached strip.
    ///
    /// This is a convenience wrapper for hosts whose backing collection length
    /// changes, avoiding a broad [`VirtualList::model_mut`] borrow when only
    /// the model length needs to change.
    pub fn set_len(&mut self, len: usize) {
        self.model.set_len(len);
        self.dirty = true;
    }
}

impl<M: ExtentModel> VirtualList<TailAnchoredExtentModel<M>> {
    /// Returns `true` if the current scroll offset is considered anchored to the tail.
    ///
    /// This delegates to [`TailAnchoredExtentModel::is_at_tail`], using the
    /// current [`VirtualList::scroll_offset`] and [`VirtualList::viewport_extent`].
    #[must_use]
    pub fn is_at_tail(&mut self) -> bool {
        let viewport = self.viewport_extent;
        let offset = self.scroll_offset;
        self.model.is_at_tail(offset, viewport)
    }

    /// Scrolls so that the tail of the content is visible at the end of the viewport.
    ///
    /// This delegates to [`TailAnchoredExtentModel::tail_scroll_offset`] and sets
    /// the scroll offset accordingly.
    pub fn scroll_to_tail(&mut self) {
        let viewport = self.viewport_extent;
        let tail = self.model.tail_scroll_offset(viewport);
        self.set_scroll_offset(tail);
    }

    /// Convenience helper for applying a tail-anchor state captured before
    /// mutating the model.
    ///
    /// Call [`VirtualList::is_at_tail`] before appending items or changing
    /// extents, update the model, then pass the captured value here. This is
    /// equivalent to calling [`VirtualList::scroll_to_tail`] when
    /// `was_at_tail` is `true`.
    pub fn restore_tail_anchor(&mut self, was_at_tail: bool) {
        if was_at_tail {
            self.scroll_to_tail();
        }
    }

    /// If the list is currently anchored to the tail, re-aligns the scroll
    /// offset to the current tail position. Otherwise, leaves the scroll offset
    /// unchanged.
    ///
    /// This checks anchoring against the current model state, which is wrong
    /// for the common append/update path: after the model changes, a scroll
    /// offset that was previously anchored may no longer be near the new tail.
    /// Capture [`VirtualList::is_at_tail`] before the mutation and call
    /// [`VirtualList::restore_tail_anchor`] after the mutation instead.
    #[deprecated(
        since = "0.1.1",
        note = "checks anchoring after the model has changed; capture is_at_tail() before mutation and call restore_tail_anchor() afterward"
    )]
    pub fn stick_to_tail_if_anchored(&mut self) {
        if self.is_at_tail() {
            self.scroll_to_tail();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollAlign;
    use crate::{
        ExtentModel, FixedExtentModel, GridTrackModel, TailAnchoredExtentModel, VirtualList,
    };

    #[derive(Debug, Clone)]
    struct CountingModel {
        len: usize,
        extent: f32,
        total_extent_calls: usize,
        offset_of_calls: usize,
        extent_of_calls: usize,
        index_at_offset_calls: usize,
    }

    impl CountingModel {
        fn new(len: usize, extent: f32) -> Self {
            Self {
                len,
                extent,
                total_extent_calls: 0,
                offset_of_calls: 0,
                extent_of_calls: 0,
                index_at_offset_calls: 0,
            }
        }
    }

    impl ExtentModel for CountingModel {
        type Scalar = f32;

        fn len(&self) -> usize {
            self.len
        }

        fn total_extent(&mut self) -> Self::Scalar {
            self.total_extent_calls += 1;
            self.len as f32 * self.extent
        }

        fn extent_of(&mut self, index: usize) -> Self::Scalar {
            self.extent_of_calls += 1;
            if index < self.len { self.extent } else { 0.0 }
        }

        fn offset_of(&mut self, index: usize) -> Self::Scalar {
            self.offset_of_calls += 1;
            index.min(self.len) as f32 * self.extent
        }

        fn index_at_offset(&mut self, offset: Self::Scalar) -> usize {
            self.index_at_offset_calls += 1;
            if self.len == 0 {
                return 0;
            }

            let mut item_offset = 0.0;
            for index in 0..self.len {
                if item_offset + self.extent > offset {
                    return index;
                }
                item_offset += self.extent;
            }
            self.len - 1
        }
    }

    #[test]
    fn visible_strip_tracks_scroll_and_viewport() {
        let model = FixedExtentModel::new(100, 10.0_f32);
        let mut list = VirtualList::new(model, 50.0, 0.0);

        // At top: items 0..5.
        let strip = list.visible_strip();
        assert_eq!(strip.start, 0);
        assert_eq!(strip.end, 5);

        // Scroll down by 10 units: items 1..6.
        list.scroll_by(10.0);
        let strip = list.visible_strip();
        assert_eq!(strip.start, 1);
        assert_eq!(strip.end, 6);
        assert_eq!(list.first_visible_index(), Some(1));
        assert_eq!(list.last_visible_index(), Some(5));
    }

    #[test]
    fn range_helpers_use_half_open_ranges() {
        let model = FixedExtentModel::new(10, 10.0_f32);
        let mut list = VirtualList::new(model, 20.0_f32, 10.0_f32);
        list.set_scroll_offset(10.0_f32);

        assert_eq!(list.visible_range(), 0..4);
        assert_eq!(
            list.visible_indices().collect::<alloc::vec::Vec<_>>(),
            alloc::vec![0, 1, 2, 3]
        );
        assert_eq!(list.viewport_range(), 1..3);

        let viewport = list.viewport_strip();
        assert_eq!(viewport.range(), 1..3);
        assert_eq!(viewport.start, 1);
        assert_eq!(viewport.end, 3);
    }

    #[test]
    fn viewport_range_handles_empty_and_single_item_ranges() {
        let model = FixedExtentModel::new(5, 10.0_f32);
        let mut list = VirtualList::new(model, 10.0_f32, 0.0);

        list.set_scroll_offset(10.0_f32);
        assert_eq!(list.viewport_range(), 1..2);

        list.set_viewport_extent(0.0_f32);
        list.set_scroll_offset(15.0_f32);
        assert_eq!(list.viewport_range(), 1..1);

        list.set_scroll_offset(50.0_f32);
        assert_eq!(list.viewport_range(), 5..5);
    }

    #[test]
    fn scroll_to_index_alignment_behaves_as_expected() {
        let model = FixedExtentModel::new(10, 10.0_f32);
        let mut list = VirtualList::new(model, 30.0, 0.0);

        // Start alignment: item 3 at top → offset 30.
        list.scroll_to_index(3, ScrollAlign::Start);
        assert!((list.scroll_offset() - 30.0_f32).abs() < 1e-5);

        // End alignment: item 3 end at viewport end → offset 10 (viewport covers items 1–3).
        list.scroll_to_index(3, ScrollAlign::End);
        assert!((list.scroll_offset() - 10.0_f32).abs() < 1e-5);

        // Center alignment: item 3 centered in viewport → offset 20.
        list.scroll_to_index(3, ScrollAlign::Center);
        assert!((list.scroll_offset() - 20.0_f32).abs() < 1e-5);

        // Nearest alignment: if already fully visible, should not move.
        let before = list.scroll_offset();
        list.scroll_to_index(3, ScrollAlign::Nearest);
        assert!((list.scroll_offset() - before).abs() < 1e-5);
    }

    #[test]
    fn overscan_accessors_and_clamp_scroll_behave() {
        // 5 items * 10 = 50 content, viewport = 30 → max offset = 20.
        let model = FixedExtentModel::new(5, 10.0_f32);
        let mut list = VirtualList::new(model, 30.0, 5.0);

        // set_overscan updates both before/after.
        assert_eq!(list.overscan_before(), 5.0_f32);
        assert_eq!(list.overscan_after(), 5.0_f32);
        list.set_overscan(8.0_f32, 3.0_f32);
        assert_eq!(list.overscan_before(), 8.0_f32);
        assert_eq!(list.overscan_after(), 3.0_f32);

        // Clamp scroll so viewport stays within content.
        list.set_scroll_offset(100.0_f32);
        list.clamp_scroll_to_content();
        assert!((list.scroll_offset() - 20.0_f32).abs() < 1e-5);

        // When content fits inside viewport, clamp to 0.
        let model = FixedExtentModel::new(2, 10.0_f32);
        let mut list = VirtualList::new(model, 30.0, 0.0);
        list.set_scroll_offset(10.0_f32);
        list.clamp_scroll_to_content();
        assert!((list.scroll_offset() - 0.0_f32).abs() < 1e-5);
    }

    #[test]
    fn visibility_queries_for_indices() {
        // 10 items * 10, viewport = 30 → three items visible at a time.
        let model = FixedExtentModel::new(10, 10.0_f32);
        let mut list = VirtualList::new(model, 30.0, 0.0);

        // At top: items 0,1,2 fully visible.
        assert!(list.is_index_fully_visible(0));
        assert!(list.is_index_fully_visible(2));
        assert!(!list.is_index_fully_visible(3));
        assert!(list.is_index_partially_visible(2));
        assert!(!list.is_index_partially_visible(5));

        // Scroll down by 5: item 0 no longer visible, item 3 partially visible.
        list.scroll_by(5.0_f32);
        assert!(list.is_index_partially_visible(0));
        assert!(list.is_index_partially_visible(3));
    }

    #[test]
    fn set_len_resizes_model_and_invalidates_cached_strip() {
        let model = FixedExtentModel::new(2, 10.0_f32);
        let mut list = VirtualList::new(model, 30.0_f32, 0.0);

        assert_eq!(list.visible_range(), 0..2);

        list.set_len(5);
        assert_eq!(list.len(), 5);
        assert!(!list.is_empty());
        assert_eq!(list.visible_range(), 0..3);

        list.set_len(0);
        assert!(list.is_empty());
        assert_eq!(list.visible_range(), 0..0);
    }

    #[test]
    fn set_len_works_through_tail_anchored_models() {
        let inner = FixedExtentModel::new(2, 10.0_f32);
        let ta = TailAnchoredExtentModel::with_default_epsilon(inner);
        let mut list = VirtualList::new(ta, 30.0_f32, 0.0);

        list.set_len(5);

        assert_eq!(list.len(), 5);
        assert_eq!(list.visible_range(), 0..3);
    }

    #[test]
    fn model_query_wrappers_do_not_dirty_cached_strip() {
        let model = CountingModel::new(5, 10.0_f32);
        let mut list = VirtualList::new(model, 30.0_f32, 0.0);

        assert_eq!(list.visible_range(), 0..3);
        let total_extent_calls = list.model().total_extent_calls;

        assert_eq!(list.total_extent(), 50.0_f32);
        assert_eq!(list.offset_of(2), 20.0_f32);
        assert_eq!(list.extent_of(2), 10.0_f32);
        assert_eq!(list.index_at_offset(25.0_f32), 2);
        assert_eq!(list.try_index_at_offset(45.0_f32), Some(4));

        assert_eq!(list.visible_range(), 0..3);
        assert_eq!(list.model().total_extent_calls, total_extent_calls + 1);
    }

    #[test]
    fn grid_virtual_list_covers_all_cells_in_visible_tracks() {
        // 1000 cells, 3 cells per track, enough tracks to cover all cells.
        let total_cells: usize = 1000;
        let cells_per_track = core::num::NonZeroUsize::new(3).unwrap();

        // Each track is 10 units tall. The grid adapter will resize the
        // underlying track model based on `len` and `cells_per_track`.
        let row_model = FixedExtentModel::new(0, 10.0_f32);
        let grid_model = GridTrackModel::new(row_model, cells_per_track, total_cells);

        // Viewport is 3 tracks tall → 3 * 10.
        let mut list = VirtualList::new(grid_model, 30.0_f32, 0.0);

        let strip = list.visible_strip();
        // At scroll_offset = 0, we expect the first three tracks to be visible:
        // 3 tracks × 3 cells per track = 9 cells (indices 0..9).
        assert_eq!(strip.start, 0);
        assert_eq!(strip.end, 9);
    }

    #[test]
    fn target_offset_for_index_returns_none_for_empty_model() {
        let model = FixedExtentModel::new(0, 10.0_f32);
        let mut list = VirtualList::new(model, 30.0, 0.0);

        assert_eq!(list.target_offset_for_index(0, ScrollAlign::Start), None);
        assert_eq!(list.target_offset_for_index(5, ScrollAlign::Center), None);
        assert_eq!(list.target_offset_for_index(0, ScrollAlign::Nearest), None);
    }

    #[test]
    fn tail_anchored_helpers_scroll_and_detect_tail() {
        // 10 items * 10 = 100 total extent, viewport = 30 → tail offset = 70.
        let inner = FixedExtentModel::new(10, 10.0_f32);
        let ta = TailAnchoredExtentModel::with_default_epsilon(inner);
        let mut list = VirtualList::new(ta, 30.0_f32, 0.0);

        // Initially at top, not at tail.
        assert!(!list.is_at_tail());

        // Scroll directly to tail and verify helpers agree.
        list.scroll_to_tail();
        assert!((list.scroll_offset() - 70.0_f32).abs() < 1e-5);
        assert!(list.is_at_tail());

        // Move slightly away from tail but within epsilon; stick_to_tail_if_anchored
        // should pull us back to the exact tail offset.
        list.set_scroll_offset(69.5_f32);
        assert!(list.is_at_tail());
        #[allow(
            deprecated,
            reason = "verify compatibility behavior for deprecated method"
        )]
        list.stick_to_tail_if_anchored();
        assert!((list.scroll_offset() - 70.0_f32).abs() < 1e-5);

        // Move far from tail; stick_to_tail_if_anchored should leave offset unchanged.
        list.set_scroll_offset(10.0_f32);
        assert!(!list.is_at_tail());
        #[allow(
            deprecated,
            reason = "verify compatibility behavior for deprecated method"
        )]
        list.stick_to_tail_if_anchored();
        assert!((list.scroll_offset() - 10.0_f32).abs() < 1e-5);
    }

    #[test]
    fn restore_tail_anchor_uses_state_captured_before_append() {
        let inner = FixedExtentModel::new(10, 10.0_f32);
        let ta = TailAnchoredExtentModel::with_default_epsilon(inner);
        let mut list = VirtualList::new(ta, 30.0_f32, 0.0);

        list.scroll_to_tail();
        assert!((list.scroll_offset() - 70.0_f32).abs() < 1e-5);
        let was_at_tail = list.is_at_tail();

        list.set_len(11);
        list.restore_tail_anchor(was_at_tail);

        assert!((list.scroll_offset() - 80.0_f32).abs() < 1e-5);
        assert!(list.is_at_tail());
    }

    #[test]
    fn restore_tail_anchor_keeps_scroll_when_not_previously_anchored() {
        let inner = FixedExtentModel::new(10, 10.0_f32);
        let ta = TailAnchoredExtentModel::with_default_epsilon(inner);
        let mut list = VirtualList::new(ta, 30.0_f32, 0.0);

        list.set_scroll_offset(10.0_f32);
        let was_at_tail = list.is_at_tail();

        list.set_len(11);
        list.restore_tail_anchor(was_at_tail);

        assert!((list.scroll_offset() - 10.0_f32).abs() < 1e-5);
        assert!(!list.is_at_tail());
    }
}
