// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::*;

/// Exercises `ClipOp::Fill` with both path and rounded-rect clip shapes.
///
/// This is a baseline clip sanity check: clipped content should not bleed outside the clip.
pub(super) struct ClipPathAndRoundedRect;

impl ClipPathAndRoundedRect {
    const NAME: &'static str = "clip_path_and_rounded_rect";
    // Vello GPU can produce small clip AA differences across platforms/drivers.
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 256;
    // Skia has small, platform-dependent AA differences for this case.
    const SKIA_MAX_DIFF_PIXELS: u64 = 512;
}

impl SnapshotCase for ClipPathAndRoundedRect {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn skia_max_diff_pixels(&self) -> u64 {
        Self::SKIA_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        // Star clip.
        let star = backend.create_path(star_path_desc(width * 0.35, height * 0.5, 60.0, 26.0, 5));
        backend.with_clip_path(star, FillRule::NonZero, |backend| {
            // Big gradient behind, clipped.
            gradients::LinearGradientPaintTransform.run(backend, width, height);
        });

        // Rounded-rect clip.
        backend.with_clip_rounded_rect(170.0, 40.0, width - 20.0, height - 20.0, 18.0, |backend| {
            let paint = backend.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(160, 40, 200, 200)),
            });
            backend.state(StateOp::SetPaint(paint));
            backend.draw(DrawOp::FillRect {
                x0: 0.0,
                y0: 0.0,
                x1: width,
                y1: height,
            });
        });
    }
}

/// Exercises `ClipOp::Stroke` for multiple clip shapes and stroke styles.
///
/// This is intended to cover joins, caps, and dash patterns when used to derive a clip outline.
pub(super) struct ClipStrokeDashesAndCaps;

impl ClipStrokeDashesAndCaps {
    const NAME: &'static str = "clip_stroke_dashes_and_caps";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for ClipStrokeDashesAndCaps {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        // Exercise `ClipOp::Stroke` across multiple shapes and stroke styles.

        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(18, 18, 22, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        let star = backend.create_path(star_path_desc(55.0, 45.0, 32.0, 14.0, 5));

        let mut solid_round = kurbo::Stroke::new(14.0);
        solid_round.join = kurbo::Join::Round;
        solid_round.start_cap = kurbo::Cap::Round;
        solid_round.end_cap = kurbo::Cap::Round;

        let mut dashed_square = kurbo::Stroke::new(12.0);
        dashed_square.join = kurbo::Join::Miter;
        dashed_square.start_cap = kurbo::Cap::Square;
        dashed_square.end_cap = kurbo::Cap::Square;
        dashed_square.dash_pattern.push(10.0);
        dashed_square.dash_pattern.push(6.0);
        dashed_square.dash_offset = 1.0;

        let mut dashed_butt = kurbo::Stroke::new(10.0);
        dashed_butt.join = kurbo::Join::Bevel;
        dashed_butt.start_cap = kurbo::Cap::Butt;
        dashed_butt.end_cap = kurbo::Cap::Butt;
        dashed_butt.dash_pattern.push(4.0);
        dashed_butt.dash_pattern.push(4.0);
        dashed_butt.dash_offset = 2.0;

        let fill_paints = [
            backend.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(255, 80, 80, 220)),
            }),
            backend.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(80, 220, 160, 220)),
            }),
            backend.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(80, 160, 255, 220)),
            }),
            backend.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(240, 180, 60, 220)),
            }),
            backend.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(200, 90, 240, 220)),
            }),
            backend.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(100, 220, 240, 220)),
            }),
        ];

        let cell_w = 110.0;
        let cell_h = 90.0;
        let x0 = 10.0;
        let y0 = 10.0;

        let cases: [(f64, f64, ClipShape, kurbo::Stroke); 6] = [
            (
                f64::from(x0),
                f64::from(y0),
                ClipShape::rect(10.0, 10.0, 100.0, 80.0),
                solid_round.clone(),
            ),
            (
                f64::from(x0 + cell_w),
                f64::from(y0),
                ClipShape::rounded_rect(10.0, 10.0, 100.0, 80.0, 16.0),
                solid_round.clone(),
            ),
            (
                f64::from(x0 + 2.0 * cell_w),
                f64::from(y0),
                ClipShape::path(star),
                solid_round,
            ),
            (
                f64::from(x0),
                f64::from(y0 + cell_h),
                ClipShape::rect(10.0, 10.0, 100.0, 80.0),
                dashed_square,
            ),
            (
                f64::from(x0 + cell_w),
                f64::from(y0 + cell_h),
                ClipShape::rounded_rect(10.0, 10.0, 100.0, 80.0, 16.0),
                dashed_butt.clone(),
            ),
            (
                f64::from(x0 + 2.0 * cell_w),
                f64::from(y0 + cell_h),
                ClipShape::path(star),
                dashed_butt,
            ),
        ];

        for (i, (tx, ty, shape, stroke)) in cases.iter().enumerate() {
            backend.state(StateOp::SetTransform(Affine::translate((*tx, *ty))));
            backend.with_clip_stroke(shape.clone(), stroke.clone(), |backend| {
                backend.state(StateOp::SetPaint(fill_paints[i]));
                backend.draw(DrawOp::FillRect {
                    x0: -20.0,
                    y0: -20.0,
                    x1: cell_w + 20.0,
                    y1: cell_h + 20.0,
                });
            });
        }
        backend.state(StateOp::SetTransform(Affine::IDENTITY));
    }
}

