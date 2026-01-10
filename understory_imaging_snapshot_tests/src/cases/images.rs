// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::*;

/// Verifies that drawing the same image at multiple scales produces stable results.
///
/// This is mainly a check that image transforms and sampling don't explode or alias unexpectedly.
pub(super) struct ImageScaled;

impl ImageScaled {
    const NAME: &'static str = "image_scaled";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for ImageScaled {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, _width: f32, _height: f32) {
        const W: u32 = 64;
        const H: u32 = 64;
        let image = solid_rgba_image(backend, W, H, [255, 0, 255, 255]);

        backend.draw(DrawOp::DrawImage {
            image,
            transform: Affine::translate((40.0, 30.0))
                * Affine::scale_non_uniform(120.0 / W as f64, 80.0 / H as f64),
            sampler: ImageSampler::default(),
        });

        backend.draw(DrawOp::DrawImage {
            image,
            transform: Affine::translate((190.0, 70.0))
                * Affine::scale_non_uniform(90.0 / W as f64, 60.0 / H as f64),
            sampler: ImageSampler::default(),
        });
    }
}

/// Checks that images with sharp alpha transitions are composited correctly against a dark background.
///
/// This is sensitive to premultiplication/unpremultiplication and edge sampling behavior.
pub(super) struct ImageAlphaEdges;

impl ImageAlphaEdges {
    const NAME: &'static str = "image_alpha_edges";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
    // Skia varies slightly in alpha edge rasterization across platforms.
    const SKIA_MAX_DIFF_PIXELS: u64 = 8_000;
}

impl SnapshotCase for ImageAlphaEdges {
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
        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(30, 30, 30, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        let image = alpha_edge_image(backend, 96, 64);
        backend.draw(DrawOp::DrawImage {
            image,
            transform: Affine::translate((40.0, 50.0)),
            sampler: ImageSampler::default(),
        });
    }
}

/// Verifies how image-local transforms compose with the current backend transform.
///
/// The expected result is that `StateOp::SetTransform` affects image draws as a parent transform.
pub(super) struct ImageTransformComposition;

impl ImageTransformComposition {
    const NAME: &'static str = "image_transform_composition";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for ImageTransformComposition {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(10, 10, 12, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        let image = solid_rgba_image(backend, 64, 64, [255, 0, 255, 255]);

        backend.state(StateOp::SetTransform(
            Affine::translate((width as f64 * 0.5, height as f64 * 0.5)) * Affine::rotate(0.25),
        ));
        backend.draw(DrawOp::DrawImage {
            image,
            transform: Affine::translate((-110.0, -40.0)),
            sampler: ImageSampler::default(),
        });
        backend.draw(DrawOp::DrawImage {
            image,
            transform: Affine::translate((30.0, -20.0)),
            sampler: ImageSampler::default(),
        });
        backend.state(StateOp::SetTransform(Affine::IDENTITY));
    }
}

/// Verifies that image drawing respects both rect clips and path clips.
///
/// This case draws different images through different clip shapes to catch clip routing bugs.
pub(super) struct ImageClipRectAndPath;

impl ImageClipRectAndPath {
    const NAME: &'static str = "image_clip_rect_and_path";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
    // Skia has small, platform-dependent clip edge differences for this case.
    const SKIA_MAX_DIFF_PIXELS: u64 = 512;
}

impl SnapshotCase for ImageClipRectAndPath {
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
        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(20, 20, 24, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        let image = solid_rgba_image(backend, 120, 90, [60, 220, 120, 255]);

        backend.with_clip_rect(30.0, 30.0, 160.0, 120.0, |backend| {
            backend.draw(DrawOp::DrawImage {
                image,
                transform: Affine::translate((10.0, 20.0)),
                sampler: ImageSampler::default(),
            });
        });

        let star = backend.create_path(star_path_desc(width * 0.72, height * 0.55, 52.0, 22.0, 5));
        let image2 = solid_rgba_image(backend, 120, 90, [255, 120, 0, 255]);
        backend.with_clip_path(star, FillRule::NonZero, |backend| {
            backend.draw(DrawOp::DrawImage {
                image: image2,
                transform: Affine::translate((190.0, 60.0)),
                sampler: ImageSampler::default(),
            });
        });
    }
}

/// Verifies `DrawOp::DrawImageRect` behavior for `src` sub-rects mapped into `dst` rectangles.
///
/// The expected result is that only the specified source region is sampled and scaled to the destination.
pub(super) struct ImageRectSrcDst;

impl ImageRectSrcDst {
    const NAME: &'static str = "image_rect_src_dst";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for ImageRectSrcDst {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, _width: f32, _height: f32) {
        let pixels: [u8; 16] = [
            255, 0, 0, 255, // red
            0, 255, 0, 255, // green
            0, 0, 255, 255, // blue
            255, 255, 0, 255, // yellow
        ];
        let image = backend.create_image(
            ImageDesc {
                width: 2,
                height: 2,
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
            },
            &pixels,
        );

        let sampler = ImageSampler {
            quality: peniko::ImageQuality::Low,
            ..ImageSampler::default()
        };

