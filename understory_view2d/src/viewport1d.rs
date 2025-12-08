// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use core::ops::Range;

use kurbo::Point;

use crate::modes::{ClampMode, FitMode};

/// 1D viewport over a world‑space axis.
///
/// `Viewport1D` tracks a span in view/device coordinates and a uniform pan+zoom
/// transform along a single world axis. It is conventionally used for the
/// horizontal (X) axis in time/timeline views or scroll regions where only the
/// X dimension is zoomed.
#[derive(Clone, Debug)]
pub struct Viewport1D {
    view_span: Range<f64>,
    world_bounds: Option<Range<f64>>,
    zoom: f64,
    pan: f64,
    min_zoom: f64,
    max_zoom: f64,
    clamp_mode: ClampMode,
    fit_mode: FitMode,
}

impl Viewport1D {
    /// Creates a new 1D viewport over the given view span.
    ///
    /// - `view_span` is expressed in view/device units (typically pixels).
    /// - Initial zoom is `1.0`.
    /// - Initial pan is zero (world origin maps to `view_span.start`).
    /// - Zoom is clamped to the range `[1e-6, 1e6]` by default.
    #[must_use]
    pub fn new(view_span: Range<f64>) -> Self {
        Self {
            view_span,
            world_bounds: None,
            zoom: 1.0,
            pan: 0.0,
            min_zoom: 1e-6,
            max_zoom: 1e6,
            clamp_mode: ClampMode::default(),
            fit_mode: FitMode::default(),
        }
    }

    /// Returns the current view span in device coordinates.
    #[must_use]
    pub fn view_span(&self) -> Range<f64> {
        self.view_span.clone()
    }

    /// Sets the view span in device coordinates.
    ///
    /// This does not change zoom or pan, but it may affect the visible world
    /// region. Clamping is applied afterwards if world bounds are set.
    pub fn set_view_span(&mut self, span: Range<f64>) {
        if self.view_span.start == span.start && self.view_span.end == span.end {
            return;
        }
        self.view_span = span;
        self.clamp_to_bounds();
    }

    /// Sets optional world bounds used for clamping and view fitting.
    pub fn set_world_bounds(&mut self, bounds: Option<Range<f64>>) {
        if self.world_bounds_is_eq(&bounds) {
            return;
        }
        self.world_bounds = bounds;
        self.clamp_to_bounds();
    }

    /// Returns the current world bounds, if any.
    #[must_use]
    pub fn world_bounds(&self) -> Option<Range<f64>> {
        self.world_bounds.clone()
    }

    /// Returns the current uniform zoom factor.
    #[must_use]
    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    /// Sets the minimum and maximum zoom factors.
    ///
    /// The provided range is normalized so that `min_zoom <= max_zoom`. The
    /// current zoom is clamped into the new range.
    pub fn set_zoom_limits(&mut self, min_zoom: f64, max_zoom: f64) {
        let (min_zoom, max_zoom) = if min_zoom <= max_zoom {
            (min_zoom, max_zoom)
        } else {
            (max_zoom, min_zoom)
        };
        self.min_zoom = min_zoom;
        self.max_zoom = max_zoom;
        self.set_zoom(self.zoom);
    }

    /// Sets the zoom factor, clamping it into the configured zoom range.
    pub fn set_zoom(&mut self, zoom: f64) {
        let clamped = zoom.clamp(self.min_zoom, self.max_zoom);
        if (self.zoom - clamped).abs() < f64::EPSILON {
            return;
        }
        self.zoom = clamped;
        self.clamp_to_bounds();
    }

    /// Sets the clamp mode for panning relative to world bounds.
    pub fn set_clamp_mode(&mut self, mode: ClampMode) {
        if self.clamp_mode != mode {
            self.clamp_mode = mode;
            self.clamp_to_bounds();
        }
    }

    /// Returns the current clamp mode.
    #[must_use]
    pub fn clamp_mode(&self) -> ClampMode {
        self.clamp_mode
    }

    /// Sets how fitted content should be positioned inside the view span.
    pub fn set_fit_mode(&mut self, mode: FitMode) {
        self.fit_mode = mode;
    }

    /// Returns the current fit mode.
    #[must_use]
    pub fn fit_mode(&self) -> FitMode {
        self.fit_mode
    }

    /// Pans the view by a delta in view/device space.
    ///
    /// This adjusts the pan offset and then applies clamping relative to world
    /// bounds if configured.
    pub fn pan_by_view(&mut self, delta: f64) {
        if delta == 0.0 {
            return;
        }
        self.pan += delta;
        self.clamp_to_bounds();
    }

    /// Zooms around a given anchor point in view/device coordinates.
    ///
    /// The anchor point remains fixed in view space as much as possible under
    /// the new zoom level.
    pub fn zoom_about_view_point(&mut self, anchor_view_x: f64, factor: f64) {
        if factor <= 0.0 {
            return;
        }
        let old_zoom = self.zoom;
        let new_zoom = (old_zoom * factor).clamp(self.min_zoom, self.max_zoom);
        if (new_zoom - old_zoom).abs() < f64::EPSILON {
            return;
        }

        let old_world = self.view_to_world_x(anchor_view_x);
        self.zoom = new_zoom;
        let new_anchor_view = self.world_to_view_x(old_world);
        let delta_view = anchor_view_x - new_anchor_view;
        self.pan_by_view(delta_view);
    }

