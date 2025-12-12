// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An [`ExtentModel`] adapter that maps a per-track model onto per-cell indices.
//!
//! This is useful for grid-like layouts where the scroll axis operates over
//! *tracks* (for example, rows in a vertical grid or columns in a horizontal
//! grid) while the backing data is indexed as a flat sequence of *cells*.
//!
//! A [`GridTrackModel`] wraps another [`ExtentModel`] that describes per-track
//! extents in the scroll direction and exposes an [`ExtentModel`] view over a
//! dense strip of `0..len` cells:
//!
//! - Each track contains a fixed number of cells (`cells_per_track`).
//! - The extent and offset of a cell are those of its containing track.
//! - [`ExtentModel::index_at_offset`] resolves to the first cell in the track
//!   whose start is at or before the given offset.
//!
//! Hosts are expected to interpret track/cell as either row/column or
//! column/row depending on scroll direction.

use core::num::NonZeroUsize;

use crate::{ExtentModel, ResizableExtentModel, Scalar};

/// Adapts a per-track [`ExtentModel`] into a per-cell model for grid layouts.
#[derive(Debug, Clone)]
pub struct GridTrackModel<M: ResizableExtentModel> {
    track_model: M,
    cells_per_track: NonZeroUsize,
    len: usize,
}

impl<M: ResizableExtentModel> GridTrackModel<M> {
    /// Creates a new [`GridTrackModel`].
    ///
    /// - `track_model` describes the extent and offset of each track.
    /// - `cells_per_track` is the number of cells in each track (must be > 0).
    /// - `len` is the total number of cells in the flattened grid.
    ///
    /// Tracks are consumed in order, so the number of logical tracks is
    /// `tracks = ceil(len / cells_per_track)`. Any trailing cells in the last
    /// (partially filled) track share that track's extent.
    ///
    /// `cells_per_track` must be non-zero.
    #[must_use]
    pub fn new(track_model: M, cells_per_track: NonZeroUsize, len: usize) -> Self {
        let mut track_model = track_model;
        let track_count = if len == 0 {
            0
        } else {
            len.div_ceil(cells_per_track.get())
        };
        track_model.set_len(track_count);

        Self {
            track_model,
            cells_per_track,
            len,
        }
    }

    /// Returns a shared reference to the underlying track model.
    #[must_use]
    pub fn track_model(&self) -> &M {
        &self.track_model
    }

    /// Returns a mutable reference to the underlying track model.
    pub fn track_model_mut(&mut self) -> &mut M {
        &mut self.track_model
    }

    /// Returns the number of cells per track.
    #[must_use]
    pub const fn cells_per_track(&self) -> usize {
        self.cells_per_track.get()
    }

    /// Sets the number of cells per track.
    ///
    /// This does not modify the underlying track model; it only affects how
    /// the flat cell indices are mapped onto tracks.
    ///
    /// `cells_per_track` must be non-zero.
    pub fn set_cells_per_track(&mut self, cells_per_track: NonZeroUsize) {
        self.cells_per_track = cells_per_track;
        let track_count = self.track_count();
        self.track_model.set_len(track_count);
    }

    /// Returns the total number of cells in the grid.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Sets the total number of cells in the grid.
    ///
    /// This does not modify the underlying track model; callers are expected
    /// to keep the number of tracks in the model consistent with their usage.
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        let track_count = self.track_count();
        self.track_model.set_len(track_count);
    }

    /// Returns `true` if there are no cells in this grid.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the track index containing `cell_index`.
    ///
    /// The result is `cell_index / cells_per_track`.
    #[must_use]
    pub const fn track_of_cell(&self, cell_index: usize) -> usize {
        cell_index / self.cells_per_track.get()
    }

    /// Returns the zero-based position of `cell_index` within its track.
    ///
    /// The result is `cell_index % cells_per_track`.
    #[must_use]
    pub const fn cell_in_track(&self, cell_index: usize) -> usize {
        cell_index % self.cells_per_track.get()
    }

    /// Returns the number of tracks needed to represent `len` cells.
    ///
    /// This is `ceil(len / cells_per_track)` and may exceed the backing track
    /// count if the underlying model is not sized consistently.
    #[must_use]
    pub const fn track_count(&self) -> usize {
        if self.len == 0 {
            return 0;
        }
        self.len.div_ceil(self.cells_per_track.get())
    }
}

