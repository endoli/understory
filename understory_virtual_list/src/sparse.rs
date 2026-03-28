// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An [`ExtentModel`] backed by per-item extents and a lazily-maintained prefix-sum cache,
//! with fixed-sized placeholders for un-materialized items.

use alloc::collections::BTreeMap;

use crate::{ExtentModel, Scalar};

/// An [`ExtentModel`] backed by per-item extents and a lazily-maintained prefix-sum cache with
/// fixed-sized placeholders for un-materialized items.
///
/// This is suitable for lists with non-uniform item sizes and incremental measurement:
/// callers can start with a default estimated extent and update extents as real layout information
/// becomes available. Host code is responsible for calling [`SparsePrefixSumExtentModel::set_extent`]
/// when it learns an item's extent (for example, after layout), or using
/// [`SparsePrefixSumExtentModel::rebuild`] as a convenience when recomputing all extents.
#[derive(Clone, Default, Debug)]
pub struct SparsePrefixSumExtentModel<S: Scalar> {
    default_extent: S,
    len: usize,
    extents_and_prefixes: BTreeMap<usize, (S, S)>,
    last_valid: Option<usize>,
}

impl<S: Scalar> SparsePrefixSumExtentModel<S> {
    /// Creates an empty model.
    #[must_use]
    pub fn new(default_extent: S, len: usize) -> Self {
        // Extents are expected to be finite. Catch NaNs (and infinities) in
        // debug builds so misuse does not go unnoticed.
        debug_assert!(
            default_extent.is_finite(),
            "SparsePrefixSumExtentModel extents must be finite; got {default_extent:?}"
        );
        let default_extent = if default_extent.is_sign_negative() {
            S::zero()
        } else {
            default_extent
        };

        Self {
            default_extent,
            len,
            extents_and_prefixes: BTreeMap::new(),
            last_valid: None,
        }
    }

    /// Returns the default extent for un-materialized items.
    pub fn default_extent(&self) -> S {
        self.default_extent
    }

    /// Updates the default extent for un-materialized items.
    pub fn set_default_extent(&mut self, default_extent: S) {
        // Extents are expected to be finite. Catch NaNs (and infinities) in
        // debug builds so misuse does not go unnoticed.
        debug_assert!(
            default_extent.is_finite(),
            "SparsePrefixSumExtentModel extents must be finite; got {default_extent:?}"
        );
        self.default_extent = if default_extent.is_sign_negative() {
            S::zero()
        } else {
            default_extent
        };
    }

    /// Returns true if there is no item.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the total number of items.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Updates the total number of items.
    pub fn set_len(&mut self, len: usize) {
        self.extents_and_prefixes.retain(|i, _| *i < len);
        self.len = len;
    }

    /// Clears all materialized items.
    pub fn clear(&mut self) {
        self.extents_and_prefixes.clear();
    }

    /// Rebuilds the extents from a sequence of items and a size function.
    ///
    /// This is a convenience for hosts that already iterate their items to
    /// compute sizes. Any previous extents are discarded.
    pub fn rebuild<T, I>(&mut self, items: I, size_fn: &dyn Fn(&T) -> S)
    where
        I: IntoIterator<Item = (usize, T)>,
    {
        self.extents_and_prefixes.clear();
        self.last_valid = None;

        for (index, item) in items {
            if index >= self.len {
                continue;
            }

            let mut extent = size_fn(&item);
            debug_assert!(
                extent.is_finite(),
                "SparsePrefixSumExtentModel extents must be finite; got {extent:?}"
            );
            if extent.is_sign_negative() {
                extent = S::zero();
            }
            self.extents_and_prefixes.insert(index, (extent, S::zero()));
        }
    }

    /// Updates the extent of a single item and marks prefix sums dirty from this index.
    pub fn set_extent(&mut self, index: usize, extent: S) {
        if index >= self.len {
            return;
        }

        // Extents are expected to be finite. Catch NaNs (and infinities) in
        // debug builds so misuse does not go unnoticed.
        debug_assert!(
            extent.is_finite(),
            "SparsePrefixSumExtentModel extents must be finite; got {extent:?}"
        );
        // Clamp finite negative values to `0.0`.
        self.extents_and_prefixes.insert(
            index,
            (
                if extent.is_sign_negative() {
                    S::zero()
                } else {
                    extent
                },
                S::zero(),
            ),
        );
        self.last_valid = self
            .last_valid
            .and_then(|last_valid| index.checked_sub(1).map(|index| index.min(last_valid)));
    }

