// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A small controller that owns an [`ExtentModel`] and scroll state.

use crate::{ExtentModel, Scalar, VisibleStrip, compute_visible_strip};

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
#[derive(Debug)]
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

    /// Computes or returns the cached visible strip.
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

    /// Convenience iterator over visible indices.
    pub fn visible_indices(&mut self) -> impl Iterator<Item = usize> {
        let strip = self.visible_strip();
        strip.start..strip.end
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

    /// Scrolls so that item `index` is brought into view using the given alignment.
    ///
    /// - [`ScrollAlign::Start`] aligns the start of the item with the start of the viewport.
    /// - [`ScrollAlign::End`] aligns the end of the item with the end of the viewport.
    /// - [`ScrollAlign::Center`] centers the item within the viewport.
    /// - [`ScrollAlign::Nearest`] moves just enough to make the item fully visible, preferring
    ///   the smallest change from the current scroll offset.
    pub fn scroll_to_index(&mut self, index: usize, align: ScrollAlign) {
        let len = self.model.len();
        if len == 0 {
            self.set_scroll_offset(M::Scalar::zero());
            return;
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

        self.set_scroll_offset(new_offset);
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollAlign;
    use crate::{FixedExtentModel, GridTrackModel, VirtualList};

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
    fn scroll_to_index_alignment_behaves_as_expected() {
        let model = FixedExtentModel::new(10, 10.0_f32);
        let mut list = VirtualList::new(model, 30.0, 0.0);

        // Start alignment: item 3 at top → offset 30.
        list.scroll_to_index(3, ScrollAlign::Start);
        assert!((list.scroll_offset() - 30.0_f32).abs() < f32::EPSILON);

        // End alignment: item 3 end at viewport end → offset 10 (viewport covers items 1–3).
        list.scroll_to_index(3, ScrollAlign::End);
        assert!((list.scroll_offset() - 10.0_f32).abs() < f32::EPSILON);

        // Center alignment: item 3 centered in viewport → offset 20.
        list.scroll_to_index(3, ScrollAlign::Center);
        assert!((list.scroll_offset() - 20.0_f32).abs() < f32::EPSILON);

        // Nearest alignment: if already fully visible, should not move.
        let before = list.scroll_offset();
        list.scroll_to_index(3, ScrollAlign::Nearest);
        assert!((list.scroll_offset() - before).abs() < f32::EPSILON);
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
        assert!((list.scroll_offset() - 20.0_f32).abs() < f32::EPSILON);

        // When content fits inside viewport, clamp to 0.
        let model = FixedExtentModel::new(2, 10.0_f32);
        let mut list = VirtualList::new(model, 30.0, 0.0);
        list.set_scroll_offset(10.0_f32);
        list.clamp_scroll_to_content();
        assert!((list.scroll_offset() - 0.0_f32).abs() < f32::EPSILON);
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
}