impl<M: ResizableExtentModel> ExtentModel for GridTrackModel<M> {
    type Scalar = M::Scalar;

    fn len(&self) -> usize {
        self.len
    }

    fn total_extent(&mut self) -> Self::Scalar {
        self.track_model.total_extent()
    }

    fn extent_of(&mut self, index: usize) -> Self::Scalar {
        if self.len == 0 {
            return M::Scalar::zero();
        }
        let clamped = index.min(self.len.saturating_sub(1));
        let track = self.track_of_cell(clamped);
        debug_assert!(
            track < self.track_model.len(),
            "GridTrackModel track index out of bounds: track={track}, len={}",
            self.track_model.len()
        );
        self.track_model.extent_of(track)
    }

    fn offset_of(&mut self, index: usize) -> Self::Scalar {
        if self.len == 0 {
            return M::Scalar::zero();
        }
        let clamped = index.min(self.len.saturating_sub(1));
        let track = self.track_of_cell(clamped);
        debug_assert!(
            track < self.track_model.len(),
            "GridTrackModel track index out of bounds: track={track}, len={}",
            self.track_model.len()
        );
        self.track_model.offset_of(track)
    }

    fn index_at_offset(&mut self, offset: Self::Scalar) -> usize {
        if self.len == 0 {
            return 0;
        }
        let track = self.track_model.index_at_offset(offset);
        track
            .saturating_mul(self.cells_per_track.get())
            .min(self.len.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::GridTrackModel;
    use crate::{ExtentModel, FixedExtentModel};
    use core::num::NonZeroUsize;

    #[test]
    fn basic_cell_to_track_mapping() {
        let track_model = FixedExtentModel::new(3, 10.0_f32);
        let grid = GridTrackModel::new(track_model, NonZeroUsize::new(4).unwrap(), 10);

        assert_eq!(grid.track_count(), 3);
        assert_eq!(grid.track_of_cell(0), 0);
        assert_eq!(grid.track_of_cell(3), 0);
        assert_eq!(grid.track_of_cell(4), 1);
        assert_eq!(grid.cell_in_track(4), 0);
        assert_eq!(grid.cell_in_track(7), 3);
    }

    #[test]
    fn extent_and_offsets_come_from_tracks() {
        let track_model = FixedExtentModel::new(3, 10.0_f32);
        let mut grid = GridTrackModel::new(track_model, NonZeroUsize::new(4).unwrap(), 10);

        // Three tracks of extent 10 → total 30.
        assert_eq!(grid.total_extent(), 30.0);

        // All cells in track 0 share offset 0 and extent 10.
        assert_eq!(grid.offset_of(0), 0.0);
        assert_eq!(grid.offset_of(3), 0.0);
        assert_eq!(grid.extent_of(0), 10.0);
        assert_eq!(grid.extent_of(3), 10.0);

        // Cells in track 1: offset 10.
        assert_eq!(grid.offset_of(4), 10.0);
        assert_eq!(grid.offset_of(7), 10.0);

        // Cells in track 2: offset 20 (partial track).
        assert_eq!(grid.offset_of(8), 20.0);
        assert_eq!(grid.offset_of(9), 20.0);
    }

    #[test]
    fn index_at_offset_resolves_to_first_cell_in_track() {
        let track_model = FixedExtentModel::new(3, 10.0_f32);
        let mut grid = GridTrackModel::new(track_model, NonZeroUsize::new(4).unwrap(), 10);

        // Offsets within first track map to first cell (0).
        assert_eq!(grid.index_at_offset(0.0), 0);
        assert_eq!(grid.index_at_offset(5.0), 0);

        // Offsets in second track map to first cell of that track (4).
        assert_eq!(grid.index_at_offset(10.0), 4);
        assert_eq!(grid.index_at_offset(19.9), 4);

        // Offsets in third track map to first cell of that track (8).
        assert_eq!(grid.index_at_offset(20.0), 8);
        assert_eq!(grid.index_at_offset(100.0), 8);
    }
}
