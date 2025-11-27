// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Stroke-oriented helpers for precise hit testing.
//!
//! These types are intentionally small building blocks rather than a full
//! stroke model. Engines are expected to compose their own stroke behavior
//! (joins, caps, variable width, etc.) on top of these primitives.

#[cfg(not(feature = "std"))]
use kurbo::common::FloatFuncs as _;
use kurbo::{Line, Point};

use crate::{HitKind, HitParams, HitScore, PreciseHitTest};

/// A simple stroked line segment (centerline + half-width).
///
/// The precise hit test uses the distance from the query point to the line
/// segment and compares it against the half-width plus
/// [`HitParams::stroke_tolerance`]. This does not model joins, caps, or
/// variable-width strokes; it is a minimal helper for straight segments.
#[derive(Clone, Copy, Debug)]
pub struct StrokedLine {
    /// The centerline segment in local coordinates.
    pub line: Line,
    /// Half of the stroke width in local units.
    pub half_width: f64,
}

impl PreciseHitTest for StrokedLine {
    fn hit_test_local(&self, pt: Point, params: &HitParams) -> Option<HitScore> {
        let p0 = self.line.p0;
        let p1 = self.line.p1;
        let vx = p1.x - p0.x;
        let vy = p1.y - p0.y;
        let wx = pt.x - p0.x;
        let wy = pt.y - p0.y;
        let len2 = vx * vx + vy * vy;
        let t = if len2 > 0.0 {
            (wx * vx + wy * vy) / len2
        } else {
            0.0
        };
        let t = t.clamp(0.0, 1.0);
        let proj_x = p0.x + t * vx;
        let proj_y = p0.y + t * vy;
        let dx = pt.x - proj_x;
        let dy = pt.y - proj_y;
        let dist = (dx * dx + dy * dy).sqrt();

        let limit = self.half_width + params.stroke_tolerance;
        if dist <= limit {
            Some(HitScore {
                distance: dist,
                kind: HitKind::Stroke,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
