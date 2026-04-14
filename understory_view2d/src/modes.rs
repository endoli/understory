// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/// Clamp behavior for panning and fitting relative to optional world bounds.
///
/// This enum is shared by both [`crate::Viewport2D`] and [`crate::Viewport1D`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ClampMode {
    /// Do not apply any clamping; the view may move/zoom freely.
    None,
    /// Clamp so that the view never moves completely outside the world bounds.
    ///
    /// When world bounds are present, this mode keeps at least some portion of
    /// them visible if possible.
    #[default]
    KeepSomeVisible,
}

/// How fitted content should be positioned inside the view.
///
/// This mode is consulted by [`crate::Viewport2D::fit_world`],
/// [`crate::Viewport2D::fit_rect`] and [`crate::Viewport1D::fit_world`],
/// [`crate::Viewport1D::fit_range`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FitMode {
    /// Center the fitted content within the view extent.
    ///
    /// For [`crate::Viewport2D`] this centers the fitted rectangle in the view
    /// rect; for [`crate::Viewport1D`] this centers the fitted range in the
    /// view span.
    #[default]
    Center,
    /// Align the minimum corner of the fitted content with the view origin.
    ///
    /// For [`crate::Viewport2D`] this aligns the world‑space minimum corner with
    /// the view rect origin; for [`crate::Viewport1D`] this aligns the range
    /// minimum with the start of the view span.
    AlignMin,
}

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
    if zoom.is_finite() && zoom > 0.0 {
        Some(zoom)
    } else {
        None
    }
}

fn sanitize_zoom_limit(value: f64, fallback: f64) -> f64 {
    sanitize_zoom_value(value).unwrap_or(fallback)
}
