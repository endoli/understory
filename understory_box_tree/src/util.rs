// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use kurbo::{Affine, Rect};
use understory_index::Aabb2D;

/// Transform an axis-aligned `Rect` by an `Affine` and return a conservative
/// axis-aligned bounding box in world space.
pub(crate) fn transform_rect_bbox(affine: Affine, rect: Rect) -> Rect {
    let [a, b, c, d, e, f] = affine.as_coeffs();
    let min_x = (a * rect.x0).min(a * rect.x1) + (c * rect.y0).min(c * rect.y1);
    let max_x = (a * rect.x0).max(a * rect.x1) + (c * rect.y0).max(c * rect.y1);
    let min_y = (b * rect.x0).min(b * rect.x1) + (d * rect.y0).min(d * rect.y1);
    let max_y = (b * rect.x0).max(b * rect.x1) + (d * rect.y0).max(d * rect.y1);
    Rect::new(min_x + e, min_y + f, max_x + e, max_y + f)
}

pub(crate) fn rect_to_aabb(r: Rect) -> Aabb2D<f64> {
    Aabb2D::new(r.x0, r.y0, r.x1, r.y1)
}