    /// Fits the entire world bounds into the view span, preserving aspect ratio.
    ///
    /// If no world bounds are set, this is a no‑op.
    pub fn fit_world(&mut self) {
        if let Some(bounds) = self.world_bounds.clone() {
            self.fit_range(bounds);
        }
    }

    /// Fits the given world‑space range into the view span, preserving aspect ratio.
    pub fn fit_range(&mut self, world_range: Range<f64>) {
        let w_len = world_range.end - world_range.start;
        if w_len <= 0.0 {
            return;
        }
        let v_len = self.view_span.end - self.view_span.start;
        if v_len <= 0.0 {
            return;
        }

        let target_zoom = v_len / w_len.max(f64::MIN_POSITIVE);
        let zoom = target_zoom.clamp(self.min_zoom, self.max_zoom);
        self.zoom = zoom;

        self.pan = match self.fit_mode {
            FitMode::Center => {
                let view_center = (self.view_span.start + self.view_span.end) * 0.5;
                let world_center = (world_range.start + world_range.end) * 0.5;
                view_center - (world_center * zoom)
            }
            FitMode::AlignMin => {
                // Align world min to view start.
                self.view_span.start - world_range.start * zoom
            }
        };

        self.clamp_to_bounds();
    }

    /// Returns the visible world‑space range.
    #[must_use]
    pub fn visible_world_range(&self) -> Range<f64> {
        let start = self.view_to_world_x(self.view_span.start);
        let end = self.view_to_world_x(self.view_span.end);
        if start <= end { start..end } else { end..start }
    }

    /// Converts a world‑space X coordinate into view/device coordinates.
    #[must_use]
    pub fn world_to_view_x(&self, x: f64) -> f64 {
        self.view_span.start + self.pan + self.zoom * x
    }

    /// Converts a view/device‑space X coordinate into world coordinates.
    #[must_use]
    pub fn view_to_world_x(&self, x: f64) -> f64 {
        (x - self.view_span.start - self.pan) / self.zoom
    }

    /// Convenience conversion from a `Point`, using its X coordinate.
    ///
    /// This helper ignores the point's Y coordinate and uses only `pt.x`. It is
    /// intended for timelines and other horizontal 1D views where the X axis is
    /// the world axis of interest.
    #[must_use]
    pub fn view_to_world_point_x(&self, pt: Point) -> f64 {
        self.view_to_world_x(pt.x)
    }

    /// Returns the current world‑units‑per‑pixel ratio along the X axis.
    #[must_use]
    pub fn world_units_per_pixel_x(&self) -> f64 {
        1.0 / self.zoom
    }

    /// Suggests a “nice” grid spacing in world units for the current zoom.
    ///
    /// The returned value is chosen so that grid lines appear roughly tens of
    /// pixels apart (using a 1‑2‑5 ladder), with `base` treated as a lower
    /// bound on the spacing in world units.
    #[must_use]
    pub fn suggest_grid_spacing(&self, base: f64) -> f64 {
        let base = base.abs().max(f64::MIN_POSITIVE);
        let target_px = 64.0_f64;
        let wu_per_px = self.world_units_per_pixel_x().abs();
        let mut desired = wu_per_px * target_px;
        if desired < base {
            desired = base;
        }

        let mut unit = 1.0_f64;
        while unit * 10.0 <= desired {
            unit *= 10.0;
        }

        loop {
            for m in [1.0_f64, 2.0, 5.0, 10.0] {
                let step = m * unit;
                if step >= desired {
                    return step;
                }
            }
            unit *= 10.0;
        }
    }

    /// Snapshot of the current 1D viewport state for debugging and inspection.
    #[must_use]
    pub fn debug_info(&self) -> Viewport1DDebugInfo {
        Viewport1DDebugInfo {
            view_span: self.view_span.clone(),
            world_bounds: self.world_bounds.clone(),
            visible_world_range: self.visible_world_range(),
            zoom: self.zoom,
            pan: self.pan,
            min_zoom: self.min_zoom,
            max_zoom: self.max_zoom,
            clamp_mode: self.clamp_mode,
            fit_mode: self.fit_mode,
        }
    }

    fn world_bounds_is_eq(&self, other: &Option<Range<f64>>) -> bool {
        match (&self.world_bounds, other) {
            (None, None) => true,
            (Some(a), Some(b)) => a.start == b.start && a.end == b.end,
            _ => false,
        }
    }