/// Verifies rounded-rect clipping for both uniform radii and per-corner radii.
///
/// The two halves should be visibly different, and per-corner radii should be honored.
pub(super) struct ClipRoundedRectPerCorner;

impl ClipRoundedRectPerCorner {
    const NAME: &'static str = "clip_rounded_rect_per_corner";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for ClipRoundedRectPerCorner {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        // Compare uniform-radius rounded-rect clipping vs per-corner radii.
        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(245, 245, 245, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        let paint_a = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(40, 120, 240, 220)),
        });
        let paint_b = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(240, 80, 40, 200)),
        });

        let w = width * 0.5;
        let h = height;

        for (tx, shape) in [
            (
                0.0_f64,
                ClipShape::rounded_rect(20.0, 20.0, w - 20.0, h - 20.0, 22.0),
            ),
            (
                f64::from(w),
                ClipShape::rounded_rect_radii(
                    20.0,
                    20.0,
                    w - 20.0,
                    h - 20.0,
                    (6.0, 24.0, 40.0, 14.0),
                ),
            ),
        ] {
            backend.state(StateOp::SetTransform(Affine::translate((tx, 0.0))));
            backend.with_clip_shape(shape, FillRule::NonZero, |backend| {
                backend.state(StateOp::SetPaint(paint_a));
                backend.draw(DrawOp::FillRect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: w,
                    y1: h,
                });

                backend.state(StateOp::SetPaint(paint_b));
                backend.draw(DrawOp::FillRect {
                    x0: 10.0,
                    y0: 50.0,
                    x1: w - 10.0,
                    y1: h - 10.0,
                });
            });
        }
        backend.state(StateOp::SetTransform(Affine::IDENTITY));
    }
}

/// Verifies that nested clips interact correctly with transforms and group opacity.
///
/// This case is sensitive to clip stack ordering and transform application in clip space.
pub(super) struct ClipNestingTransform;

impl ClipNestingTransform {
    const NAME: &'static str = "clip_nesting_transform";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for ClipNestingTransform {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        // Nested clips + transforms + group opacity.
        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(12, 12, 14, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        let outer = ClipShape::rect(20.0, 20.0, width - 20.0, height - 20.0);
        backend.with_clip_shape(outer, FillRule::NonZero, |backend| {
            backend.with_composite_layer(None, BlendMode::default(), 0.9, |backend| {
                // Rotate around center and apply a path clip in the rotated space.
                let cx = width as f64 * 0.5;
                let cy = height as f64 * 0.5;
                backend.state(StateOp::SetTransform(
                    Affine::translate((cx, cy))
                        * Affine::rotate(0.35)
                        * Affine::translate((-cx, -cy)),
                ));
                let star =
                    backend.create_path(star_path_desc(width * 0.5, height * 0.5, 80.0, 35.0, 5));
                backend.with_clip_path(star, FillRule::NonZero, |backend| {
                    // Draw two big rects to make the transformed clip obvious.
                    let a = backend.create_paint(PaintDesc {
                        brush: Brush::Solid(Color::from_rgba8(60, 180, 255, 200)),
                    });
                    let b = backend.create_paint(PaintDesc {
                        brush: Brush::Solid(Color::from_rgba8(255, 120, 60, 200)),
                    });
                    backend.state(StateOp::SetPaint(a));
                    backend.draw(DrawOp::FillRect {
                        x0: -200.0,
                        y0: -200.0,
                        x1: width + 200.0,
                        y1: height * 0.7,
                    });
                    backend.state(StateOp::SetPaint(b));
                    backend.draw(DrawOp::FillRect {
                        x0: -200.0,
                        y0: height * 0.3,
                        x1: width + 200.0,
                        y1: height + 200.0,
                    });
                });
                backend.state(StateOp::SetTransform(Affine::IDENTITY));
            });
        });
    }
}
