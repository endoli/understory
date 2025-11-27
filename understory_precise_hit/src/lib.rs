// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Geometry-level precise hit testing utilities.
//!
//! This crate provides small, reusable primitives for doing narrow-phase
//! hit testing in local 2D coordinates, built on top of [`kurbo`]. It is
//! intentionally decoupled from any particular scene tree or event router.
//!
//! # Typical usage
//!
//! - Use your own broad-phase index (e.g., `understory_index` or
//!   `understory_box_tree`) to cull candidates.
//! - Transform the query point into a shape's local coordinates.
//! - Call [`PreciseHitTest::hit_test_local`] on the shape.
//! - Use the returned [`HitScore`] only for *scoring and tie-breaking*.
//!   Any rich metadata (segment indices, glyphs, winding info, etc.) should
//!   be carried alongside the score in your own structures (for example as
//!   the `meta` payload in a responder hit type).
//!
//! # Key types
//!
//! - [`HitParams`] – per-query parameters such as fill/stroke tolerances and
//!   a hint for preferring fill vs stroke when both are possible.
//! - [`HitScore`] – a small scoring record `{ distance, kind }` used for
//!   ranking candidates. Lower distance is preferred; [`HitKind`] is a coarse
//!   class (fill, stroke, handle, other).
//! - [`PreciseHitTest`] – a trait implemented by shapes that can answer
//!   “does this local-space point hit me?” queries.
//!
//! ## Shapes and scope
//!
//! This crate includes [`PreciseHitTest`] implementations for several
//! [`kurbo`] primitives:
//!
//! - [`Rect`] – axis-aligned rectangle, with configurable fill tolerance.
//! - [`Circle`] – circle, treated as a filled disk.
//! - [`RoundedRect`] – rounded rectangle, treated as a filled shape; tolerant
//!   hits are based on its bounding box.
//! - [`BezPath`] – **fill-only** path hit using [`kurbo::Shape::contains`].
//!   The fill rule is whatever `kurbo` uses for `contains`; it is not
//!   currently configurable here. Stroke hits for paths are intentionally left
//!   to higher-level engines, which can wrap their own stroke representations
//!   and still implement [`PreciseHitTest`].
//!
//! The [`stroke`] module provides helpers for stroke-oriented tests (for
//! example, a simple [`stroke::StrokedLine`] type). These are minimal
//! building blocks and not a full stroke model; engines are expected to build
//! richer stroke behavior on top.

#![no_std]

extern crate alloc;

use core::cmp::Ordering;

#[cfg(not(feature = "std"))]
use kurbo::common::FloatFuncs as _;
use kurbo::{BezPath, Circle, Point, Rect, RoundedRect, Shape};

/// Stroke-oriented helpers and primitives.
pub mod stroke;

/// Kind of hit produced by a precise test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HitKind {
    /// Hit the interior/fill of a shape.
    Fill,
    /// Hit the stroked outline of a shape.
    Stroke,
    /// Hit a control handle or other auxiliary affordance.
    Handle,
    /// Hit, but kind is unspecified/other.
    Other,
}

/// Parameters controlling precise hit tests.
#[derive(Clone, Copy, Debug)]
pub struct HitParams {
    /// Tolerance in local units for hits against filled regions.
    ///
    /// This is typically used to slightly inflate shapes for touch/pointer
    /// input or fuzzy selection, or to treat points very near the boundary
    /// as hits.
    pub fill_tolerance: f64,
    /// Tolerance in local units for hits against stroked outlines.
    ///
    /// Engines that distinguish between fill and stroke hits can use this to
    /// widen or narrow stroke pick regions independently of fill behavior.
    pub stroke_tolerance: f64,
    /// Prefer fill hits over stroke hits when both are possible at the same
    /// location.
    ///
    /// This is a hint for callers when combining multiple `HitScore`s for the
    /// same key; the trait itself does not enforce any policy.
    pub prefer_fill: bool,
}

impl Default for HitParams {
    fn default() -> Self {
        Self {
            fill_tolerance: 0.0,
            stroke_tolerance: 0.0,
            prefer_fill: true,
        }
    }
}

/// Score returned from a precise hit.
///
/// Lower distance is considered a better (closer) hit for tie-breaking.
#[derive(Clone, Copy, Debug)]
pub struct HitScore {
    /// Geometric distance in local coordinate space.
    pub distance: f64,
    /// Classification of what was hit.
    pub kind: HitKind,
}