    /// Clears the materialized item at index.
    pub fn clear_extent(&mut self, index: usize) {
        if self.extents_and_prefixes.remove(&index).is_some() {
            self.last_valid = Some(self.last_valid.unwrap_or(index).min(index));
        }
    }

    fn ensure_prefix_through(&mut self, through: usize) {
        let through = through.min(self.len);

        let (start_from, mut last_prefix) = if let Some(last_valid) = self.last_valid {
            if last_valid >= through {
                return;
            };
            (
                last_valid + 1,
                self.extents_and_prefixes
                    .range(..=last_valid)
                    .last()
                    .map_or_else(S::zero, |(_, (_, p))| *p),
            )
        } else {
            (0, S::zero())
        };

        for (_, (extent, prefix)) in self.extents_and_prefixes.range_mut(start_from..=through) {
            last_prefix = last_prefix + *extent;
            *prefix = last_prefix;
        }

        self.last_valid = Some(through);
    }

    /// Returns the offset of `index` from the start of the strip.
    ///
    /// This is a convenience wrapper around the internal prefix-sum cache and
    /// is useful when callers want direct access to offsets for a specific item.
    pub fn offset_at(&mut self, index: usize) -> S {
        if index == 0 || self.len == 0 {
            return S::zero();
        }
        let index = index.min(self.len - 1);

        self.ensure_prefix_through(index);
        let from_default = self.default_extent
            * S::from_usize(index - self.extents_and_prefixes.range(..index).count());
        let last = self.extents_and_prefixes.range(..index).last();
        let from_prefix = last.map_or_else(S::zero, |(_, (_, p))| *p);

        from_default + from_prefix
    }

    /// Returns the extent of `index`.
    ///
    /// This is a convenience wrapper for callers that need extents without going
    /// through the [`ExtentModel`] trait.
    pub fn extent_at(&self, index: usize) -> S {
        if index < self.len {
            self.extents_and_prefixes
                .get(&index)
                .map_or(self.default_extent, |(e, _)| *e)
        } else {
            S::zero()
        }
    }

    /// Returns the total extent for the first `len` items.
    ///
    /// If `len` exceeds the current number of extents, it is clamped.
    pub fn total_extent_for_len(&mut self, len: usize) -> S {
        len.min(self.len)
            .checked_sub(1)
            .map_or_else(S::zero, |last| self.offset_at(last) + self.extent_at(last))
    }

    /// Returns an index for `offset` within the first `len` items.
    ///
    /// This is useful for hosts that want to constrain queries to a known
    /// prefix of the data.
    pub fn index_at_offset_for_len(&mut self, offset: S, len: usize) -> usize {
        let len = len.min(self.len);
        if len == 0 {
            return 0;
        }

        let target = offset.max(S::zero());

        // Find the greatest index whose start offset is <= target.
        let mut lo = 0;
        let mut hi = len - 1;

        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if self.offset_at(mid) <= target {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }

        lo
    }
}

impl<S: Scalar> ExtentModel for SparsePrefixSumExtentModel<S> {
    type Scalar = S;

    fn len(&self) -> usize {
        self.len()
    }

    fn total_extent(&mut self) -> S {
        self.total_extent_for_len(self.len)
    }

    fn extent_of(&mut self, index: usize) -> S {
        self.extent_at(index)
    }

    fn offset_of(&mut self, index: usize) -> S {
        self.offset_at(index)
    }

    fn index_at_offset(&mut self, offset: S) -> usize {
        self.index_at_offset_for_len(offset, self.len)
    }
}

#[cfg(test)]
mod tests {
    use crate::{PrefixSumExtentModel, compute_visible_strip};

    use super::{ExtentModel, SparsePrefixSumExtentModel};

    #[test]
    fn only_default_offsets() {
        let mut model = SparsePrefixSumExtentModel::new(10., 30);

        assert_eq!(model.offset_of(1), 10.);
        assert_eq!(model.offset_of(5), 50.);
        assert_eq!(model.offset_of(9), 90.);
    }

