// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::*;

/// Verifies that recorded pictures can be replayed multiple times with different transforms.
///
/// This should render identically (modulo backend tolerances) each time the same picture is drawn.
pub(super) struct PictureRecordingReuse;

impl PictureRecordingReuse {
    const NAME: &'static str = "picture_recording_reuse";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for PictureRecordingReuse {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, _width: f32, _height: f32) {
        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(30, 30, 30, 255)),
        });
        backend.state(StateOp::SetPaint(paint));

        let picture = record_picture(backend, |b| {
            let p1 = b.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(255, 0, 0, 200)),
            });
            let p2 = b.create_paint(PaintDesc {
                brush: Brush::Solid(Color::from_rgba8(0, 0, 255, 200)),
            });
            b.with_blend_layer(BlendMode::from(Mix::Screen), |b| {
                b.state(StateOp::SetPaint(p1));
                b.draw(DrawOp::FillRect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 120.0,
                    y1: 80.0,
                });
            });
            b.with_blend_layer(BlendMode::from(Mix::Screen), |b| {
                b.state(StateOp::SetPaint(p2));
                b.draw(DrawOp::FillRect {
                    x0: 40.0,
                    y0: 20.0,
                    x1: 160.0,
                    y1: 100.0,
                });
            });
        });

        backend.draw(DrawOp::DrawPicture {
            picture,
            transform: Affine::translate((40.0, 40.0)),
        });
        backend.draw(DrawOp::DrawPicture {
            picture,
            transform: Affine::translate((140.0, 80.0)) * Affine::scale(0.8),
        });
    }
}

/// Verifies that picture recording captures stateful operations like clips, groups, and image draws.
///
/// This specifically checks that the recorded picture replays its internal clip/group state in the
/// correct order and doesn't leak state to the caller.
pub(super) struct PictureWithImagesAndState;

impl PictureWithImagesAndState {
    const NAME: &'static str = "picture_with_images_and_state";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for PictureWithImagesAndState {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 0, 0, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        let image = alpha_edge_image(backend, 80, 60);
        let overlay = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(255, 255, 255, 80)),
        });

        let picture = record_picture(backend, |b| {
            b.with_clip_rounded_rect(0.0, 0.0, 110.0, 80.0, 14.0, |b| {
                b.draw(DrawOp::DrawImage {
                    image,
                    transform: Affine::translate((10.0, 10.0)),
                    sampler: ImageSampler::default(),
                });
            });
            b.with_composite_layer(None, BlendMode::from(Mix::Screen), 0.9, |b| {
                b.state(StateOp::SetPaint(overlay));
                b.draw(DrawOp::FillRect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 110.0,
                    y1: 80.0,
                });
            });
        });

        backend.draw(DrawOp::DrawPicture {
            picture,
            transform: Affine::translate((40.0, 40.0)),
        });
        backend.draw(DrawOp::DrawPicture {
            picture,
            transform: Affine::translate((190.0, 70.0)) * Affine::scale(1.2),
        });
    }
}