impl HitScore {
    /// Convenience constructor for a filled hit at distance 0.
    pub const fn filled() -> Self {
        Self {
            distance: 0.0,
            kind: HitKind::Fill,
        }
    }

    /// Compare two scores, preferring smaller distance; ties keep original order.
    pub fn cmp_distance(&self, other: &Self) -> Ordering {
        self.distance
            .partial_cmp(&other.distance)
            .unwrap_or(Ordering::Equal)
    }
}

/// Trait for precise 2D hit testing in local coordinates.
///
/// Implementors are free to use any strategy, but should treat the
/// tolerances in [`HitParams`] as inclusive radii when appropriate.
pub trait PreciseHitTest {
    /// Perform a precise hit test against `pt` in the shape's local
    /// coordinate space.
    ///
    /// Returns `Some(HitScore)` when the point is considered a hit.
    fn hit_test_local(&self, pt: Point, params: &HitParams) -> Option<HitScore>;
}

/// Simple rectangular precise hit implementation.
impl PreciseHitTest for Rect {
    fn hit_test_local(&self, pt: Point, params: &HitParams) -> Option<HitScore> {
        // Expand the rect by fill_tolerance on all sides for a near-miss hit.
        let inflated = if params.fill_tolerance > 0.0 {
            self.inflate(params.fill_tolerance, params.fill_tolerance)
        } else {
            *self
        };
        if inflated.contains(pt) {
            // Use distance to the original rect edge as score; interior points are 0.
            let dx = if pt.x < self.x0 {
                self.x0 - pt.x
            } else if pt.x > self.x1 {
                pt.x - self.x1
            } else {
                0.0
            };
            let dy = if pt.y < self.y0 {
                self.y0 - pt.y
            } else if pt.y > self.y1 {
                pt.y - self.y1
            } else {
                0.0
            };
            let dist = (dx * dx + dy * dy).sqrt();
            Some(HitScore {
                distance: dist,
                kind: HitKind::Fill,
            })
        } else {
            None
        }
    }
}

/// Precise hit implementation for circular shapes using [`Circle`].
///
/// The hit is considered a fill hit when the distance from the point to the
/// center is within the radius plus tolerance.
impl PreciseHitTest for Circle {
    fn hit_test_local(&self, pt: Point, params: &HitParams) -> Option<HitScore> {
        let center = self.center;
        let dx = pt.x - center.x;
        let dy = pt.y - center.y;
        let dist = (dx * dx + dy * dy).sqrt();
        let radius = self.radius;
        let tol = params.fill_tolerance;
        if dist <= radius + tol {
            // Distance inside the circle is 0, outside is how far we exceeded the radius.
            let distance = if dist <= radius { 0.0 } else { dist - radius };
            Some(HitScore {
                distance,
                kind: HitKind::Fill,
            })
        } else {
            None
        }
    }
}

/// Precise hit implementation for rounded rectangles using [`RoundedRect`].
///
/// This uses the shape's own `contains`/`bounding_box` implementation and
/// applies `fill_tolerance` as an inflation on the bounding box for a simple
/// near-miss behavior.
impl PreciseHitTest for RoundedRect {
    fn hit_test_local(&self, pt: Point, params: &HitParams) -> Option<HitScore> {
        // First, reject quickly using the bounding box plus fill tolerance.
        let bounds = self.bounding_box();
        let inflated = if params.fill_tolerance > 0.0 {
            bounds.inflate(params.fill_tolerance, params.fill_tolerance)
        } else {
            bounds
        };
        if !inflated.contains(pt) {
            return None;
        }

        // Within the inflated bounds: treat `contains` as a filled hit.
        if self.contains(pt) {
            Some(HitScore::filled())
        } else if params.fill_tolerance > 0.0 {
            // For now we do not compute exact distance to the corner curves.
            // Report a generic tolerant hit with distance equal to the
            // configured tolerance.
            Some(HitScore {
                distance: params.fill_tolerance,
                kind: HitKind::Fill,
            })
        } else {
            None
        }
    }
}