    #[test]
    fn only_default_extents() {
        let mut model = SparsePrefixSumExtentModel::new(10., 30);

        assert_eq!(model.extent_of(1), 10.);
        assert_eq!(model.extent_of(5), 10.);
    }

    #[test]
    fn only_default_indices() {
        let mut model = SparsePrefixSumExtentModel::new(10., 30);

        assert_eq!(model.index_at_offset(15.), 1);
        assert_eq!(model.index_at_offset(56.), 5);
        assert_eq!(model.index_at_offset(89.), 8);
    }

    #[test]
    fn before_set_offsets() {
        let mut model = SparsePrefixSumExtentModel::new(10., 30);
        model.set_extent(11, 25.);
        model.set_extent(17, 45.);

        assert_eq!(model.offset_of(1), 10.);
        assert_eq!(model.offset_of(5), 50.);
        assert_eq!(model.offset_of(9), 90.);
    }

    #[test]
    fn before_set_extents() {
        let mut model = SparsePrefixSumExtentModel::new(10., 30);
        model.set_extent(11, 25.);
        model.set_extent(17, 45.);

        assert_eq!(model.extent_of(1), 10.);
        assert_eq!(model.extent_of(5), 10.);
    }

    #[test]
    fn before_set_indices() {
        let mut model = SparsePrefixSumExtentModel::new(10., 30);
        model.set_extent(11, 25.);
        model.set_extent(17, 45.);

        assert_eq!(model.index_at_offset(15.), 1);
        assert_eq!(model.index_at_offset(56.), 5);
        assert_eq!(model.index_at_offset(89.), 8);
    }

    #[test]
    fn at_and_after_set_offsets() {
        let mut model = SparsePrefixSumExtentModel::new(10., 30);
        model.set_extent(11, 25.);
        model.set_extent(17, 45.);

        assert_eq!(model.offset_of(11), 110.);
        assert_eq!(model.offset_of(12), 135.);
        assert_eq!(model.offset_of(17), 185.);
        assert_eq!(model.offset_of(18), 230.);
    }

    #[test]
    fn at_set_extents() {
        let mut model = SparsePrefixSumExtentModel::new(10., 30);
        model.set_extent(11, 25.);
        model.set_extent(17, 45.);

        assert_eq!(model.extent_of(11), 25.);
        assert_eq!(model.extent_of(17), 45.);
    }

    #[test]
    fn at_and_after_set_indices() {
        let mut model = SparsePrefixSumExtentModel::new(10., 30);
        model.set_extent(11, 25.);
        model.set_extent(17, 45.);

        assert_eq!(model.index_at_offset(125.), 11);
        assert_eq!(model.index_at_offset(139.), 12);
        assert_eq!(model.index_at_offset(222.), 17);
        assert_eq!(model.index_at_offset(234.), 18);
    }

    #[test]
    fn at_and_after_smaller_set_offsets() {
        let mut model = SparsePrefixSumExtentModel::new(20., 30);
        model.set_extent(11, 15.);
        model.set_extent(17, 5.);

        assert_eq!(model.offset_of(11), 220.);
        assert_eq!(model.offset_of(12), 235.);
        assert_eq!(model.offset_of(17), 335.);
        assert_eq!(model.offset_of(18), 340.);
    }

    #[test]
    fn at_and_after_smaller_set_indices() {
        let mut model = SparsePrefixSumExtentModel::new(20., 30);
        model.set_extent(11, 15.);
        model.set_extent(17, 5.);

        assert_eq!(model.index_at_offset(225.), 11);
        assert_eq!(model.index_at_offset(236.), 12);
        assert_eq!(model.index_at_offset(336.), 17);
        assert_eq!(model.index_at_offset(341.), 18);
    }

    #[test]
    fn compare_dense() {
        let mut dense_model = PrefixSumExtentModel::<f64>::new();
        dense_model.set_len(100);
        for i in 0..100 {
            dense_model.set_extent(i, 51.);
        }
        let dense_strip = compute_visible_strip(&mut dense_model, 0., 640., 320., 320.);

        let mut sparse_model = SparsePrefixSumExtentModel::new(51., 100);
        assert_eq!(sparse_model.total_extent(), 5100.);
        for i in 0..6 {
            sparse_model.set_extent(i, 51.);
        }
        assert_eq!(sparse_model.total_extent(), 5100.);
        let sparse_strip = compute_visible_strip(&mut sparse_model, 0., 640., 320., 320.);

        assert_eq!(dense_strip, sparse_strip);
    }

