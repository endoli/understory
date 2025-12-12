// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A simple extent model with uniform per-item extent.

use crate::{ExtentModel, ResizableExtentModel, Scalar};

/// An [`ExtentModel`] where all items share the same extent.
#[derive(Debug, Clone, Copy)]
pub struct FixedExtentModel<S: Scalar> {
    len: usize,
    extent: S,
}

impl<S: Scalar> FixedExtentModel<S> {
    /// Creates a new model with `len` items of uniform `extent`.
    #[must_use]
    pub fn new(len: usize, extent: S) -> Self {
        Self {
            len,
            // Clamp finite negative values to `0.0`. NaNs are preserved here;
            // callers are expected to avoid them and `set_extent` debug-asserts.
            extent: if extent.is_sign_negative() {
                S::zero()
            } else {
                extent
            },
        }
    }

    /// Sets the number of items in the strip.
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
    }

    /// Sets the uniform extent for all items.
    pub fn set_extent(&mut self, extent: S) {
        // Extents are expected to be finite. Catch NaNs (and infinities) in
        // debug builds so misuse does not go unnoticed.
        debug_assert!(
            extent.is_finite(),
            "FixedExtentModel extents must be finite; got {extent:?}"
        );
        // Clamp finite negative values to `0.0`.
        self.extent = if extent.is_sign_negative() {
            S::zero()
        } else {
            extent
        };
    }

    /// Returns the uniform extent for all items.
    #[must_use]
    pub const fn extent(&self) -> S {
        self.extent
    }
}

impl<S: Scalar> ExtentModel for FixedExtentModel<S> {
    type Scalar = S;

    fn len(&self) -> usize {
        self.len
    }

    fn total_extent(&mut self) -> S {
        self.extent * S::from_usize(self.len)
    }

    fn extent_of(&mut self, _index: usize) -> S {
        self.extent
    }

    fn offset_of(&mut self, index: usize) -> S {
        S::from_usize(index) * self.extent
    }

    fn index_at_offset(&mut self, offset: S) -> usize {
        if self.len == 0 || self.extent <= S::zero() {
            return 0;
        }
        let ratio = offset / self.extent;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "Index is clamped to bounds immediately after the cast"
        )]
        let i = ratio.floor_to_isize();
        i.clamp(0, self.len as isize - 1) as usize
    }
}

impl<S: Scalar> ResizableExtentModel for FixedExtentModel<S> {
    fn set_len(&mut self, len: usize) {
        self.set_len(len);
    }
}

#[cfg(test)]
mod tests {
    use super::{ExtentModel, FixedExtentModel};

    #[test]
    fn basic_offsets_and_indices() {
        let mut model = FixedExtentModel::new(5, 10.0);
        assert_eq!(model.total_extent(), 50.0);
        assert_eq!(model.offset_of(0), 0.0);
        assert_eq!(model.offset_of(3), 30.0);
        assert_eq!(model.index_at_offset(0.0), 0);
        assert_eq!(model.index_at_offset(9.9), 0);
        assert_eq!(model.index_at_offset(10.0), 1);
        assert_eq!(model.index_at_offset(49.9), 4);
        assert_eq!(model.index_at_offset(100.0), 4);
    }

    #[test]
    fn negative_extents_are_clamped_to_zero() {
        let mut model = FixedExtentModel::new(3, -5.0);
        // Constructor clamps finite negatives to 0.
        assert_eq!(model.extent(), 0.0);

        model.set_extent(-10.0);
        assert_eq!(model.extent(), 0.0);
    }
}
