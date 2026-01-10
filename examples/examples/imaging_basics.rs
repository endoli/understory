// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Imaging IR basics across multiple backends.
//!
//! This example renders the same imaging IR scene (background,
//! blend/opacity circles, HELLO text, COLRv1 emoji) using different
//! imaging backends, selected by a command-line argument:
//!   - `skia`
//!   - `vello_cpu`
//!   - `vello` (classic Vello/Scene via winit)
//!   - `vello_headless` (classic Vello/Scene to PNG)
//!
//! Examples:
//!   `cargo run -p understory_examples --example imaging_basics -- skia`
//!   `cargo run -p understory_examples --example imaging_basics -- vello_cpu`
//!   `cargo run -p understory_examples --example imaging_basics -- vello`
//!   `cargo run -p understory_examples --example imaging_basics -- vello_headless`

use std::env;

use kurbo::{Affine, Rect};
use png::{BitDepth, ColorType, Encoder};
use understory_examples::{
    vello_headless::render_scene_to_png,
    vello_winit::{VelloDemo, VelloWinitApp},
};
use understory_imaging::{
    BlendMode, DrawOp, ImagingBackend, ImagingBackendExt, PaintDesc, PathCmd, PathDesc,
    ResourceBackend, StateOp,
};
use understory_imaging_skia::SkiaImagingBackend;
use understory_imaging_vello::VelloImagingBackend;
use understory_imaging_vello_cpu::VelloCpuImagingBackend;
use understory_text_imaging::{TextHinting, draw_text_run};
use vello::Scene;
use vello::peniko::{Brush, Color, Mix};
use vello_cpu::{Pixmap, RenderContext};
use winit::event_loop::EventLoop;

fn rect_path_desc(rect: Rect) -> PathDesc {
    PathDesc {
        commands: vec![
            PathCmd::MoveTo {
                x: rect.x0 as f32,
                y: rect.y0 as f32,
            },
            PathCmd::LineTo {
                x: rect.x1 as f32,
                y: rect.y0 as f32,
            },
            PathCmd::LineTo {
                x: rect.x1 as f32,
                y: rect.y1 as f32,
            },
            PathCmd::LineTo {
                x: rect.x0 as f32,
                y: rect.y1 as f32,
            },
            PathCmd::Close,
        ]
        .into_boxed_slice(),
    }
}

fn circle_path_desc(cx: f32, cy: f32, r: f32, segments: usize) -> PathDesc {
    let mut cmds = Vec::with_capacity(segments + 2);
    for i in 0..=segments {
        let t = (i as f32 / segments as f32) * core::f32::consts::TAU;
        let x = cx + r * t.cos();
        let y = cy + r * t.sin();
        if i == 0 {
            cmds.push(PathCmd::MoveTo { x, y });
        } else {
            cmds.push(PathCmd::LineTo { x, y });
        }
    }
    cmds.push(PathCmd::Close);
    PathDesc {
        commands: cmds.into_boxed_slice(),
    }
}

