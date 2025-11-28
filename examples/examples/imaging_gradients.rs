// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Imaging IR gradient and extend-mode demo across backends.
//!
//! This example renders a small matrix of gradients using:
//!   - linear / radial / sweep kinds
//!   - different extend modes (Pad, Repeat, Reflect)
//!
//! on multiple imaging backends, selected by a command-line argument:
//!   - `skia`
//!   - `vello_cpu`
//!   - `vello`
//!   - `vello_headless`

use std::env;

use png::{BitDepth, ColorType, Encoder};
use understory_examples::{
    vello_headless::render_scene_to_png,
    vello_winit::{VelloDemo, VelloWinitApp},
};
use understory_imaging::{DrawOp, ImagingBackend, PaintDesc, ResourceBackend, StateOp};
use understory_imaging_skia::SkiaImagingBackend;
use understory_imaging_vello::VelloImagingBackend;
use understory_imaging_vello_cpu::VelloCpuImagingBackend;
use vello::Scene;
use vello::peniko::{
    Brush, Color, ColorStop, Extend, Gradient, GradientKind, LinearGradientPosition,
    RadialGradientPosition, SweepGradientPosition,
};
use vello_cpu::{Pixmap, RenderContext};
use winit::event_loop::EventLoop;

fn make_linear(p0: (f64, f64), p1: (f64, f64), extend: Extend) -> Brush {
    let kind = GradientKind::Linear(LinearGradientPosition::new(p0, p1));
    let stops = vec![
        ColorStop::from((0.0, Color::from_rgba8(255, 0, 0, 255))),
        ColorStop::from((0.5, Color::from_rgba8(0, 255, 0, 200))),
        ColorStop::from((1.0, Color::from_rgba8(0, 0, 255, 255))),
    ];
    Brush::Gradient(Gradient {
        kind,
        extend,
        stops: stops.as_slice().into(),
        ..Gradient::default()
    })
}

fn make_radial(c0: (f64, f64), r0: f32, c1: (f64, f64), r1: f32, extend: Extend) -> Brush {
    let kind = GradientKind::Radial(RadialGradientPosition::new_two_point(c0, r0, c1, r1));
    let stops = vec![
        ColorStop::from((0.0, Color::from_rgba8(255, 255, 0, 255))),
        ColorStop::from((0.5, Color::from_rgba8(255, 0, 255, 200))),
        ColorStop::from((1.0, Color::from_rgba8(0, 255, 255, 255))),
    ];
    Brush::Gradient(Gradient {
        kind,
        extend,
        stops: stops.as_slice().into(),
        ..Gradient::default()
    })
}

fn make_sweep(center: (f64, f64), extend: Extend) -> Brush {
    let kind = GradientKind::Sweep(SweepGradientPosition::new(
        center,
        0.0_f32,
        std::f32::consts::TAU,
    ));
    let stops = vec![
        ColorStop::from((0.0, Color::from_rgba8(255, 0, 0, 255))),
        ColorStop::from((1.0 / 3.0, Color::from_rgba8(0, 255, 0, 255))),
        ColorStop::from((2.0 / 3.0, Color::from_rgba8(0, 0, 255, 255))),
        ColorStop::from((1.0, Color::from_rgba8(255, 0, 0, 255))),
    ];
    Brush::Gradient(Gradient {
        kind,
        extend,
        stops: stops.as_slice().into(),
        ..Gradient::default()
    })
}

