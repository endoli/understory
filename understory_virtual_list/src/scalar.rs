// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Scalar abstraction used by extent models.
//!
//!
//! This trait is intentionally small and only implemented for `f32` and `f64`.

use core::fmt::Debug;
use core::ops::{Add, Div, Mul, Sub};

/// Scalar type used for extents, offsets, and scroll positions.
///
/// This is currently implemented for `f32` and `f64`. The trait is deliberately
/// minimal and geared toward floating-point coordinates.
pub trait Scalar:
    Copy
    + PartialOrd
    + Debug
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
{
    /// Additive identity (typically `0.0`).
    fn zero() -> Self;

    /// Returns the maximum of `self` and `other`.
    fn max(self, other: Self) -> Self;

    /// Returns the minimum of `self` and `other`.
    fn min(self, other: Self) -> Self;

    /// Returns `true` if the value is finite (not NaN or infinite).
    fn is_finite(self) -> bool;

    /// Returns `true` if the value is negative, including `-0.0`.
    fn is_sign_negative(self) -> bool;

    /// Constructs from a `usize` lossily.
    fn from_usize(value: usize) -> Self;

    /// Clamps negative values to zero.
    fn clamp_non_negative(self) -> Self {
        if self.is_sign_negative() {
            Self::zero()
        } else {
            self
        }
    }

    /// Floors the value and converts it to `isize`.
    ///
    /// Implementations may clamp or truncate as needed; callers are expected
    /// to clamp the result to a valid index range afterwards.
    fn floor_to_isize(self) -> isize;
}

impl Scalar for f32 {
    fn zero() -> Self {
        0.0
    }

    fn max(self, other: Self) -> Self {
        Self::max(self, other)
    }

    fn min(self, other: Self) -> Self {
        Self::min(self, other)
    }

    fn is_finite(self) -> bool {
        Self::is_finite(self)
    }

    fn is_sign_negative(self) -> bool {
        Self::is_sign_negative(self)
    }

    fn from_usize(value: usize) -> Self {
        value as Self
    }

    fn floor_to_isize(self) -> isize {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Used only for index approximation; result is clamped immediately after"
        )]
        {
            self as isize
        }
    }
}

impl Scalar for f64 {
    fn zero() -> Self {
        0.0
    }

    fn max(self, other: Self) -> Self {
        Self::max(self, other)
    }

    fn min(self, other: Self) -> Self {
        Self::min(self, other)
    }

    fn is_finite(self) -> bool {
        Self::is_finite(self)
    }

    fn is_sign_negative(self) -> bool {
        Self::is_sign_negative(self)
    }

    fn from_usize(value: usize) -> Self {
        value as Self
    }

    fn floor_to_isize(self) -> isize {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Used only for index approximation; result is clamped immediately after"
        )]
        {
            self as isize
        }
    }
}
