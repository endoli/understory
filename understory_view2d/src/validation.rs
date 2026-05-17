// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use core::ops::Range;

use kurbo::{Point, Rect, Vec2};

pub(crate) fn normalize_zoom_limits(
    min_zoom: f64,
    max_zoom: f64,
    current_min: f64,
    current_max: f64,
) -> (f64, f64) {
    let min_zoom = sanitize_zoom_limit(min_zoom, current_min);
    let max_zoom = sanitize_zoom_limit(max_zoom, current_max);
    if min_zoom <= max_zoom {
        (min_zoom, max_zoom)
    } else {
        (max_zoom, min_zoom)
    }
}

pub(crate) fn sanitize_zoom_value(zoom: f64) -> Option<f64> {
    if zoom.is_finite() && zoom >= f64::MIN_POSITIVE {
        Some(zoom)
    } else {
        None
    }
}

pub(crate) fn point_is_finite(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

pub(crate) fn vec2_is_finite(vec: Vec2) -> bool {
    vec.x.is_finite() && vec.y.is_finite()
}

pub(crate) fn rect_is_finite(rect: Rect) -> bool {
    rect.x0.is_finite() && rect.y0.is_finite() && rect.x1.is_finite() && rect.y1.is_finite()
}

pub(crate) fn view_rect_is_valid(rect: Rect) -> bool {
    rect_is_finite(rect) && rect.width() >= 0.0 && rect.height() >= 0.0
}

pub(crate) fn world_rect_is_valid(rect: Rect) -> bool {
    rect_is_finite(rect) && rect.width() > 0.0 && rect.height() > 0.0
}

pub(crate) fn view_span_is_valid(span: &Range<f64>) -> bool {
    span.start.is_finite() && span.end.is_finite() && span.start <= span.end
}

pub(crate) fn world_range_is_valid(range: &Range<f64>) -> bool {
    range.start.is_finite() && range.end.is_finite() && range.start < range.end
}

pub(crate) fn sanitize_grid_spacing_base(base: f64) -> f64 {
    if base.is_finite() {
        base.abs().max(f64::MIN_POSITIVE)
    } else {
        f64::MIN_POSITIVE
    }
}

pub(crate) fn nice_grid_spacing(world_units_per_pixel: f64, base: f64) -> f64 {
    let target_px = 64.0_f64;
    let base = sanitize_grid_spacing_base(base);
    let world_units_per_pixel = if world_units_per_pixel.is_finite() {
        world_units_per_pixel.abs().max(f64::MIN_POSITIVE)
    } else {
        f64::MAX / target_px
    };
    let desired = (world_units_per_pixel * target_px).max(base);
    let desired = if desired.is_finite() {
        desired
    } else {
        f64::MAX
    };

    let mut unit = 1.0_f64;
    while unit <= desired / 10.0 {
        unit *= 10.0;
    }

    loop {
        for m in [1.0_f64, 2.0, 5.0, 10.0] {
            let step = m * unit;
            if !step.is_finite() {
                return f64::MAX;
            }
            if step >= desired {
                return step;
            }
        }
        unit *= 10.0;
    }
}

fn sanitize_zoom_limit(value: f64, fallback: f64) -> f64 {
    sanitize_zoom_value(value).unwrap_or(fallback)
}
