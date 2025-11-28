// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Imaging IR picture recordings across backends.
//!
//! This example exercises `record_picture` + `DrawPicture` with the same
//! recorded pictures on multiple backends, selected by a command-line
//! argument:
//!   - `skia`
//!   - `vello_cpu`
//!   - `vello`
//!   - `vello_headless`

use std::env;

use kurbo::Affine;
use png::{BitDepth, ColorType, Encoder};
use understory_examples::{
    vello_headless::render_scene_to_png,
    vello_winit::{VelloDemo, VelloWinitApp},
};
use understory_imaging::{
    DrawOp, ImagingBackend, ImagingBackendExt, PaintDesc, PathCmd, PathDesc, ResourceBackend,
    StateOp, record_picture,
};
use understory_imaging_skia::SkiaImagingBackend;
use understory_imaging_vello::VelloImagingBackend;
use understory_imaging_vello_cpu::VelloCpuImagingBackend;
use vello::Scene;
use vello::peniko::{Brush, Color};
use vello_cpu::{Pixmap, RenderContext};
use winit::event_loop::EventLoop;

fn square_path_desc(size: f32) -> PathDesc {
    PathDesc {
        commands: vec![
            PathCmd::MoveTo { x: 0.0, y: 0.0 },
            PathCmd::LineTo { x: size, y: 0.0 },
            PathCmd::LineTo { x: size, y: size },
            PathCmd::LineTo { x: 0.0, y: size },
            PathCmd::Close,
        ]
        .into_boxed_slice(),
    }
}

fn build_pictures<B>(backend: &mut B) -> [understory_imaging::PictureId; 3]
where
    B: ImagingBackend + ResourceBackend,
{
    let orange_paint = backend.create_paint(PaintDesc {
        brush: Brush::Solid(Color::from_rgba8(255, 128, 0, 255)),
    });
    let blue_paint = backend.create_paint(PaintDesc {
        brush: Brush::Solid(Color::from_rgba8(80, 160, 255, 255)),
    });
    let square = backend.create_path(square_path_desc(20.0));

    // Picture 1: solid orange square with a faint outer background.
    let picture_fill = record_picture(backend, |b| {
        let bg_paint = b.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(40, 40, 40, 255)),
        });
        b.state(StateOp::SetPaint(bg_paint));
        b.draw(DrawOp::FillRect {
            x0: -4.0,
            y0: -4.0,
            x1: 24.0,
            y1: 24.0,
        });
        b.state(StateOp::SetPaint(orange_paint));
        b.draw(DrawOp::FillPath(square));
    });

    // Picture 2: blue stroked square in a semi-transparent group.
    let picture_stroke = record_picture(backend, |b| {
        let stroke = kurbo::Stroke::new(2.0);
        b.state(StateOp::SetStroke(stroke));
        b.state(StateOp::SetPaint(blue_paint));
        b.with_opacity_layer(0.7, |b| {
            b.state(StateOp::SetTransform(Affine::translate((2.0, 2.0))));
            b.draw(DrawOp::StrokePath(square));
        });
    });

    // Picture 3: rotated orange square with a blue outline.
    let picture_rotated = record_picture(backend, |b| {
        b.state(StateOp::SetPaint(orange_paint));
        let xf = Affine::translate((10.0, 10.0)) * Affine::rotate(std::f64::consts::FRAC_PI_4);
        b.state(StateOp::SetTransform(xf));
        b.draw(DrawOp::FillPath(square));
        let stroke = kurbo::Stroke::new(1.0);
        b.state(StateOp::SetStroke(stroke));
        b.state(StateOp::SetPaint(blue_paint));
        b.draw(DrawOp::StrokePath(square));
    });

    [picture_fill, picture_stroke, picture_rotated]
}

