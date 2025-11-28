// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Image snapshot tests for `understory_imaging_vello_cpu` using `kompari`.

use kompari::Image;
use understory_imaging::ImagingBackend;
use understory_imaging_snapshot_tests::cases::{DEFAULT_HEIGHT, DEFAULT_WIDTH};
use understory_imaging_vello_cpu::VelloCpuImagingBackend;
use vello_cpu::{Pixmap, RenderContext, RenderMode, RenderSettings};

mod common;

fn render_case<F>(width: u16, height: u16, build: F) -> Image
where
    F: FnOnce(&mut VelloCpuImagingBackend<'_>),
{
    let settings = RenderSettings {
        // Force u8 pipeline output even if `f32_pipeline` is enabled elsewhere in the workspace
        // (e.g. via `--all-features`), to keep snapshots stable across configurations.
        render_mode: RenderMode::OptimizeSpeed,
        ..RenderSettings::default()
    };
    let mut ctx = RenderContext::new_with(width, height, settings);
    let mut backend = VelloCpuImagingBackend::new(&mut ctx);
    build(&mut backend);

    let mut pixmap = Pixmap::new(width, height);
    backend.ctx.flush();
    backend.ctx.render_to_pixmap(&mut pixmap);

    let unpremul = pixmap.take_unpremultiplied();
    let mut bytes = Vec::with_capacity(unpremul.len() * 4);
    for p in unpremul {
        bytes.extend_from_slice(&[p.r, p.g, p.b, p.a]);
    }

    kompari::image::ImageBuffer::from_raw(u32::from(width), u32::from(height), bytes)
        .expect("RGBA buffer size should match image dimensions")
}

#[test]
fn snapshots() {
    let mut errors = Vec::new();
    let w = f32::from(DEFAULT_WIDTH);
    let h = f32::from(DEFAULT_HEIGHT);
    common::run_cases(
        "vello_cpu",
        |case| {
            render_case(DEFAULT_WIDTH, DEFAULT_HEIGHT, |backend| {
                let backend: &mut dyn ImagingBackend = backend;
                case.run(backend, w, h);
            })
        },
        &mut errors,
    );

    common::assert_no_snapshot_errors(errors);
}
