// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use kurbo::{Affine, Point, Rect, Vec2};

use crate::modes::{ClampMode, FitMode};

/// 2D viewport over a world‑space plane.
///
/// `Viewport2D` tracks a rectangular region in device/view space and a
/// uniform pan+zoom transform mapping world coordinates into that region.
/// It can be used to:
/// - Convert points and rectangles between world and view coordinates.
/// - Pan and zoom around a chosen anchor point.
/// - Fit the entire world bounds (or a sub‑rect) into the view.
#[derive(Clone, Debug)]
pub struct Viewport2D {
    view_rect: Rect,
    world_bounds: Option<Rect>,
    zoom: f64,
    pan: Vec2,
    min_zoom: f64,
    max_zoom: f64,
    clamp_mode: ClampMode,
    fit_mode: FitMode,
    world_to_view: Affine,
    view_to_world: Affine,
}

impl Viewport2D {
    /// Creates a new viewport covering `view_rect` with default zoom and clamping.
    ///
    /// - Initial zoom is `1.0`.
    /// - Initial pan is zero (world origin maps to the view rect origin).
    /// - Zoom is clamped to the range `[1e-3, 1e3]` by default.
    #[must_use]
    pub fn new(view_rect: Rect) -> Self {
        let mut vp = Self {
            view_rect,
            world_bounds: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            min_zoom: 1e-3,
            max_zoom: 1e3,
            clamp_mode: ClampMode::default(),
            fit_mode: FitMode::default(),
            world_to_view: Affine::IDENTITY,
            view_to_world: Affine::IDENTITY,
        };
        vp.rebuild_transforms();
        vp
    }

    /// Returns the current view rectangle in device coordinates.
    #[must_use]
    pub fn view_rect(&self) -> Rect {
        self.view_rect
    }

    /// Sets the view rectangle in device coordinates.
    ///
    /// This does not change zoom or pan, but it may affect the visible world
    /// region. Transforms are rebuilt to account for the new rect.
    pub fn set_view_rect(&mut self, rect: Rect) {
        if self.view_rect == rect {
            return;
        }
        self.view_rect = rect;
        self.rebuild_transforms();
        self.clamp_to_bounds();
    }

    /// Sets optional world bounds used for clamping and view fitting.
    pub fn set_world_bounds(&mut self, bounds: Option<Rect>) {
        if self.world_bounds == bounds {
            return;
        }
        self.world_bounds = bounds;
        self.clamp_to_bounds();
    }

