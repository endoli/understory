// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Primitive geometry types and helpers.

use core::cmp::Ordering;
use core::fmt::Debug;

/// Axis-aligned bounding box in 2D.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Aabb2D<T> {
    /// Minimum x (left)
    pub min_x: T,
    /// Minimum y (top)
    pub min_y: T,
    /// Maximum x (right)
    pub max_x: T,
    /// Maximum y (bottom)
    pub max_y: T,
}

impl<T> Aabb2D<T> {
    /// Create a new AABB from min/max corners.
    #[inline(always)]
    pub const fn new(min_x: T, min_y: T, max_x: T, max_y: T) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }
}

impl<T: Copy + PartialOrd> Aabb2D<T> {
    /// Whether this AABB contains the point.
    #[inline]
    pub fn contains_point(&self, x: T, y: T) -> bool {
        self.min_x <= x && self.min_y <= y && x <= self.max_x && y <= self.max_y
    }

    /// The intersection of two AABBs.
    #[inline]
    pub fn intersect(&self, other: &Self) -> Self {
        let min_x = max_t(self.min_x, other.min_x);
        let min_y = max_t(self.min_y, other.min_y);
        let max_x = min_t(self.max_x, other.max_x);
        let max_y = min_t(self.max_y, other.max_y);
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    /// Determines whether this AABB overlaps with another in any way.
    ///
    /// Note that the edge of the AABB is considered to be part of itself, meaning
    /// that two AABBs that share an edge are considered to overlap.
    ///
    /// Returns `true` if the AABBs overlap, `false` otherwise.
    ///
    /// If you want to compute the *intersection* of two AABBs, use the
    /// [`intersect`][Self::intersect`] method instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use understory_index::Aabb2D;
    ///
    /// let aabb1 = Aabb2D::new(0.0, 0.0, 10.0, 10.0);
    /// let aabb2 = Aabb2D::new(5.0, 5.0, 15.0, 15.0);
    /// assert!(aabb1.overlaps(&aabb2));
    ///
    /// let aabb1 = Aabb2D::new(0.0, 0.0, 10.0, 10.0);
    /// let aabb2 = Aabb2D::new(10.0, 0.0, 20.0, 10.0);
    /// assert!(aabb1.overlaps(&aabb2));
    ///
    /// let aabb1 = Aabb2D::new(0.0, 0.0, 10.0, 10.0);
    /// let aabb2 = Aabb2D::new(11.0, 0.0, 20.0, 10.0);
    /// assert!(!aabb1.overlaps(&aabb2));
    /// ```
    #[inline]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }

    /// The smallest AABB enclosing two AABBs.
    #[inline]
    pub(crate) fn union(&self, other: Self) -> Self {
        Self {
            min_x: min_t(self.min_x, other.min_x),
            min_y: min_t(self.min_y, other.min_y),
            max_x: max_t(self.max_x, other.max_x),
            max_y: max_t(self.max_y, other.max_y),
        }
    }

    /// Return true if the AABB is empty or inverted (no area). Assumes no NaN.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.max_x < self.min_x || self.max_y < self.min_y
    }
}

impl<T: Scalar> Aabb2D<T> {
    /// Create an AABB from origin and size in f32.
    #[inline]
    pub fn from_xywh(x: T, y: T, w: T, h: T) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: T::add(x, w),
            max_y: T::add(y, h),
        }
    }

    /// Compute the area of an AABB using the scalar's widened accumulator type.
    #[inline]
    pub fn area(&self) -> T::Acc {
        let w = T::max_zero(T::sub(self.max_x, self.min_x));
        let h = T::max_zero(T::sub(self.max_y, self.min_y));
        T::widen(w) * T::widen(h)
    }
}

