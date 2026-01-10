// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::*;

/// Exercises nested groups with opacity and blending, plus drawing on top of the group contents.
///
/// This primarily checks that group compositing behaves like an offscreen layer with the specified
/// blend mode and opacity.
pub(super) struct GroupOpacityStack;

impl GroupOpacityStack {
    const NAME: &'static str = "group_opacity_stack";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for GroupOpacityStack {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        // Dark background.
        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(15, 15, 18, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        backend.with_composite_layer(None, BlendMode::default(), 0.75, |backend| {
            let red = backend.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(240, 50, 60, 255)),
            });
            backend.state(StateOp::SetPaint(red));
            backend.draw(DrawOp::FillRect {
                x0: 30.0,
                y0: 30.0,
                x1: 210.0,
                y1: 150.0,
            });

            backend.with_composite_layer(None, BlendMode::from(Mix::Multiply), 0.7, |backend| {
                let blue = backend.create_paint(PaintDesc {
                    brush: Brush::Solid(Color::from_rgba8(60, 120, 255, 255)),
                });
                backend.state(StateOp::SetPaint(blue));
                backend.draw(DrawOp::FillRect {
                    x0: 120.0,
                    y0: 60.0,
                    x1: width - 40.0,
                    y1: 170.0,
                });
            });

            // Stroke on top of group content.
            let stroke = backend.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(240, 240, 240, 255)),
            });
            backend.state(StateOp::SetPaint(stroke));
            backend.state(StateOp::SetStroke(kurbo::Stroke::new(6.0)));
            backend.draw(DrawOp::StrokeRect {
                x0: 25.0,
                y0: 25.0,
                x1: width - 25.0,
                y1: height - 25.0,
            });
        });
    }
}
