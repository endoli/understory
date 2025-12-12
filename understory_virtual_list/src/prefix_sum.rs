// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An [`ExtentModel`] backed by per-item extents and a lazily-maintained prefix-sum cache.

use alloc::vec::Vec;

use crate::{ExtentModel, ResizableExtentModel, Scalar};

/// An [`ExtentModel`] backed by per-item extents and a lazily-maintained prefix-sum cache.
///
/// This is suitable for lists with non-uniform item sizes and incremental measurement:
/// callers can start with rough estimates and update extents as real layout information
/// becomes available. Host code is responsible for calling [`PrefixSumExtentModel::set_extent`]
/// when it learns an item's extent (for example, after layout), or using
/// [`PrefixSumExtentModel::rebuild`] as a convenience when recomputing all extents.
#[derive(Clone, Default, Debug)]
pub struct PrefixSumExtentModel<S: Scalar> {
    extents: Vec<S>,
    prefix_starts: Vec<S>,
    dirty_from: Option<usize>,
}

impl<S: Scalar> PrefixSumExtentModel<S> {
    /// Creates an empty model.
    #[must_use]
    pub fn new() -> Self {
        Self {
            extents: Vec::new(),
            prefix_starts: Vec::new(),
            dirty_from: Some(0),
        }
    }

    /// Rebuilds the extents from a sequence of items and a size function.
    ///
    /// This is a convenience for hosts that already iterate their items to
    /// compute sizes. Any previous extents are discarded.
    pub fn rebuild<T, I>(&mut self, items: I, size_fn: &dyn Fn(&T) -> S)
    where
        I: IntoIterator<Item = T>,
    {
        self.extents.clear();
        self.prefix_starts.clear();
        self.dirty_from = Some(0);

        for item in items {
            let mut extent = size_fn(&item);
            debug_assert!(
                extent.is_finite(),
                "PrefixSumExtentModel extents must be finite; got {extent:?}"
            );
            if extent.is_sign_negative() {
                extent = S::zero();
            }
            self.extents.push(extent);
        }
        if self.prefix_starts.len() < self.extents.len() {
            self.prefix_starts.resize(self.extents.len(), S::zero());
        }
    }

    /// Ensures storage for `len` items. Newly added items receive extent `0.0`.
    pub fn set_len(&mut self, len: usize) {
        self.extents.resize(len, S::zero());
        if self.prefix_starts.len() < len {
            self.prefix_starts.resize(len, S::zero());
        }
        self.dirty_from = Some(self.dirty_from.unwrap_or(0).min(len));
    }

    /// Updates the extent of a single item and marks prefix sums dirty from this index.
    pub fn set_extent(&mut self, index: usize, extent: S) {
        if index >= self.extents.len() {
            self.set_len(index + 1);
        }
        // Extents are expected to be finite. Catch NaNs (and infinities) in
        // debug builds so misuse does not go unnoticed.
        debug_assert!(
            extent.is_finite(),
            "PrefixSumExtentModel extents must be finite; got {extent:?}"
        );
        // Clamp finite negative values to `0.0`.
        self.extents[index] = if extent.is_sign_negative() {
            S::zero()
        } else {
            extent
        };
        self.dirty_from = Some(self.dirty_from.unwrap_or(index).min(index));
    }

    fn ensure_prefix_through(&mut self, through: usize) {
        let len = self.extents.len();
        if len == 0 || through >= len {
            return;
        }

        let dirty_from = match self.dirty_from {
            Some(d) if d <= through => d,
            _ => return,
        };

        let mut pos = if dirty_from == 0 {
            S::zero()
        } else {
            self.prefix_starts[dirty_from - 1] + self.extents[dirty_from - 1]
        };

        for i in dirty_from..len {
            self.prefix_starts[i] = pos;
            pos = pos + self.extents[i];
        }

        if through >= len.saturating_sub(1) {
            self.dirty_from = None;
        } else {
            self.dirty_from = Some(through + 1);
        }
    }

    fn offset_at_inner(&mut self, index: usize) -> S {
        if index == 0 || self.extents.is_empty() {
            return S::zero();
        }
        let i = index.min(self.extents.len().saturating_sub(1));
        self.ensure_prefix_through(i);
        self.prefix_starts.get(i).copied().unwrap_or_else(S::zero)
    }

    fn extent_at_inner(&self, index: usize) -> S {
        self.extents.get(index).copied().unwrap_or_else(S::zero)
    }

    fn total_extent_inner(&mut self) -> S {
        let len = self.extents.len();
        if len == 0 {
            return S::zero();
        }
        let last = len - 1;
        self.ensure_prefix_through(last);
        self.offset_at_inner(last) + self.extent_at_inner(last)
    }