    #[test]
    fn constructor_clamps_negative_default_extent() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(-5.0, 3);

        assert_eq!(model.extent_of(0), 0.0);
        assert_eq!(model.offset_of(1), 0.0);
        assert_eq!(model.total_extent(), 0.0);
    }

    #[test]
    fn total_extent_for_len_clamp() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(10.0, 3);

        assert_eq!(model.total_extent_for_len(0), 0.0);
        assert_eq!(model.total_extent_for_len(1), 10.0);
        assert_eq!(model.total_extent_for_len(usize::MAX), 30.0);
    }

    #[test]
    fn total_extent_for_len_is_not_off_by_one() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(10.0, 30);

        assert_eq!(model.total_extent_for_len(0), 0.0);
        assert_eq!(model.total_extent_for_len(1), 10.0);
        assert_eq!(model.total_extent_for_len(5), 50.0);
        assert_eq!(model.total_extent_for_len(30), 300.0);
    }

    #[test]
    fn index_at_offset_clamps_to_last_item() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(10.0, 30);

        assert_eq!(model.index_at_offset(299.0), 29);
        assert_eq!(model.index_at_offset(300.0), 29);
        assert_eq!(model.index_at_offset(1_000.0), 29);
    }

    #[test]
    fn index_at_offset_for_len_clamps_to_last_item_in_prefix() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(10.0, 30);

        assert_eq!(model.index_at_offset_for_len(49.0, 5), 4);
        assert_eq!(model.index_at_offset_for_len(50.0, 5), 4);
        assert_eq!(model.index_at_offset_for_len(1_000.0, 5), 4);
    }

    #[test]
    fn rebuild_initializes_prefixes_from_zero() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(10.0, 3);
        model.rebuild([(0, 25_f32), (1, 15.)], &|v| *v);

        assert_eq!(model.offset_of(0), 0.0);
        assert_eq!(model.offset_of(1), 25.0);
        assert_eq!(model.offset_of(2), 40.0);
        assert_eq!(model.total_extent(), 50.0);
    }

    #[test]
    fn shrink_then_grow_discards_stale_sparse_entries() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(10.0, 10);
        model.set_extent(5, 20.0);

        model.set_len(5);
        model.set_len(6);

        assert_eq!(model.extent_of(5), 10.0);
        assert_eq!(model.total_extent_for_len(6), 60.0);
    }

    #[test]
    fn index_at_offset_for_zero_default_extent() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(0.0, 4);
        model.set_extent(1, 20.0);
        model.set_extent(2, 10.0);

        assert_eq!(model.index_at_offset(10.0), 1);
        assert_eq!(model.index_at_offset(25.0), 2);
        assert_eq!(model.index_at_offset(35.0), 3);
    }

    #[test]
    fn out_of_range_set_extent() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(10.0, 4);

        model.set_extent(5, 20.0);
        assert_eq!(model.extent_at(5), 0.0);

        model.set_len(6);
        assert_eq!(model.extent_at(5), 10.0);
    }

    #[test]
    fn out_of_range_rebuild() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(10.0, 4);

        model.rebuild([(2, ()), (5, ())], &|_| 20.0);
        assert_eq!(model.total_extent(), 50.0);

        model.set_len(6);
        assert_eq!(model.total_extent(), 70.0);
    }

    #[test]
    fn index_at_offset_handles_initial_guess_past_len() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(1.0, 5);
        model.set_extent(0, 100.0);

        assert_eq!(model.index_at_offset(50.0), 0);
        assert_eq!(model.index_at_offset(99.0), 0);
    }

    #[test]
    fn index_at_offset_returns_greatest_index_for_trailing_zero_sized_defaults() {
        let mut model = SparsePrefixSumExtentModel::<f32>::new(0.0, 4);
        model.set_extent(1, 20.0);
        model.set_extent(2, 10.0);

        assert_eq!(model.offset_of(3), 30.0);
        assert_eq!(model.index_at_offset(30.0), 3);
        assert_eq!(model.index_at_offset(35.0), 3);
    }
}
