// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use peniko::{Brush, Color};
use understory_imaging::{
    DrawOp, FillRule, FilterDesc, ImagingBackend, ImagingBackendExt, PaintDesc, PathCmd, PathDesc,
    StateOp,
};

use super::SnapshotCase;
use super::star_path_desc;

/// Verifies that layer filters are applied when compositing the layer into its parent.
///
/// This currently runs only on the Skia backend; other backends may ignore filters.
pub(super) struct FilterBlurLayer;

impl SnapshotCase for FilterBlurLayer {
    fn name(&self) -> &'static str {
        "filter_blur_layer"
    }

    fn supports_backend(&self, backend: &str) -> bool {
        backend == "skia" || backend == "vello_cpu"
    }

    fn run(&self, backend: &mut dyn ImagingBackend, _width: f32, _height: f32) {
        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::WHITE),
        });
        let black = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 0, 0, 255)),
        });
        let blue = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 90, 220, 255)),
        });
        let red = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(220, 40, 40, 255)),
        });

        // Background.
        backend.state(StateOp::SetPaint(paint));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: 320.0,
            y1: 200.0,
        });

        // First draw a high-frequency pattern inside a blur-filtered layer, then draw a crisp
        // shape on top (outside the filter layer) so the blur is obvious.
        backend.with_filter_layer(FilterDesc::blur(6.0), |backend| {
            // Multi-color "barcode" pattern that should smear when blurred.
            let x0 = 80.0;
            let y0 = 60.0;
            let x1 = 240.0;
            let y1 = 140.0;

            let mut i: u32 = 0;
            let mut x = x0;
            while x < x1 {
                let paint = if i.is_multiple_of(2) { blue } else { red };
                backend.state(StateOp::SetPaint(paint));
                backend.draw(DrawOp::FillRect {
                    x0: x,
                    y0,
                    x1: (x + 8.0).min(x1),
                    y1,
                });
                i += 1;
                x += 12.0;
            }

            // Add horizontal bars too, so the pattern stays recognizable after blur.
            backend.state(StateOp::SetPaint(black));
            let mut y = y0 + 8.0;
            while y < y1 {
                backend.draw(DrawOp::FillRect {
                    x0,
                    y0: y,
                    x1,
                    y1: (y + 2.0).min(y1),
                });
                y += 16.0;
            }
        });

        // Crisp overlay on top: should remain sharp while the background bars are blurred.
        let overlay = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::BLACK),
        });
        backend.state(StateOp::SetPaint(overlay));

        // Big corner marks + a few sharp bars to make the comparison obvious.
        backend.draw(DrawOp::FillRect {
            x0: 76.0,
            y0: 56.0,
            x1: 244.0,
            y1: 60.0,
        });
        backend.draw(DrawOp::FillRect {
            x0: 76.0,
            y0: 56.0,
            x1: 80.0,
            y1: 144.0,
        });
        backend.draw(DrawOp::FillRect {
            x0: 240.0,
            y0: 56.0,
            x1: 244.0,
            y1: 144.0,
        });
        backend.draw(DrawOp::FillRect {
            x0: 76.0,
            y0: 140.0,
            x1: 244.0,
            y1: 144.0,
        });

        // A small set of crisp barcode bars over the blurred ones.
        for i in 0..4 {
            let x0 = 92.0 + (i as f32) * 18.0;
            backend.draw(DrawOp::FillRect {
                x0,
                y0: 72.0,
                x1: x0 + 4.0,
                y1: 128.0,
            });
        }
    }
}

/// Verifies that drop-shadow filters are applied as a layer effect.
pub(super) struct FilterDropShadowLayer;

impl SnapshotCase for FilterDropShadowLayer {
    fn name(&self) -> &'static str {
        "filter_drop_shadow_layer"
    }

    fn skia_max_diff_pixels(&self) -> u64 {
        // Skia's drop-shadow filter output can differ slightly across platforms/toolchains.
        500
    }

    fn supports_backend(&self, backend: &str) -> bool {
        backend == "skia" || backend == "vello_cpu"
    }

    fn run(&self, backend: &mut dyn ImagingBackend, _width: f32, _height: f32) {
        // Background.
        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::WHITE),
        });
        backend.state(StateOp::SetPaint(paint));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: 320.0,
            y1: 200.0,
        });

        let navy = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(10, 35, 70, 255)),
        });
        let teal = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 170, 160, 255)),
        });
        let black = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 0, 0, 255)),
        });

        // Star + swoosh inside the filtered layer.
        let star = backend.create_path(star_path_desc(150.0, 95.0, 52.0, 22.0, 7));
        let swoosh = backend.create_path(PathDesc {
            commands: Box::new([
                PathCmd::MoveTo { x: 68.0, y: 138.0 },
                PathCmd::CurveTo {
                    x1: 110.0,
                    y1: 112.0,
                    x2: 170.0,
                    y2: 162.0,
                    x: 250.0,
                    y: 122.0,
                },
                PathCmd::CurveTo {
                    x1: 210.0,
                    y1: 126.0,
                    x2: 160.0,
                    y2: 110.0,
                    x: 120.0,
                    y: 130.0,
                },
            ]),
        });

        backend.with_filter_layer(
            FilterDesc::DropShadow {
                dx: 12.0,
                dy: 10.0,
                std_deviation_x: 6.0,
                std_deviation_y: 6.0,
                color: Color::from_rgba8(0, 0, 0, 140),
            },
            |backend| {
                backend.state(StateOp::SetPaint(navy));
                backend.draw(DrawOp::FillPath(star));

                backend.state(StateOp::SetPaint(teal));
                backend.state(StateOp::SetStroke(kurbo::Stroke::new(8.0)));
                backend.draw(DrawOp::StrokePath(swoosh));
            },
        );

        // Crisp, unfiltered mark to make the shadow offset obvious.
        let mark = backend.create_path(PathDesc {
            commands: Box::new([
                PathCmd::MoveTo { x: 270.0, y: 40.0 },
                PathCmd::LineTo { x: 298.0, y: 68.0 },
                PathCmd::MoveTo { x: 298.0, y: 40.0 },
                PathCmd::LineTo { x: 270.0, y: 68.0 },
            ]),
        });
        backend.state(StateOp::SetPaint(black));
        backend.state(StateOp::SetStroke(kurbo::Stroke::new(5.0)));
        backend.draw(DrawOp::StrokePath(mark));
    }
}