/// Precise hit implementation for filled bezier paths using [`BezPath`].
///
/// This uses the path's `contains` and `bounding_box` methods for a fill-only
/// hit test and applies `fill_tolerance` as an inflation on the bounding
/// box for near-miss behavior.
impl PreciseHitTest for BezPath {
    fn hit_test_local(&self, pt: Point, params: &HitParams) -> Option<HitScore> {
        let bounds = self.bounding_box();
        let inflated = if params.fill_tolerance > 0.0 {
            bounds.inflate(params.fill_tolerance, params.fill_tolerance)
        } else {
            bounds
        };
        if !inflated.contains(pt) {
            return None;
        }
        if self.contains(pt) {
            Some(HitScore::filled())
        } else if params.fill_tolerance > 0.0 {
            Some(HitScore {
                distance: params.fill_tolerance,
                kind: HitKind::Fill,
            })
        } else {
            None
        }
    }
}

/// Generic precise hit test for any [`kurbo::Shape`].
///
/// This provides a fallback implementation using the shape's `contains` and
/// `bounding_box` methods. We deliberately avoid a blanket `impl<T: Shape>` to
/// make it straightforward for engines to provide their own specialized
/// implementations without running into coherence issues.
pub fn hit_test_shape<S: Shape>(shape: &S, pt: Point, params: &HitParams) -> Option<HitScore> {
    let bounds = shape.bounding_box();
    let inflated = if params.fill_tolerance > 0.0 {
        bounds.inflate(params.fill_tolerance, params.fill_tolerance)
    } else {
        bounds
    };

    if !inflated.contains(pt) {
        return None;
    }

    if shape.contains(pt) {
        Some(HitScore::filled())
    } else if params.fill_tolerance > 0.0 {
        Some(HitScore {
            distance: params.fill_tolerance,
            kind: HitKind::Fill,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stroke::StrokedLine;
    use kurbo::{BezPath, Line, Rect, RoundedRect};

    #[test]
    fn rect_hit_inside() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let pt = Point::new(5.0, 5.0);
        let score = r
            .hit_test_local(pt, &HitParams::default())
            .expect("expected hit");
        assert_eq!(score.kind, HitKind::Fill);
        assert_eq!(score.distance, 0.0);
    }

    #[test]
    fn rect_miss_outside_without_tolerance() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let pt = Point::new(11.0, 5.0);
        assert!(r.hit_test_local(pt, &HitParams::default()).is_none());
    }

    #[test]
    fn rect_hit_with_tolerance() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let pt = Point::new(10.5, 5.0);
        let score = r
            .hit_test_local(
                pt,
                &HitParams {
                    fill_tolerance: 1.0,
                    ..HitParams::default()
                },
            )
            .expect("expected tolerant hit");
        assert!(score.distance > 0.0);
    }

    #[test]
    fn circle_hit_and_miss() {
        let c = Circle::new((0.0, 0.0), 5.0);
        let inside = Point::new(1.0, 1.0);
        let outside = Point::new(10.0, 0.0);

        let inside_score = c
            .hit_test_local(inside, &HitParams::default())
            .expect("expected inside hit");
        assert_eq!(inside_score.distance, 0.0);

        assert!(c.hit_test_local(outside, &HitParams::default()).is_none());
    }

    #[test]
    fn rounded_rect_hit_and_miss() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let rr = RoundedRect::from_rect(rect, 2.0);
        let inside = Point::new(5.0, 5.0);
        let outside = Point::new(20.0, 20.0);

        assert!(rr.hit_test_local(inside, &HitParams::default()).is_some());
        assert!(rr.hit_test_local(outside, &HitParams::default()).is_none());
    }

    #[test]
    fn bezpath_hit_within_bounds() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((10.0, 0.0));
        path.line_to((10.0, 10.0));
        path.line_to((0.0, 10.0));
        path.close_path();

        let inside = Point::new(5.0, 5.0);
        let outside = Point::new(20.0, 20.0);

        assert!(path.hit_test_local(inside, &HitParams::default()).is_some());
        assert!(
            path.hit_test_local(outside, &HitParams::default())
                .is_none()
        );
    }

    #[test]
    fn stroked_line_hit_and_miss() {
        let line = Line::new((0.0, 0.0), (10.0, 0.0));
        let stroked = StrokedLine {
            line,
            half_width: 1.0,
        };

        let center = Point::new(5.0, 0.0);
        let near = Point::new(5.0, 0.5);
        let outside = Point::new(5.0, 5.0);

        let params = HitParams::default();

        assert!(stroked.hit_test_local(center, &params).is_some());
        assert!(stroked.hit_test_local(near, &params).is_some());
        assert!(stroked.hit_test_local(outside, &params).is_none());
    }
}