/// Numeric scalar abstraction for 2D AABBs used by backends.
///
/// This trait provides a minimal set of operations required for SAH metrics and
/// centroid computations, and an associated widened accumulator type for area
/// (e.g., f32→f64, i64→i128).
pub trait Scalar: Copy + PartialOrd + Debug {
    /// Widened accumulator type suitable for area/cost computations.
    type Acc: Copy
        + PartialOrd
        + core::ops::Add<Output = Self::Acc>
        + core::ops::Sub<Output = Self::Acc>
        + core::ops::Mul<Output = Self::Acc>
        + Debug;

    /// Add two scalar values.
    fn add(a: Self, b: Self) -> Self;

    /// Subtract two scalar values: a - b.
    fn sub(a: Self, b: Self) -> Self;

    /// Zero value for the scalar type.
    fn zero() -> Self;

    /// Max of the scalar value and zero.
    fn max_zero(v: Self) -> Self;

    /// Midpoint between a and b (used for centroid ordering).
    fn mid(a: Self, b: Self) -> Self;

    /// Convert a scalar to the accumulator type.
    fn widen(v: Self) -> Self::Acc;

    /// Convert a `usize` to the accumulator type (for SAH weighting).
    fn acc_from_usize(n: usize) -> Self::Acc;
}

impl Scalar for f32 {
    type Acc = f64;

    #[inline]
    fn add(a: Self, b: Self) -> Self {
        a + b
    }

    #[inline]
    fn sub(a: Self, b: Self) -> Self {
        a - b
    }

    #[inline(always)]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    fn max_zero(v: Self) -> Self {
        v.max(0.0)
    }

    #[inline]
    fn mid(a: Self, b: Self) -> Self {
        0.5 * (a + b)
    }

    #[inline]
    fn widen(v: Self) -> Self::Acc {
        v as f64
    }

    #[inline]
    fn acc_from_usize(n: usize) -> Self::Acc {
        n as f64
    }
}

impl Scalar for f64 {
    type Acc = Self;

    #[inline]
    fn add(a: Self, b: Self) -> Self {
        a + b
    }

    #[inline]
    fn sub(a: Self, b: Self) -> Self {
        a - b
    }

    #[inline(always)]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    fn max_zero(v: Self) -> Self {
        v.max(0.0)
    }

    #[inline]
    fn mid(a: Self, b: Self) -> Self {
        0.5 * (a + b)
    }

    #[inline(always)]
    fn widen(v: Self) -> Self::Acc {
        v
    }

    #[inline]
    fn acc_from_usize(n: usize) -> Self::Acc {
        n as Self::Acc
    }
}

impl Scalar for i64 {
    type Acc = i128;

    #[inline]
    fn add(a: Self, b: Self) -> Self {
        a.saturating_add(b)
    }

    #[inline]
    fn sub(a: Self, b: Self) -> Self {
        a.saturating_sub(b)
    }

    #[inline(always)]
    fn zero() -> Self {
        0
    }

    #[inline]
    fn max_zero(v: Self) -> Self {
        v.max(0)
    }

    #[inline]
    fn mid(a: Self, b: Self) -> Self {
        // Average without overflow: (a & b) + ((a ^ b) >> 1)
        (a & b) + ((a ^ b) >> 1)
    }

    #[inline]
    fn widen(v: Self) -> Self::Acc {
        v as i128
    }

    #[inline]
    fn acc_from_usize(n: usize) -> Self::Acc {
        n as i128
    }
}

/// Helper alias for the widened accumulator type `Scalar::Acc` associated with a `T: Scalar`.
pub type ScalarAcc<T> = <T as Scalar>::Acc;

pub(crate) fn min_t<T: PartialOrd + Copy>(a: T, b: T) -> T {
    match a.partial_cmp(&b) {
        Some(Ordering::Greater) => b,
        _ => a,
    }
}

pub(crate) fn max_t<T: PartialOrd + Copy>(a: T, b: T) -> T {
    match a.partial_cmp(&b) {
        Some(Ordering::Less) => b,
        _ => a,
    }
}