    fn clamp_to_bounds(&mut self) {
        if self.clamp_mode == ClampMode::None {
            return;
        }
        let bounds = match &self.world_bounds {
            Some(b) if b.end > b.start => b,
            _ => return,
        };

        let visible = self.visible_world_range();
        if visible.end <= visible.start {
            return;
        }

        let mut dx = 0.0;

        if visible.end < bounds.start {
            dx = bounds.start - visible.end;
        } else if visible.start > bounds.end {
            dx = bounds.end - visible.start;
        }

        if dx != 0.0 {
            let delta_view = -dx * self.zoom;
            self.pan += delta_view;
        }
    }
}

/// Debug snapshot of a [`Viewport1D`] state.
#[derive(Clone, Debug)]
pub struct Viewport1DDebugInfo {
    /// Current view span in device coordinates.
    pub view_span: Range<f64>,
    /// Optional world bounds for clamping and fitting.
    pub world_bounds: Option<Range<f64>>,
    /// World‑space range currently visible through the view.
    pub visible_world_range: Range<f64>,
    /// Current uniform zoom factor.
    pub zoom: f64,
    /// Current pan offset in view coordinates.
    pub pan: f64,
    /// Minimum zoom factor.
    pub min_zoom: f64,
    /// Maximum zoom factor.
    pub max_zoom: f64,
    /// Clamp mode for panning relative to bounds.
    pub clamp_mode: ClampMode,
    /// Fit mode used by [`Viewport1D::fit_world`] / [`Viewport1D::fit_range`].
    pub fit_mode: FitMode,
}

#[cfg(test)]
mod tests {
    use core::ops::Range;

    use kurbo::Point;

    use super::{ClampMode, FitMode, Viewport1D};

    #[test]
    fn world_view_roundtrip_1d() {
        let mut vp = Viewport1D::new(0.0..800.0);
        vp.set_zoom(2.0);
        vp.pan_by_view(10.0);

        let world_x = 123.456;
        let view_x = vp.world_to_view_x(world_x);
        let back = vp.view_to_world_x(view_x);
        assert!((back - world_x).abs() < 1e-9);
    }

    #[test]
    fn zoom_about_anchor_keeps_anchor_fixed_1d() {
        let vp_span = 0.0..800.0;
        let mut vp = Viewport1D::new(vp_span.clone());

        let anchor_view = (vp_span.start + vp_span.end) * 0.5;
        let world_at_anchor_before = vp.view_to_world_x(anchor_view);

        vp.zoom_about_view_point(anchor_view, 3.0);
        let world_at_anchor_after = vp.view_to_world_x(anchor_view);

        assert!((world_at_anchor_after - world_at_anchor_before).abs() < 1e-9);
    }

    #[test]
    fn fit_range_center_and_align_min() {
        let view_span = 0.0..200.0;
        let world_range = -50.0..150.0;
        let mut vp = Viewport1D::new(view_span.clone());

        // Center mode.
        vp.set_fit_mode(FitMode::Center);
        vp.fit_range(world_range.clone());
        let world_center = (world_range.start + world_range.end) * 0.5;
        let view_center = (view_span.start + view_span.end) * 0.5;
        let mapped_center = vp.world_to_view_x(world_center);
        assert!((mapped_center - view_center).abs() < 1e-6);

        // AlignMin mode.
        let mut vp = Viewport1D::new(view_span.clone());
        vp.set_fit_mode(FitMode::AlignMin);
        vp.fit_range(world_range.clone());
        let mapped_min = vp.world_to_view_x(world_range.start);
        assert!((mapped_min - view_span.start).abs() < 1e-6);
    }

    #[test]
    fn clamp_keeps_some_world_visible_1d() {
        let mut vp = Viewport1D::new(0.0..100.0);
        vp.set_clamp_mode(ClampMode::KeepSomeVisible);
        vp.set_world_bounds(Some(0.0..50.0));

        vp.pan_by_view(10_000.0);
        let vis = vp.visible_world_range();
        assert!(vis.end >= 0.0 - 1e-6);
    }

    #[test]
    fn suggest_grid_spacing_and_debug_info_1d() {
        let mut vp = Viewport1D::new(0.0..800.0);
        let base = 0.01;
        let s0 = vp.suggest_grid_spacing(base);
        assert!(s0 >= base);

        vp.set_zoom(10.0);
        let s1 = vp.suggest_grid_spacing(base);
        assert!(s1 <= s0);

        let info = vp.debug_info();
        assert_eq!(
            info.view_span,
            Range {
                start: 0.0,
                end: 800.0
            }
        );
        assert_eq!(info.clamp_mode, ClampMode::KeepSomeVisible);
        assert_eq!(info.fit_mode, FitMode::Center);
    }

    #[test]
    fn view_to_world_point_x_ignores_y_coordinate() {
        let mut vp = Viewport1D::new(0.0..800.0);
        vp.set_zoom(3.0);
        vp.pan_by_view(5.0);

        let world_from_y0 = vp.view_to_world_point_x(Point { x: 100.0, y: 0.0 });
        let world_from_y1 = vp.view_to_world_point_x(Point {
            x: 100.0,
            y: 12345.0,
        });
        assert!((world_from_y0 - world_from_y1).abs() < 1e-9);
    }
}
