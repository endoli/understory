// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::*;

/// Verifies that `StateOp::SetPaintTransform` affects linear gradient evaluation.
///
/// The gradient should appear rotated/skewed relative to the axis-aligned rectangle.
pub(super) struct LinearGradientPaintTransform;

impl LinearGradientPaintTransform {
    const NAME: &'static str = "linear_gradient_paint_transform";
    // Vello GPU can produce small AA/gradient differences across platforms/drivers.
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 512;
}

impl SnapshotCase for LinearGradientPaintTransform {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        let stops = vec![
            ColorStop::from((0.0, Color::from_rgba8(255, 0, 0, 255))),
            ColorStop::from((0.5, Color::from_rgba8(0, 255, 0, 200))),
            ColorStop::from((1.0, Color::from_rgba8(0, 0, 255, 255))),
        ];
        let kind =
            GradientKind::Linear(LinearGradientPosition::new((0.0, 0.0), (width as f64, 0.0)));
        let brush = Brush::Gradient(Gradient {
            kind,
            extend: Extend::Pad,
            stops: stops.as_slice().into(),
            ..Gradient::default()
        });
        let paint = backend.create_paint(PaintDesc { brush });

        backend.state(StateOp::SetPaint(paint));
        backend.state(StateOp::SetPaintTransform(Affine::rotate(
            std::f64::consts::FRAC_PI_8,
        )));
        backend.draw(DrawOp::FillRect {
            x0: 20.0,
            y0: 20.0,
            x1: width - 20.0,
            y1: height - 20.0,
        });
    }
}

/// Verifies that `StateOp::SetPaintTransform` affects radial gradient evaluation.
///
/// The gradient should appear transformed (scaled/rotated) without affecting the geometry.
pub(super) struct RadialGradientPaintTransform;

impl RadialGradientPaintTransform {
    const NAME: &'static str = "radial_gradient_paint_transform";
    // Vello GPU can produce small AA/gradient differences across platforms/drivers.
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 256;
}

impl SnapshotCase for RadialGradientPaintTransform {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        let stops = vec![
            ColorStop::from((0.0, Color::from_rgba8(255, 220, 0, 255))),
            ColorStop::from((0.6, Color::from_rgba8(0, 200, 255, 200))),
            ColorStop::from((1.0, Color::from_rgba8(120, 0, 200, 255))),
        ];
        let kind = GradientKind::Radial(RadialGradientPosition::new(
            (width as f64 * 0.5, height as f64 * 0.5),
            width.min(height) * 0.45,
        ));
        let brush = Brush::Gradient(Gradient {
            kind,
            extend: Extend::Pad,
            stops: stops.as_slice().into(),
            ..Gradient::default()
        });
        let paint = backend.create_paint(PaintDesc { brush });

        backend.state(StateOp::SetPaint(paint));
        backend.state(StateOp::SetPaintTransform(
            Affine::translate((width as f64 * 0.5, height as f64 * 0.5))
                * Affine::scale_non_uniform(1.2, 0.8)
                * Affine::rotate(std::f64::consts::FRAC_PI_6)
                * Affine::translate((-width as f64 * 0.5, -height as f64 * 0.5)),
        ));
        backend.draw(DrawOp::FillRect {
            x0: 20.0,
            y0: 20.0,
            x1: width - 20.0,
            y1: height - 20.0,
        });
        backend.state(StateOp::SetPaintTransform(Affine::IDENTITY));
    }
}