    /// Returns the current world bounds, if any.
    #[must_use]
    pub fn world_bounds(&self) -> Option<Rect> {
        self.world_bounds
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

    /// Sets how fitted content should be positioned inside the view rect.
    pub fn set_fit_mode(&mut self, mode: FitMode) {
        self.fit_mode = mode;
    }

    /// Returns the current fit mode.
    #[must_use]
    pub fn fit_mode(&self) -> FitMode {
        self.fit_mode
    }

    /// Sets the zoom factor, clamping it into the configured zoom range.
    pub fn set_zoom(&mut self, zoom: f64) {
        let clamped = zoom.clamp(self.min_zoom, self.max_zoom);
        if (self.zoom - clamped).abs() < f64::EPSILON {
            return;
        }
        self.zoom = clamped;
        self.rebuild_transforms();
        self.clamp_to_bounds();
    }

    /// Pans the view by a delta in view/device space.
    ///
    /// This adjusts the pan offset and then applies clamping relative to world
    /// bounds if configured.
    pub fn pan_by_view(&mut self, delta: Vec2) {
        if delta == Vec2::ZERO {
            return;
        }
        self.pan += delta;
        self.rebuild_transforms();
        self.clamp_to_bounds();
    }

    /// Zooms around a given anchor point in view/device coordinates.
    ///
    /// The anchor point remains fixed in view space as much as possible under
    /// the new zoom level.
    pub fn zoom_about_view_point(&mut self, anchor_view: Point, factor: f64) {
        if factor <= 0.0 {
            return;
        }
        let old_zoom = self.zoom;
        let new_zoom = (old_zoom * factor).clamp(self.min_zoom, self.max_zoom);
        if (new_zoom - old_zoom).abs() < f64::EPSILON {
            return;
        }

        let old_world = self.view_to_world_point(anchor_view);
        self.zoom = new_zoom;
        self.rebuild_transforms();
        let new_anchor_view = self.world_to_view_point(old_world);
        let delta_view = anchor_view - new_anchor_view;
        self.pan_by_view(delta_view);
    }

    /// Fits the entire world bounds into the view, preserving aspect ratio.
    ///
    /// If no world bounds are set, this is a no‑op.
    pub fn fit_world(&mut self) {
        if let Some(bounds) = self.world_bounds {
            self.fit_rect(bounds);
        }
    }

    /// Fits the given world‑space rectangle into the view, preserving aspect ratio.
    pub fn fit_rect(&mut self, rect: Rect) {
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        let view_size = self.view_rect.size();
        if view_size.width <= 0.0 || view_size.height <= 0.0 {
            return;
        }

        let sx = view_size.width / rect.width().max(f64::MIN_POSITIVE);
        let sy = view_size.height / rect.height().max(f64::MIN_POSITIVE);
        let target_zoom = sx.min(sy);

        let zoom = target_zoom.clamp(self.min_zoom, self.max_zoom);
        self.zoom = zoom;

        // Choose pan based on fit mode so that either the content is centered
        // or its minimum corner aligns with the view origin.
        let view_origin = self.view_rect.origin().to_vec2();
        self.pan = match self.fit_mode {
            FitMode::Center => {
                let view_center = self.view_rect.center().to_vec2();
                let world_center = rect.center().to_vec2();
                view_center - view_origin - world_center * zoom
            }
            FitMode::AlignMin => {
                let world_min = rect.origin().to_vec2();
                -world_min * zoom
            }
        };

        self.rebuild_transforms();
        self.clamp_to_bounds();
    }

    /// Centers the view on the given world‑space point.
    pub fn center_on(&mut self, world_pt: Point) {
        let view_center = self.view_rect.center();
        let world_in_view = self.world_to_view_point(world_pt);
        let delta = view_center - world_in_view;
        self.pan_by_view(delta);
    }

    /// Returns the visible world‑space rectangle.
    #[must_use]
    pub fn visible_world_rect(&self) -> Rect {
        self.view_to_world_rect(self.view_rect)
    }

    /// Converts a world‑space point into view/device coordinates.
    #[must_use]
    pub fn world_to_view_point(&self, pt: Point) -> Point {
        self.world_to_view * pt
    }

    /// Converts a view/device‑space point into world coordinates.
    #[must_use]
    pub fn view_to_world_point(&self, pt: Point) -> Point {
        self.view_to_world * pt
    }

    /// Converts a world‑space rectangle into view/device coordinates.
    #[must_use]
    pub fn world_to_view_rect(&self, rect: Rect) -> Rect {
        // Transform the four corners and take their bounding box. This is
        // sufficient for the axis‑aligned, uniform zoom transform used here.
        let p0 = rect.origin();
        let p1 = Point::new(rect.max_x(), rect.y0);
        let p2 = Point::new(rect.x0, rect.max_y());
        let p3 = Point::new(rect.max_x(), rect.max_y());
        let q0 = self.world_to_view * p0;
        let q1 = self.world_to_view * p1;
        let q2 = self.world_to_view * p2;
        let q3 = self.world_to_view * p3;
        let min_x = q0.x.min(q1.x).min(q2.x).min(q3.x);
        let min_y = q0.y.min(q1.y).min(q2.y).min(q3.y);
        let max_x = q0.x.max(q1.x).max(q2.x).max(q3.x);
        let max_y = q0.y.max(q1.y).max(q2.y).max(q3.y);
        Rect::new(min_x, min_y, max_x, max_y)
    }

    /// Converts a view/device‑space rectangle into world coordinates.
    #[must_use]
    pub fn view_to_world_rect(&self, rect: Rect) -> Rect {
        let p0 = rect.origin();
        let p1 = Point::new(rect.max_x(), rect.y0);
        let p2 = Point::new(rect.x0, rect.max_y());
        let p3 = Point::new(rect.max_x(), rect.max_y());
        let q0 = self.view_to_world * p0;
        let q1 = self.view_to_world * p1;
        let q2 = self.view_to_world * p2;
        let q3 = self.view_to_world * p3;
        let min_x = q0.x.min(q1.x).min(q2.x).min(q3.x);
        let min_y = q0.y.min(q1.y).min(q2.y).min(q3.y);
        let max_x = q0.x.max(q1.x).max(q2.x).max(q3.x);
        let max_y = q0.y.max(q1.y).max(q2.y).max(q3.y);
        Rect::new(min_x, min_y, max_x, max_y)
    }

    /// Returns the current world‑units‑per‑pixel ratio at the view center.
    ///
    /// This is `1.0 / zoom` for the axis‑aligned, uniform zoom model used
    /// by this crate and can be used to choose grid spacing or stroke
    /// thickness in world units.
    #[must_use]
    pub fn world_units_per_pixel(&self) -> f64 {
        1.0 / self.zoom
    }

    /// Returns the current world‑units‑per‑pixel ratio along the X axis.
    ///
    /// For the uniform zoom model used by this crate, this is identical
    /// to [`Viewport2D::world_units_per_pixel`].
    #[must_use]
    pub fn world_units_per_pixel_x(&self) -> f64 {
        self.world_units_per_pixel()
    }

    /// Returns the current world‑units‑per‑pixel ratio along the Y axis.
    ///
    /// For the uniform zoom model used by this crate, this is identical
    /// to [`Viewport2D::world_units_per_pixel`].
    #[must_use]
    pub fn world_units_per_pixel_y(&self) -> f64 {
        self.world_units_per_pixel()
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

    /// Snapshot of the current viewport state for debugging and inspection.
    #[must_use]
    pub fn debug_info(&self) -> Viewport2DDebugInfo {
        Viewport2DDebugInfo {
            view_rect: self.view_rect,
            world_bounds: self.world_bounds,
            visible_world_rect: self.visible_world_rect(),
            zoom: self.zoom,
            pan: self.pan,
            min_zoom: self.min_zoom,
            max_zoom: self.max_zoom,
            clamp_mode: self.clamp_mode,
            fit_mode: self.fit_mode,
        }
    }

    fn rebuild_transforms(&mut self) {
        let view_origin = self.view_rect.origin().to_vec2();
        let scale = self.zoom;
        // World → view: translate by pan, then scale, then translate into view rect.
        self.world_to_view = Affine::translate(view_origin + self.pan) * Affine::scale(scale);
        self.view_to_world = self.world_to_view.inverse();
    }

    fn clamp_to_bounds(&mut self) {
        if self.clamp_mode == ClampMode::None {
            return;
        }
        let bounds = match self.world_bounds {
            Some(b) if b.width() > 0.0 && b.height() > 0.0 => b,
            _ => return,
        };

        // Current visible world rect; we will adjust pan to keep at least some
        // overlap with `bounds`.
        let visible = self.visible_world_rect();
        if visible.width() <= 0.0 || visible.height() <= 0.0 {
            return;
        }

        let mut dx = 0.0;
        let mut dy = 0.0;

        // Horizontal clamping: keep some overlap between visible rect and bounds.
        if visible.max_x() < bounds.min_x() {
            dx = bounds.min_x() - visible.max_x();
        } else if visible.min_x() > bounds.max_x() {
            dx = bounds.max_x() - visible.min_x();
        }

        // Vertical clamping.
        if visible.max_y() < bounds.min_y() {
            dy = bounds.min_y() - visible.max_y();
        } else if visible.min_y() > bounds.max_y() {
            dy = bounds.max_y() - visible.min_y();
        }

        if dx != 0.0 || dy != 0.0 {
            // Adjust pan so that the visible world rect moves by `dx`/`dy`
            // in world space. Increasing pan moves the world in the negative
            // direction, so we need to negate.
            let scale = self.zoom;
            let delta_view = Vec2::new(-dx * scale, -dy * scale);
            self.pan += delta_view;
            self.rebuild_transforms();
        }
    }
}

/// Debug snapshot of a [`Viewport2D`] state.
#[derive(Clone, Copy, Debug)]
pub struct Viewport2DDebugInfo {
    /// Current view rectangle in device coordinates.
    pub view_rect: Rect,
    /// Optional world bounds for clamping and fitting.
    pub world_bounds: Option<Rect>,
    /// World‑space rectangle currently visible through the view.
    pub visible_world_rect: Rect,
    /// Current uniform zoom factor.
    pub zoom: f64,
    /// Current pan offset in view coordinates.
    pub pan: Vec2,
    /// Minimum zoom factor.
    pub min_zoom: f64,
    /// Maximum zoom factor.
    pub max_zoom: f64,
    /// Clamp mode for panning relative to bounds.
    pub clamp_mode: ClampMode,
    /// Fit mode used by [`Viewport2D::fit_world`] / [`Viewport2D::fit_rect`].
    pub fit_mode: FitMode,
}

#[cfg(test)]
mod tests {
    use kurbo::{Point, Rect};

    use super::{ClampMode, FitMode, Viewport2D};

    #[test]
    fn basic_world_view_roundtrip() {
        let view_rect = Rect::new(0.0, 0.0, 800.0, 600.0);
        let vp = Viewport2D::new(view_rect);

        let world_pt = Point::new(10.0, -5.0);
        let view_pt = vp.world_to_view_point(world_pt);
        let world_back = vp.view_to_world_point(view_pt);
        assert!((world_back.x - world_pt.x).abs() < 1e-9);
        assert!((world_back.y - world_pt.y).abs() < 1e-9);
    }

    #[test]
    fn zoom_about_anchor_keeps_anchor_fixed() {
        let view_rect = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut vp = Viewport2D::new(view_rect);

        // Choose an anchor at the center of the view.
        let anchor_view = view_rect.center();
        let world_at_anchor_before = vp.view_to_world_point(anchor_view);

        vp.zoom_about_view_point(anchor_view, 2.0);
        let world_at_anchor_after = vp.view_to_world_point(anchor_view);

        assert!((world_at_anchor_after.x - world_at_anchor_before.x).abs() < 1e-9);
        assert!((world_at_anchor_after.y - world_at_anchor_before.y).abs() < 1e-9);
    }

    #[test]
    fn fit_world_respects_bounds_and_aspect_ratio() {
        let view_rect = Rect::new(0.0, 0.0, 200.0, 100.0);
        let mut vp = Viewport2D::new(view_rect);

        let world_bounds = Rect::new(-50.0, -25.0, 50.0, 25.0);
        vp.set_world_bounds(Some(world_bounds));
        vp.fit_world();

        let visible = vp.visible_world_rect();
        // The world bounds should be fully visible; allow tiny numeric slack.
        assert!(visible.min_x() <= world_bounds.min_x() + 1e-9);
        assert!(visible.max_x() >= world_bounds.max_x() - 1e-9);
        assert!(visible.min_y() <= world_bounds.min_y() + 1e-9);
        assert!(visible.max_y() >= world_bounds.max_y() - 1e-9);
    }

    #[test]
    fn fit_mode_align_min_aligns_world_min_to_view_origin() {
        let view_rect = Rect::new(0.0, 0.0, 200.0, 100.0);
        let mut vp = Viewport2D::new(view_rect);
        vp.set_fit_mode(FitMode::AlignMin);

        let world_bounds = Rect::new(-50.0, -20.0, 150.0, 80.0);
        vp.set_world_bounds(Some(world_bounds));
        vp.fit_world();

        // World min corner should map close to the view origin.
        let view_origin_world = vp.world_to_view_point(world_bounds.origin());
        let origin = view_rect.origin();
        assert!((view_origin_world.x - origin.x).abs() < 1e-6);
        assert!((view_origin_world.y - origin.y).abs() < 1e-6);
    }

    #[test]
    fn extend_and_fit_preserve_anchor_and_clamp() {
        let view_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut vp = Viewport2D::new(view_rect);
        vp.set_clamp_mode(ClampMode::KeepSomeVisible);

        let bounds = Rect::new(0.0, 0.0, 50.0, 50.0);
        vp.set_world_bounds(Some(bounds));
        vp.fit_world();

        // Attempt to pan far away; clamping should pull the view back so that
        // the visible rect still overlaps the bounds.
        vp.pan_by_view((1000.0, 1000.0).into());
        let visible = vp.visible_world_rect();

        assert!(visible.max_x() >= bounds.min_x() - 1e-6);
        assert!(visible.max_y() >= bounds.min_y() - 1e-6);
    }

    #[test]
    fn suggest_grid_spacing_and_debug_info_2d() {
        let view_rect = Rect::new(0.0, 0.0, 400.0, 300.0);
        let mut vp = Viewport2D::new(view_rect);

        let base = 0.01;
        let s0 = vp.suggest_grid_spacing(base);
        assert!(s0 >= base);

        // Zoom in: world units per pixel get smaller, so suggested spacing should
        // also get smaller or stay the same.
        vp.set_zoom(10.0);
        let s1 = vp.suggest_grid_spacing(base);
        assert!(s1 <= s0);

        // Zoom out: suggested spacing should grow.
        vp.set_zoom(0.1);
        let s2 = vp.suggest_grid_spacing(base);
        assert!(s2 >= s1);

        let info = vp.debug_info();
        assert_eq!(info.view_rect, view_rect);
        assert_eq!(info.clamp_mode, ClampMode::KeepSomeVisible);
        assert!(info.min_zoom <= info.max_zoom);
    }
}
