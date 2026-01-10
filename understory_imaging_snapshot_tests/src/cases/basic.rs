// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::*;

/// Exercises a representative "basic scene" with a rect clip, group opacity, and a blend mode change.
///
/// This is a quick sanity check that state stacking (clip/group/blend/paint) behaves consistently
/// across backends.
pub(super) struct BasicShapesClipGroup;

impl BasicShapesClipGroup {
    const NAME: &'static str = "basic_shapes_clip_group";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for BasicShapesClipGroup {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(250, 250, 250, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        let red = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(220, 40, 40, 255)),
        });
        let blue = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(40, 80, 220, 255)),
        });

        backend.with_clip_rect(20.0, 20.0, width - 20.0, height - 20.0, |backend| {
            backend.with_composite_layer(None, BlendMode::default(), 0.85, |backend| {
                backend.state(StateOp::SetPaint(red));
                backend.draw(DrawOp::FillRect {
                    x0: 40.0,
                    y0: 40.0,
                    x1: 200.0,
                    y1: 160.0,
                });

                backend.with_blend_layer(BlendMode::from(Mix::Multiply), |backend| {
                    backend.state(StateOp::SetPaint(blue));
                    backend.draw(DrawOp::FillRect {
                        x0: 120.0,
                        y0: 30.0,
                        x1: 280.0,
                        y1: 150.0,
                    });
                });
            });
        });
    }
}
