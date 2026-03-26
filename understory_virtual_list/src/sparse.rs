// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An [`ExtentModel`] backed by per-item extents and a lazily-maintained prefix-sum cache,
//! with fixed-sized placeholders for un-materialized items.

use alloc::collections::BTreeMap;

use crate::{ExtentModel, Scalar};

/// An [`ExtentModel`] backed by per-item extents and a lazily-maintained prefix-sum cache.
/// with fixed-sized placeholders for un-materialized items.
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
        self.default_extent = default_extent;
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
        self.extents_and_prefixes.retain(|i, _| *i <= len);
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
        self.last_valid = Some(0);

        for (index, item) in items {
            let mut extent = size_fn(&item);
            debug_assert!(
                extent.is_finite(),
                "SpasrePrefixSumExtentModel extents must be finite; got {extent:?}"
            );
            if extent.is_sign_negative() {
                extent = S::zero();
            }
            self.extents_and_prefixes.insert(index, (extent, S::zero()));
        }
    }

    /// Updates the extent of a single item and marks prefix sums dirty from this index.
    pub fn set_extent(&mut self, index: usize, extent: S) {
        // Extents are expected to be finite. Catch NaNs (and infinities) in
        // debug builds so misuse does not go unnoticed.
        debug_assert!(
            extent.is_finite(),
            "PrefixSumExtentModel extents must be finite; got {extent:?}"
        );
        // Clamp finite negative values to `0.0`.
        self.extents_and_prefixes.insert(
            index,
            if extent.is_sign_negative() {
                (S::zero(), S::zero())
            } else {
                (extent, S::zero())
            },
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

    fn offset_at_inner(&mut self, index: usize) -> S {
        if index == 0 {
            return S::zero();
        }
        self.ensure_prefix_through(index);
        let from_default = self.default_extent
            * S::from_usize(index - self.extents_and_prefixes.range(..index).count());
        let last = self.extents_and_prefixes.range(..index).last();
        let from_prefix = last.map_or_else(S::zero, |(_, (_, p))| *p);
        // dbg!(last, from_default, from_prefix);
        from_default + from_prefix
    }

    fn extent_at_inner(&self, index: usize) -> S {
        self.extents_and_prefixes
            .get(&index)
            .map_or(self.default_extent, |(e, _)| *e)
    }

    fn total_extent_inner(&mut self) -> S {
        self.len.checked_sub(1).map_or_else(S::zero, |last| {
            self.offset_at_inner(last) + self.extent_at_inner(last)
        })
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
        if len == 0 {
            return S::zero();
        }
        self.offset_at_inner(len + 1)
    }

    /// Returns an index for `offset` within the first `len` items.
    ///
    /// This is useful for hosts that want to constrain queries to a known
    /// prefix of the data.
    pub fn index_at_offset_for_len(&mut self, offset: S, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        // let len = len.min(self.extents_and_prefixes.len());

        self.ensure_prefix_through(len);

        let target = offset.max(S::zero());
        let mut result = (target / self.default_extent)
            .truncate_to_isize()
            .cast_unsigned();
        loop {
            let offset_at = self.offset_at(result);
            let offset_past = offset_at + self.extent_at(result);
            match (offset_at <= target, target < offset_past) {
                (true, true) => break result,
                (true, false) => result += 1,
                (false, true) => result -= 1,
                (false, false) => unreachable!(),
            }
        }
    }
}

impl<S: Scalar> ExtentModel for SparsePrefixSumExtentModel<S> {
    type Scalar = S;

    fn len(&self) -> usize {
        self.len()
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
        self.index_at_offset_for_len(offset, usize::MAX)
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
    fn comp() {
        let mut dense_model = PrefixSumExtentModel::<f64>::new();
        dense_model.set_len(100);
        for i in 0..100 {
            dense_model.set_extent(i, 51.);
        }
        let dense_strip = compute_visible_strip(&mut dense_model, 0., 640., 320., 320.);

        let mut sparse_model = SparsePrefixSumExtentModel::new(51., 100);
        assert_eq!(sparse_model.offset_at_inner(100), 5100.);
        for i in 0..6 {
            sparse_model.set_extent(i, 51.);
        }
        assert_eq!(sparse_model.offset_at_inner(100), 5100.);
        let sparse_strip = compute_visible_strip(&mut sparse_model, 0., 640., 320., 320.);

        assert_eq!(dense_strip, sparse_strip);
    }
}