fn render_scene<B>(backend: &mut B, width: f64, height: f64)
where
    B: ImagingBackend + ResourceBackend,
{
    // Background and overlapping circles.
    let bg_path = backend.create_path(rect_path_desc(Rect::new(0.0, 0.0, width, height)));
    let left_circle = backend.create_path(circle_path_desc(0.0, 0.0, 40.0, 48));
    let right_circle = backend.create_path(circle_path_desc(40.0, 0.0, 40.0, 48));

    let bg_paint = backend.create_paint(PaintDesc {
        brush: Brush::Solid(Color::from_rgba8(240, 240, 240, 255)),
    });
    let red_paint = backend.create_paint(PaintDesc {
        brush: Brush::Solid(Color::from_rgba8(220, 50, 47, 255)),
    });
    let blue_paint = backend.create_paint(PaintDesc {
        brush: Brush::Solid(Color::from_rgba8(38, 139, 210, 255)),
    });

    backend.state(StateOp::SetTransform(Affine::IDENTITY));
    backend.state(StateOp::SetPaint(bg_paint));
    backend.draw(DrawOp::FillPath(bg_path));

    let multiply = BlendMode::from(Mix::Multiply);
    let exclusion = BlendMode::from(Mix::Exclusion);

    // Row 1: SrcOver with varying opacity.
    let y1 = 110.0;
    backend.state(StateOp::SetTransform(Affine::translate((140.0, y1))));
    backend.state(StateOp::SetPaint(red_paint));
    backend.draw(DrawOp::FillPath(left_circle));
    backend.state(StateOp::SetPaint(blue_paint));
    backend.draw(DrawOp::FillPath(right_circle));

    backend.state(StateOp::SetTransform(Affine::translate((360.0, y1))));
    backend.with_opacity_layer(0.5, |b| {
        b.state(StateOp::SetPaint(red_paint));
        b.draw(DrawOp::FillPath(left_circle));
        b.state(StateOp::SetPaint(blue_paint));
        b.draw(DrawOp::FillPath(right_circle));
    });

    backend.state(StateOp::SetTransform(Affine::translate((580.0, y1))));
    backend.with_opacity_layer(0.25, |b| {
        b.state(StateOp::SetPaint(red_paint));
        b.draw(DrawOp::FillPath(left_circle));
        b.state(StateOp::SetPaint(blue_paint));
        b.draw(DrawOp::FillPath(right_circle));
    });

    // Row 2: different blend mixes at full opacity.
    let y2 = 250.0;

    // Column 1: baseline SrcOver.
    backend.state(StateOp::SetTransform(Affine::translate((140.0, y2))));
    backend.state(StateOp::SetPaint(red_paint));
    backend.draw(DrawOp::FillPath(left_circle));
    backend.state(StateOp::SetPaint(blue_paint));
    backend.draw(DrawOp::FillPath(right_circle));

    // Column 2: Multiply.
    backend.state(StateOp::SetTransform(Affine::translate((360.0, y2))));
    backend.with_blend_layer(multiply, |b| {
        b.state(StateOp::SetPaint(red_paint));
        b.draw(DrawOp::FillPath(left_circle));
        b.state(StateOp::SetPaint(blue_paint));
        b.draw(DrawOp::FillPath(right_circle));
    });

    // Column 3: Exclusion.
    backend.state(StateOp::SetTransform(Affine::translate((580.0, y2))));
    backend.with_blend_layer(exclusion, |b| {
        b.state(StateOp::SetPaint(red_paint));
        b.draw(DrawOp::FillPath(left_circle));
        b.state(StateOp::SetPaint(blue_paint));
        b.draw(DrawOp::FillPath(right_circle));
    });

    // Simple clip demo: clip a region and draw a large circle that would
    // otherwise bleed outside the clip.
    backend.state(StateOp::SetTransform(Affine::IDENTITY));
    let big_circle = backend.create_path(circle_path_desc(80.0, 80.0, 80.0, 64));
    backend.with_clip_rect(60.0, 40.0, 260.0, 220.0, |b| {
        b.state(StateOp::SetPaint(red_paint));
        b.draw(DrawOp::FillPath(big_circle));
    });

    // Outline text and COLRv1 emoji.

    let text_paint = backend.create_paint(PaintDesc {
        brush: Brush::Solid(Color::from_rgba8(0, 0, 0, 255)),
    });

    let roboto_bytes: &[u8] = include_bytes!("../../assets/fonts/roboto/Roboto-Regular.ttf");
    let text_xf = Affine::translate((60.0, 320.0));
    draw_text_run(
        backend,
        roboto_bytes,
        "HELLO",
        0,
        32.0,
        text_paint,
        text_xf,
        TextHinting::Hinted,
    );

    let colr_subset_bytes: &[u8] =
        include_bytes!("../../assets/fonts/noto_color_emoji/NotoColorEmoji-Subset.ttf");
    let emoji_text = "🎉🤠✅";
    let emoji_xf = Affine::translate((200.0, 320.0));
    draw_text_run(
        backend,
        colr_subset_bytes,
        emoji_text,
        0,
        32.0,
        text_paint,
        emoji_xf,
        TextHinting::Hinted,
    );
}

struct VelloBasicsDemo;

impl VelloDemo for VelloBasicsDemo {
    fn window_title(&self) -> &'static str {
        "Understory Imaging Basics (Vello)"
    }

    fn initial_logical_size(&self) -> (f64, f64) {
        (720.0, 360.0)
    }

    fn rebuild_scene(&mut self, scene: &mut Scene, _scale_factor: f64) {
        scene.reset();
        let mut backend = VelloImagingBackend::new(scene);
        render_scene(&mut backend, 720.0, 360.0);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend_name = env::args().nth(1).unwrap_or_else(|| "skia".to_string());

    match backend_name.as_str() {
        "skia" => {
            let width: i32 = 720;
            let height: i32 = 360;
            let mut surface = skia_safe::surfaces::raster_n32_premul((width, height))
                .ok_or("failed to create Skia surface")?;

            {
                let canvas = surface.canvas();
                canvas.clear(skia_safe::Color::from_argb(255, 255, 255, 255));
                let mut backend = SkiaImagingBackend::new(canvas);
                render_scene(&mut backend, width as f64, height as f64);
            }

            let image = surface.image_snapshot();
            let png_data = image
                .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
                .ok_or("failed to encode PNG")?;
            std::fs::write("imaging_basics_skia.png", png_data.as_bytes())?;
            eprintln!("Wrote imaging_basics_skia.png");
        }
        "vello_cpu" => {
            let width: u16 = 720;
            let height: u16 = 360;

            let mut ctx = RenderContext::new(width, height);
            ctx.reset();

            {
                let mut backend = VelloCpuImagingBackend::new(&mut ctx);
                render_scene(&mut backend, width as f64, height as f64);
            }

            let mut pixmap = Pixmap::new(width, height);
            ctx.flush();
            ctx.render_to_pixmap(&mut pixmap);
            let mut data = Vec::with_capacity((width as usize) * (height as usize) * 4);
            for p in pixmap.data() {
                data.extend_from_slice(&[p.r, p.g, p.b, p.a]);
            }

            let file = std::fs::File::create("imaging_basics_vello_cpu.png")?;
            let mut encoder = Encoder::new(file, width as u32, height as u32);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(&data)?;

            eprintln!("Wrote imaging_basics_vello_cpu.png");
        }
        "vello" => {
            let demo = VelloBasicsDemo;
            let mut app = VelloWinitApp::new(demo);
            let event_loop = EventLoop::new()?;
            event_loop
                .run_app(&mut app)
                .expect("Couldn't run event loop");
        }
        "vello_headless" => {
            render_scene_to_png(720, 360, "imaging_basics_vello_headless.png", |scene| {
                let mut backend = VelloImagingBackend::new(scene);
                render_scene(&mut backend, 720.0, 360.0);
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