    /// Returns the offset of `index` from the start of the strip.
    ///
    /// This is a convenience wrapper around the internal prefix-sum cache and
    /// is useful when callers want direct access to offsets for a specific item.
    pub fn offset_at(&mut self, index: usize) -> S {
        self.offset_at_inner(index)
    }

    /// Returns the extent of `index`.
    ///
    /// This is a convenience wrapper for callers that need extents without going
    /// through the [`ExtentModel`] trait.
    pub fn extent_at(&self, index: usize) -> S {
        self.extent_at_inner(index)
    }

    /// Returns the total extent for the first `len` items.
    ///
    /// If `len` exceeds the current number of extents, it is clamped.
    pub fn total_extent_for_len(&mut self, len: usize) -> S {
        let len = len.min(self.extents.len());
        if len == 0 {
            return S::zero();
        }
        let last = len - 1;
        self.ensure_prefix_through(last);
        self.offset_at_inner(last) + self.extent_at_inner(last)
    }

    /// Returns an index for `offset` within the first `len` items.
    ///
    /// This is useful for hosts that want to constrain queries to a known
    /// prefix of the data.
    pub fn index_at_offset_for_len(&mut self, offset: S, len: usize) -> usize {
        let len = len.min(self.extents.len());
        if len == 0 {
            return 0;
        }

        self.ensure_prefix_through(len.saturating_sub(1));

        let target = offset.max(S::zero());
        let slice = &self.prefix_starts[..len];

        match slice.binary_search_by(|pos| {
            pos.partial_cmp(&target)
                .unwrap_or(core::cmp::Ordering::Equal)
        }) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }
}

impl<S: Scalar> ExtentModel for PrefixSumExtentModel<S> {
    type Scalar = S;

    fn len(&self) -> usize {
        self.extents.len()
    }

    fn total_extent(&mut self) -> S {
        self.total_extent_inner()
    }

    fn extent_of(&mut self, index: usize) -> S {
        self.extent_at_inner(index)
    }

    fn offset_of(&mut self, index: usize) -> S {
        self.offset_at_inner(index)
    }

    fn index_at_offset(&mut self, offset: S) -> usize {
        let len = self.extents.len();
        self.index_at_offset_for_len(offset, len)
    }
}

impl<S: Scalar> ResizableExtentModel for PrefixSumExtentModel<S> {
    fn set_len(&mut self, len: usize) {
        self.set_len(len);
    }
}

#[cfg(test)]
mod tests {
    use super::{ExtentModel, PrefixSumExtentModel};

    #[test]
    fn grows_and_reports_extents() {
        let mut model = PrefixSumExtentModel::<f32>::new();
        model.set_len(3);
        model.set_extent(0, 10.0);
        model.set_extent(1, 20.0);
        model.set_extent(2, 30.0);

        assert_eq!(model.len(), 3);
        assert_eq!(model.total_extent(), 60.0);
        assert_eq!(model.offset_of(0), 0.0);
        assert_eq!(model.offset_of(1), 10.0);
        assert_eq!(model.offset_of(2), 30.0);
        assert_eq!(model.extent_of(1), 20.0);
    }

    #[test]
    fn index_lookup_uses_prefix_sums() {
        let mut model = PrefixSumExtentModel::<f32>::new();
        model.set_len(3);
        model.set_extent(0, 10.0);
        model.set_extent(1, 10.0);
        model.set_extent(2, 10.0);

        assert_eq!(model.index_at_offset(0.0), 0);
        assert_eq!(model.index_at_offset(5.0), 0);
        assert_eq!(model.index_at_offset(10.0), 1);
        assert_eq!(model.index_at_offset(25.0), 2);
        assert_eq!(model.index_at_offset(100.0), 2);
    }

    #[test]
    fn negative_extents_are_clamped_to_zero() {
        let mut model = PrefixSumExtentModel::<f32>::new();
        model.set_len(2);

        // Negative inputs are clamped to 0.
        model.set_extent(0, -5.0);
        assert_eq!(model.extent_of(0), 0.0);
    }

    #[test]
    fn rebuild_and_helpers_match_expectations() {
        let mut model = PrefixSumExtentModel::<f32>::new();
        let items = [10_u32, 20, 30];
        model.rebuild(items, &|v| *v as f32);

        // Offsets and extents
        assert_eq!(model.offset_at(0), 0.0);
        assert_eq!(model.offset_at(1), 10.0);
        assert_eq!(model.extent_at(1), 20.0);

        // Total extent for first two items
        assert_eq!(model.total_extent_for_len(2), 30.0);

        // Index lookup bounded by len
        assert_eq!(model.index_at_offset_for_len(0.0, 3), 0);
        assert_eq!(model.index_at_offset_for_len(15.0, 3), 1);
        assert_eq!(model.index_at_offset_for_len(40.0, 3), 2);
    }
}
