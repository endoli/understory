// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Image snapshot tests for `understory_imaging_skia` using `kompari`.

#![cfg(feature = "skia")]

use kompari::Image;
use skia_safe::image::CachingHint;
use skia_safe::{AlphaType, ColorType, ImageInfo, surfaces};
use understory_imaging::ImagingBackend;
use understory_imaging_skia::SkiaImagingBackend;
use understory_imaging_snapshot_tests::cases::{DEFAULT_HEIGHT, DEFAULT_WIDTH};

mod common;

fn render_case<F>(width: u16, height: u16, build: F) -> Image
where
    F: FnOnce(&mut SkiaImagingBackend<'_>),
{
    let mut surface = surfaces::raster_n32_premul((i32::from(width), i32::from(height)))
        .expect("create skia raster surface");
    let canvas = surface.canvas();
    let mut backend = SkiaImagingBackend::new(canvas);
    build(&mut backend);

    let image = surface.image_snapshot();
    let info = ImageInfo::new(
        (i32::from(width), i32::from(height)),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let mut bytes = vec![0_u8; usize::from(width) * usize::from(height) * 4];
    let ok = image.read_pixels(
        &info,
        bytes.as_mut_slice(),
        (4 * i32::from(width)) as usize,
        (0, 0),
        CachingHint::Disallow,
    );
    assert!(ok, "read_pixels should succeed");

    kompari::image::ImageBuffer::from_raw(u32::from(width), u32::from(height), bytes)
        .expect("RGBA buffer size should match image dimensions")
}

#[test]
fn snapshots() {
    let mut errors = Vec::new();

    let w = f32::from(DEFAULT_WIDTH);
    let h = f32::from(DEFAULT_HEIGHT);
    common::run_cases_with(
        "skia",
        |case| {
            render_case(DEFAULT_WIDTH, DEFAULT_HEIGHT, |backend| {
                let backend: &mut dyn ImagingBackend = backend;
                case.run(backend, w, h);
            })
        },
        |case| case.skia_max_diff_pixels(),
        &mut errors,
    );

    common::assert_no_snapshot_errors(errors);
}
