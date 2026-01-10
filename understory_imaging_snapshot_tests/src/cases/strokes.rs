// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::*;

/// Exercises stroke rendering styles (width, join, cap) and transform interaction.
///
/// This checks that stroke styling is applied consistently across backends.
pub(super) struct StrokeStyles;

impl StrokeStyles {
    const NAME: &'static str = "stroke_styles";
    // Vello GPU can produce small stroke AA differences across platforms/drivers.
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 256;
}

impl SnapshotCase for StrokeStyles {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, _width: f32, _height: f32) {
        let path = backend.create_path(rect_path_desc(30.0, 30.0, 140.0, 120.0));

        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(40, 120, 240, 255)),
        });
        backend.state(StateOp::SetPaint(paint));

        // Thick stroke with round joins/caps.
        let mut stroke = kurbo::Stroke::new(14.0);
        stroke.join = kurbo::Join::Round;
        stroke.start_cap = kurbo::Cap::Round;
        stroke.end_cap = kurbo::Cap::Round;
        backend.state(StateOp::SetStroke(stroke));
        backend.draw(DrawOp::StrokePath(path));

        // Thin stroke with miter joins/caps and a transform.
        let path2 = backend.create_path(rect_path_desc(170.0, 50.0, 290.0, 140.0));
        let paint2 = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(240, 80, 40, 255)),
        });
        backend.state(StateOp::SetPaint(paint2));
        let mut stroke2 = kurbo::Stroke::new(6.0);
        stroke2.join = kurbo::Join::Miter;
        stroke2.start_cap = kurbo::Cap::Square;
        stroke2.end_cap = kurbo::Cap::Square;
        backend.state(StateOp::SetStroke(stroke2));
        backend.state(StateOp::SetTransform(
            Affine::translate((0.0, 0.0)) * Affine::rotate(0.05),
        ));
        backend.draw(DrawOp::StrokePath(path2));
        backend.state(StateOp::SetTransform(Affine::IDENTITY));
    }
}