fn draw_picture_grid<B>(
    backend: &mut B,
    pictures: &[understory_imaging::PictureId; 3],
    rows: u32,
    cols: u32,
    base: Affine,
) where
    B: ImagingBackend + ResourceBackend,
{
    for row in 0..rows {
        for col in 0..cols {
            let idx = (row + col) as usize % pictures.len();
            let local_rotate = if (row + col) % 2 == 0 {
                Affine::rotate(0.15)
            } else {
                Affine::rotate(-0.15)
            };
            let xf = base
                * Affine::translate((40.0 + 40.0 * col as f64, 40.0 + 40.0 * row as f64))
                * local_rotate;
            backend.draw(DrawOp::DrawPicture {
                picture: pictures[idx],
                transform: xf,
            });
        }
    }
}

struct VelloPicturesDemo;

impl VelloDemo for VelloPicturesDemo {
    fn window_title(&self) -> &'static str {
        "Understory Imaging Pictures (Vello)"
    }

    fn initial_logical_size(&self) -> (f64, f64) {
        (800.0, 800.0)
    }

    fn rebuild_scene(&mut self, scene: &mut Scene, scale_factor: f64) {
        scene.reset();
        let mut backend = VelloImagingBackend::new(scene);
        let pictures = build_pictures(&mut backend);
        let base = Affine::scale(scale_factor * 1.2);
        draw_picture_grid(&mut backend, &pictures, 20, 15, base);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend_name = env::args().nth(1).unwrap_or_else(|| "skia".to_string());

    match backend_name.as_str() {
        "skia" => {
            let width: i32 = 800;
            let height: i32 = 800;
            let mut surface = skia_safe::surfaces::raster_n32_premul((width, height))
                .ok_or("failed to create Skia surface")?;

            {
                let canvas = surface.canvas();
                canvas.clear(skia_safe::Color::from_argb(255, 30, 30, 30));
                let mut backend = SkiaImagingBackend::new(canvas);
                let pictures = build_pictures(&mut backend);
                draw_picture_grid(&mut backend, &pictures, 20, 15, Affine::scale(1.2));
            }

            let image = surface.image_snapshot();
            let png_data = image
                .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
                .ok_or("failed to encode PNG")?;
            std::fs::write("imaging_pictures_skia.png", png_data.as_bytes())?;
            eprintln!("Wrote imaging_pictures_skia.png");
        }
        "vello_cpu" => {
            let width: u16 = 800;
            let height: u16 = 800;

            let mut ctx = RenderContext::new(width, height);
            ctx.reset();

            {
                let mut backend = VelloCpuImagingBackend::new(&mut ctx);
                let pictures = build_pictures(&mut backend);
                // Use the same grid size as Skia/Vello so behavior
                // is directly comparable; run in release mode for
                // reasonable performance.
                draw_picture_grid(&mut backend, &pictures, 20, 15, Affine::scale(1.2));
            }

            let mut pixmap = Pixmap::new(width, height);
            ctx.flush();
            ctx.render_to_pixmap(&mut pixmap);
            let mut data = Vec::with_capacity((width as usize) * (height as usize) * 4);
            for p in pixmap.data() {
                data.extend_from_slice(&[p.r, p.g, p.b, p.a]);
            }

            let file = std::fs::File::create("imaging_pictures_vello_cpu.png")?;
            let mut encoder = Encoder::new(file, width as u32, height as u32);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(&data)?;

            eprintln!("Wrote imaging_pictures_vello_cpu.png");
        }
        "vello" => {
            let demo = VelloPicturesDemo;
            let mut app = VelloWinitApp::new(demo);
            let event_loop = EventLoop::new()?;
            event_loop
                .run_app(&mut app)
                .expect("Couldn't run event loop");
        }
        "vello_headless" => {
            render_scene_to_png(800, 800, "imaging_pictures_vello_headless.png", |scene| {
                let mut backend = VelloImagingBackend::new(scene);
                let pictures = build_pictures(&mut backend);
                draw_picture_grid(&mut backend, &pictures, 20, 15, Affine::scale(1.2));
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