        backend.draw(DrawOp::DrawImageRect {
            image,
            src: Some(RectF {
                x0: 0.25,
                y0: 0.25,
                x1: 0.75,
                y1: 0.75,
            }),
            dst: RectF {
                x0: 40.0,
                y0: 40.0,
                x1: 160.0,
                y1: 120.0,
            },
            sampler,
        });

        backend.draw(DrawOp::DrawImageRect {
            image,
            src: Some(RectF {
                x0: 1.25,
                y0: 1.25,
                x1: 1.75,
                y1: 1.75,
            }),
            dst: RectF {
                x0: 200.0,
                y0: 60.0,
                x1: 280.0,
                y1: 140.0,
            },
            sampler,
        });
    }
}

/// Compares nearest-neighbor vs linear sampling on a high-contrast checkerboard image.
///
/// The left and right panes should differ clearly if sampling quality is wired correctly.
pub(super) struct ImageSamplingNearestVsLinear;

impl ImageSamplingNearestVsLinear {
    const NAME: &'static str = "image_sampling_nearest_vs_linear";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for ImageSamplingNearestVsLinear {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, _width: f32, _height: f32) {
        const W: u32 = 16;
        const H: u32 = 16;
        let mut pixels = vec![0_u8; (W as usize) * (H as usize) * 4];
        for y in 0..H {
            for x in 0..W {
                let tile = ((x / 2) + (y / 2)) % 2;
                let (r, g, b) = if tile == 0 {
                    (240, 240, 240)
                } else {
                    (40, 40, 40)
                };
                let i = ((y * W + x) as usize) * 4;
                pixels[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
        let image = backend.create_image(
            ImageDesc {
                width: W,
                height: H,
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
            },
            &pixels,
        );

        let src = Some(RectF {
            x0: 0.25,
            y0: 0.25,
            x1: (W as f32) - 0.25,
            y1: (H as f32) - 0.25,
        });
        let dst_left = RectF {
            x0: 30.0,
            y0: 40.0,
            x1: 150.0,
            y1: 160.0,
        };
        let dst_right = RectF {
            x0: 170.0,
            y0: 40.0,
            x1: 290.0,
            y1: 160.0,
        };

        let nearest = ImageSampler {
            quality: peniko::ImageQuality::Low,
            ..ImageSampler::default()
        };
        let linear = ImageSampler {
            quality: peniko::ImageQuality::Medium,
            ..ImageSampler::default()
        };

        backend.draw(DrawOp::DrawImageRect {
            image,
            src,
            dst: dst_left,
            sampler: nearest,
        });
        backend.draw(DrawOp::DrawImageRect {
            image,
            src,
            dst: dst_right,
            sampler: linear,
        });
    }
}

/// Verifies image extend modes (`Pad`, `Repeat`, `Reflect`) when sampling outside the source image.
///
/// This catches backend differences in how out-of-bounds coordinates are handled.
pub(super) struct ImageExtendModes;

impl ImageExtendModes {
    const NAME: &'static str = "image_extend_modes";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
    // Skia varies slightly in sampler/edge behavior across platforms.
    const SKIA_MAX_DIFF_PIXELS: u64 = 512;
}

impl SnapshotCase for ImageExtendModes {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn skia_max_diff_pixels(&self) -> u64 {
        Self::SKIA_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, _width: f32, _height: f32) {
        const W: u32 = 4;
        const H: u32 = 4;
        let mut pixels = vec![0_u8; (W as usize) * (H as usize) * 4];
        for y in 0..H {
            for x in 0..W {
                let (r, g, b) = match (x < 2, y < 2) {
                    (true, true) => (255, 0, 0),
                    (false, true) => (0, 255, 0),
                    (true, false) => (0, 0, 255),
                    (false, false) => (255, 255, 0),
                };
                let i = ((y * W + x) as usize) * 4;
                pixels[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
        let image = backend.create_image(
            ImageDesc {
                width: W,
                height: H,
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
            },
            &pixels,
        );

        let src = Some(RectF {
            x0: -6.0,
            y0: -4.0,
            x1: (W as f32) + 6.0,
            y1: (H as f32) + 4.0,
        });

        let base = ImageSampler {
            quality: peniko::ImageQuality::Low,
            ..ImageSampler::default()
        };

        let mut pad = base;
        pad.x_extend = Extend::Pad;
        pad.y_extend = Extend::Pad;
        let mut repeat = base;
        repeat.x_extend = Extend::Repeat;
        repeat.y_extend = Extend::Repeat;
        let mut reflect = base;
        reflect.x_extend = Extend::Reflect;
        reflect.y_extend = Extend::Reflect;

        let y0 = 30.0;
        let y1 = 170.0;
        backend.draw(DrawOp::DrawImageRect {
            image,
            src,
            dst: RectF {
                x0: 20.0,
                y0,
                x1: 115.0,
                y1,
            },
            sampler: pad,
        });
        backend.draw(DrawOp::DrawImageRect {
            image,
            src,
            dst: RectF {
                x0: 125.0,
                y0,
                x1: 220.0,
                y1,
            },
            sampler: repeat,
        });
        backend.draw(DrawOp::DrawImageRect {
            image,
            src,
            dst: RectF {
                x0: 230.0,
                y0,
                x1: 325.0,
                y1,
            },
            sampler: reflect,
        });
    }
}
