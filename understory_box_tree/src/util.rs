// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use kurbo::{Affine, Rect};
use understory_index::Aabb2D;

/// Transform an axis-aligned [`Rect`] by an [`Affine`] and return a tight axis-aligned bounding
/// box in world space.
///
/// The returned AABB is tight in the sense that it is no larger than it needs to be while fully
/// containing the transformed rectangle. Note that a transformed rectangle will in general be a
/// (non-axis-aligned) parallelogram.
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

#[cfg(test)]
mod tests {
    use kurbo::{Affine, Rect};

    use super::*;

    /// The bounding box of a square rotated by 45 degrees has sides of length
    /// `sqrt(2) * original length`.
    ///
    /// Hence, the area of the bounding box is `2 * area of square`.
    #[test]
    fn non_axis_aligned_bbox_rotation() {
        let rect = Rect::new(-1., -1., 2., 2.);
        let bbox_rotation = transform_rect_bbox(Affine::rotate(45_f64.to_radians()), rect);
        assert!((bbox_rotation.area() - 2. * rect.area()).abs() < 1e-8);
    }
}
