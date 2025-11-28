// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Imaging IR image drawing across backends.
//!
//! This example exercises `create_image` + `DrawImage` on multiple
//! imaging backends, selected by a command-line argument:
//!   - `skia`
//!   - `vello_cpu`
//!   - `vello`
//!   - `vello_headless`
//!
//! It builds a small procedural RGBA8 checkerboard image, installs it
//! as an imaging `ImageId`, draws it many times with different transforms,
//! and either shows a window (Vello) or writes a PNG (Skia / Vello CPU).

use std::env;

use kurbo::Affine;
use png::{BitDepth, ColorType, Encoder};
use understory_examples::{
    vello_headless::render_scene_to_png,
    vello_winit::{VelloDemo, VelloWinitApp},
};
use understory_imaging::{
    DrawOp, ImageAlphaType, ImageDesc, ImageFormat, ImageSampler, ImagingBackend, ResourceBackend,
};
use understory_imaging_skia::SkiaImagingBackend;
use understory_imaging_vello::VelloImagingBackend;
use understory_imaging_vello_cpu::VelloCpuImagingBackend;
use vello::Scene;
use vello_cpu::{Pixmap, RenderContext};
use winit::event_loop::EventLoop;

fn make_checker(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let tile = ((x / 8) + (y / 8)) % 2;
            let (r, g, b) = if tile == 0 {
                (220, 220, 220)
            } else {
                (80, 80, 160)
            };
            pixels.extend_from_slice(&[r, g, b, 255]);
        }
    }
    pixels
}

fn render_images<B>(backend: &mut B, surface_width: f64, surface_height: f64)
where
    B: ImagingBackend + ResourceBackend,
{
    // Create a small checkerboard image resource.
    let img_w: u32 = 64;
    let img_h: u32 = 64;
    let pixels = make_checker(img_w, img_h);
    let image = backend.create_image(
        ImageDesc {
            width: img_w,
            height: img_h,
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
        },
        &pixels,
    );

    // Draw the image in a small grid with varying transforms.
    let rows = 4;
    let cols = 6;
    let dx = surface_width / (cols as f64 + 1.0);
    let dy = surface_height / (rows as f64 + 1.0);
    for row in 0..rows {
        for col in 0..cols {
            let x = dx * (col as f64 + 1.0) - img_w as f64 * 0.5;
            let y = dy * (row as f64 + 1.0) - img_h as f64 * 0.5;
            let mut xf = Affine::translate((x, y));
            // Scale and rotate some entries to exercise sampling.
            let scale = 1.0 + 0.5 * ((row + col) % 3) as f64;
            xf *= Affine::scale(scale);
            if (row + col) % 2 == 1 {
                xf *= Affine::rotate(0.5);
            }
            backend.draw(DrawOp::DrawImage {
                image,
                transform: xf,
                sampler: ImageSampler::default(),
            });
        }
    }
}

struct VelloImagesDemo;

impl VelloDemo for VelloImagesDemo {
    fn window_title(&self) -> &'static str {
        "Understory Imaging Images (Vello)"
    }

    fn initial_logical_size(&self) -> (f64, f64) {
        (480.0, 320.0)
    }

    fn rebuild_scene(&mut self, scene: &mut Scene, _scale_factor: f64) {
        scene.reset();
        let mut backend = VelloImagingBackend::new(scene);
        render_images(&mut backend, 480.0, 320.0);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend_name = env::args().nth(1).unwrap_or_else(|| "skia".to_string());

    match backend_name.as_str() {
        "skia" => {
            let width: i32 = 480;
            let height: i32 = 320;
            let mut surface = skia_safe::surfaces::raster_n32_premul((width, height))
                .ok_or("failed to create Skia surface")?;

            {
                let canvas = surface.canvas();
                canvas.clear(skia_safe::Color::from_argb(255, 255, 255, 255));
                let mut backend = SkiaImagingBackend::new(canvas);
                render_images(&mut backend, width as f64, height as f64);
            }

            let image = surface.image_snapshot();
            let png_data = image
                .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
                .ok_or("failed to encode PNG")?;
            std::fs::write("imaging_images_skia.png", png_data.as_bytes())?;
            eprintln!("Wrote imaging_images_skia.png");
        }
        "vello_cpu" => {
            let width: u16 = 480;
            let height: u16 = 320;

            let mut ctx = RenderContext::new(width, height);
            ctx.reset();

            {
                let mut backend = VelloCpuImagingBackend::new(&mut ctx);
                render_images(&mut backend, f64::from(width), f64::from(height));
            }

            let mut pixmap = Pixmap::new(width, height);
            ctx.flush();
            ctx.render_to_pixmap(&mut pixmap);
            let mut data = Vec::with_capacity((width as usize) * (height as usize) * 4);
            for p in pixmap.data() {
                data.extend_from_slice(&[p.r, p.g, p.b, p.a]);
            }

            let file = std::fs::File::create("imaging_images_vello_cpu.png")?;
            let mut encoder = Encoder::new(file, width as u32, height as u32);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(&data)?;

            eprintln!("Wrote imaging_images_vello_cpu.png");
        }
        "vello" => {
            let demo = VelloImagesDemo;
            let mut app = VelloWinitApp::new(demo);
            let event_loop = EventLoop::new()?;
            event_loop
                .run_app(&mut app)
                .expect("Couldn't run event loop");
        }
        "vello_headless" => {
            render_scene_to_png(480, 320, "imaging_images_vello_headless.png", |scene| {
                let mut backend = VelloImagingBackend::new(scene);
                render_images(&mut backend, 480.0, 320.0);
            })?;
        }
        other => {
            eprintln!(
                "Unknown backend '{}'. Expected 'skia', 'vello_cpu', 'vello', or 'vello_headless'.",
                other
            );
            std::process::exit(1);
        }
    }

    Ok(())
}