fn render_gradients<B>(backend: &mut B, surface_width: f64, surface_height: f64)
where
    B: ImagingBackend + ResourceBackend,
{
    // Layout: 3 columns (Pad, Repeat, Reflect) × 3 rows (linear, radial, sweep)
    let cols = 3;
    let rows = 3;
    let dx = surface_width / (cols as f64 + 1.0);
    let dy = surface_height / (rows as f64 + 1.0);

    let extends = [Extend::Pad, Extend::Repeat, Extend::Reflect];

    for (row, kind_ix) in [0, 1, 2].into_iter().enumerate() {
        for (col, &extend) in extends.iter().enumerate() {
            let cx = dx * (col as f64 + 1.0);
            let cy = dy * (row as f64 + 1.0);
            let rect_w = 120.0;
            let rect_h = 80.0;
            let x0 = cx - rect_w * 0.5;
            let y0 = cy - rect_h * 0.5;
            let x1 = cx + rect_w * 0.5;
            let y1 = cy + rect_h * 0.5;

            let brush = match kind_ix {
                0 => make_linear((x0, cy), (x1, cy), extend),
                1 => make_radial((cx - 20.0, cy), 10.0, (cx + 10.0, cy + 5.0), 60.0, extend),
                _ => make_sweep((cx, cy), extend),
            };

            let paint = backend.create_paint(PaintDesc { brush });
            backend.state(StateOp::SetPaint(paint));
            backend.draw(DrawOp::FillRect {
                x0: x0 as f32,
                y0: y0 as f32,
                x1: x1 as f32,
                y1: y1 as f32,
            });
        }
    }
}

struct VelloGradientsDemo;

impl VelloDemo for VelloGradientsDemo {
    fn window_title(&self) -> &'static str {
        "Understory Imaging Gradients (Vello)"
    }

    fn initial_logical_size(&self) -> (f64, f64) {
        (640.0, 480.0)
    }

    fn rebuild_scene(&mut self, scene: &mut Scene, _scale_factor: f64) {
        scene.reset();
        let mut backend = VelloImagingBackend::new(scene);
        render_gradients(&mut backend, 640.0, 480.0);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend_name = env::args().nth(1).unwrap_or_else(|| "skia".to_string());

    match backend_name.as_str() {
        "skia" => {
            let width: i32 = 640;
            let height: i32 = 480;
            let mut surface = skia_safe::surfaces::raster_n32_premul((width, height))
                .ok_or("failed to create Skia surface")?;

            {
                let canvas = surface.canvas();
                canvas.clear(skia_safe::Color::from_argb(255, 255, 255, 255));
                let mut backend = SkiaImagingBackend::new(canvas);
                render_gradients(&mut backend, width as f64, height as f64);
            }

            let image = surface.image_snapshot();
            let png_data = image
                .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
                .ok_or("failed to encode PNG")?;
            std::fs::write("imaging_gradients_skia.png", png_data.as_bytes())?;
            eprintln!("Wrote imaging_gradients_skia.png");
        }
        "vello_cpu" => {
            let width: u16 = 640;
            let height: u16 = 480;

            let mut ctx = RenderContext::new(width, height);
            ctx.reset();

            {
                let mut backend = VelloCpuImagingBackend::new(&mut ctx);
                render_gradients(&mut backend, f64::from(width), f64::from(height));
            }

            let mut pixmap = Pixmap::new(width, height);
            ctx.flush();
            ctx.render_to_pixmap(&mut pixmap);
            let mut data = Vec::with_capacity((width as usize) * (height as usize) * 4);
            for p in pixmap.data() {
                data.extend_from_slice(&[p.r, p.g, p.b, p.a]);
            }

            let file = std::fs::File::create("imaging_gradients_vello_cpu.png")?;
            let mut encoder = Encoder::new(file, width as u32, height as u32);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(&data)?;

            eprintln!("Wrote imaging_gradients_vello_cpu.png");
        }
        "vello" => {
            let demo = VelloGradientsDemo;
            let mut app = VelloWinitApp::new(demo);
            let event_loop = EventLoop::new()?;
            event_loop
                .run_app(&mut app)
                .expect("Couldn't run event loop");
        }
        "vello_headless" => {
            render_scene_to_png(640, 480, "imaging_gradients_vello_headless.png", |scene| {
                let mut backend = VelloImagingBackend::new(scene);
                render_gradients(&mut backend, 640.0, 480.0);
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