/// Verifies that offset filters translate a layer as a layer effect.
pub(super) struct FilterOffsetLayer;

impl SnapshotCase for FilterOffsetLayer {
    fn name(&self) -> &'static str {
        "filter_offset_layer"
    }

    fn skia_max_diff_pixels(&self) -> u64 {
        // Skia's filter output can differ slightly across platforms/toolchains.
        64
    }

    fn supports_backend(&self, backend: &str) -> bool {
        // `vello_cpu` does not support offset yet; Skia does.
        backend == "skia"
    }

    fn run(&self, backend: &mut dyn ImagingBackend, _width: f32, _height: f32) {
        // Background.
        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::WHITE),
        });
        backend.state(StateOp::SetPaint(paint));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: 320.0,
            y1: 200.0,
        });

        let navy = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(10, 35, 70, 255)),
        });
        let teal = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 170, 160, 255)),
        });
        let red = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(220, 40, 40, 255)),
        });

        let star = backend.create_path(star_path_desc(150.0, 95.0, 52.0, 22.0, 7));
        let swoosh = backend.create_path(PathDesc {
            commands: Box::new([
                PathCmd::MoveTo { x: 68.0, y: 138.0 },
                PathCmd::CurveTo {
                    x1: 110.0,
                    y1: 112.0,
                    x2: 170.0,
                    y2: 162.0,
                    x: 250.0,
                    y: 122.0,
                },
                PathCmd::CurveTo {
                    x1: 210.0,
                    y1: 126.0,
                    x2: 160.0,
                    y2: 110.0,
                    x: 120.0,
                    y: 130.0,
                },
            ]),
        });

        // Reference outline at the original position.
        backend.state(StateOp::SetPaint(red));
        backend.state(StateOp::SetStroke(kurbo::Stroke::new(3.0)));
        backend.draw(DrawOp::StrokePath(star));

        // Offset-filtered layer.
        backend.with_filter_layer(FilterDesc::offset(18.0, -12.0), |backend| {
            backend.state(StateOp::SetPaint(navy));
            backend.draw(DrawOp::FillPath(star));

            backend.state(StateOp::SetPaint(teal));
            backend.state(StateOp::SetStroke(kurbo::Stroke::new(8.0)));
            backend.draw(DrawOp::StrokePath(swoosh));
        });
    }
}

/// Verifies that flood filters replace a layer with a solid color.
pub(super) struct FilterFloodLayer;

impl SnapshotCase for FilterFloodLayer {
    fn name(&self) -> &'static str {
        "filter_flood_layer"
    }

    fn skia_max_diff_pixels(&self) -> u64 {
        // Skia's shader-image-filter output can differ slightly across platforms/toolchains.
        256
    }

    fn supports_backend(&self, backend: &str) -> bool {
        backend == "skia" || backend == "vello_cpu"
    }

    fn run(&self, backend: &mut dyn ImagingBackend, _width: f32, _height: f32) {
        // Background.
        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::WHITE),
        });
        backend.state(StateOp::SetPaint(paint));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: 320.0,
            y1: 200.0,
        });

        // Draw some content that should be ignored by Flood.
        let star = backend.create_path(star_path_desc(150.0, 95.0, 52.0, 22.0, 7));
        let teal = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 170, 160, 255)),
        });
        backend.state(StateOp::SetPaint(teal));
        backend.draw(DrawOp::FillPath(star));

        // Flood layer: clip defines the flood region; source content should be ignored.
        let blob_clip = backend.create_path(PathDesc {
            commands: vec![
                PathCmd::MoveTo { x: 150.0, y: 40.0 },
                PathCmd::LineTo { x: 210.0, y: 70.0 },
                PathCmd::LineTo { x: 235.0, y: 110.0 },
                PathCmd::LineTo { x: 210.0, y: 150.0 },
                PathCmd::LineTo { x: 150.0, y: 180.0 },
                PathCmd::LineTo { x: 90.0, y: 150.0 },
                PathCmd::LineTo { x: 65.0, y: 110.0 },
                PathCmd::LineTo { x: 90.0, y: 70.0 },
                PathCmd::Close,
            ]
            .into(),
        });

        backend.with_clip_path(blob_clip, FillRule::NonZero, |backend| {
            backend.with_filter_layer(
                FilterDesc::flood(Color::from_rgba8(220, 40, 40, 200)),
                |b| {
                    let black = b.create_paint(PaintDesc {
                        brush: Brush::Solid(Color::BLACK),
                    });
                    b.state(StateOp::SetPaint(black));
                    b.draw(DrawOp::FillRect {
                        x0: 60.0,
                        y0: 50.0,
                        x1: 240.0,
                        y1: 170.0,
                    });
                },
            );
        });
    }
}
