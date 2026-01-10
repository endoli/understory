// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Image snapshot tests for `understory_imaging_vello` using `kompari`.

#![cfg(feature = "vello")]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use kompari::Image;
use peniko::Color;
use understory_imaging::ImagingBackend;
use understory_imaging_snapshot_tests::cases::{
    DEFAULT_HEIGHT, DEFAULT_WIDTH, selected_cases_for_backend,
};
use understory_imaging_vello::VelloImagingBackend;
use vello::util::RenderContext as VelloRenderContext;
use vello::wgpu::{
    self, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, TextureDescriptor,
    TextureFormat, TextureUsages,
};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

mod common;

fn try_render_case<F>(width: u32, height: u32, build: F) -> Option<Image>
where
    F: FnOnce(&mut VelloImagingBackend<'_>),
{
    let mut ctx = VelloRenderContext::new();
    let device_id = pollster::block_on(ctx.device(None))?;
    let device_handle = &ctx.devices[device_id];
    let device = &device_handle.device;
    let queue = &device_handle.queue;

    let mut renderer = Renderer::new(
        device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::area_only(),
            num_init_threads: std::num::NonZeroUsize::new(1),
            pipeline_cache: None,
        },
    )
    .expect("create vello renderer");

    let mut scene = Scene::new();
    {
        let mut backend = VelloImagingBackend::new(&mut scene);
        build(&mut backend);
    }

    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&TextureDescriptor {
        label: Some("Understory snapshot target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let params = RenderParams {
        base_color: Color::from_rgba8(0, 0, 0, 0),
        width,
        height,
        antialiasing_method: AaConfig::Area,
    };
    renderer
        .render_to_texture(device, queue, &scene, &view, &params)
        .expect("render to texture");

    let padded_row_bytes = (width * 4).next_multiple_of(256);
    let buffer_size = padded_row_bytes as u64 * height as u64;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Understory snapshot readback"),
        size: buffer_size,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("Understory snapshot copy encoder"),
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes),
                rows_per_image: None,
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let done = Arc::new(AtomicBool::new(false));
    let done_clone = done.clone();
    slice.map_async(wgpu::MapMode::Read, move |_| {
        done_clone.store(true, Ordering::SeqCst);
    });
    while !done.load(Ordering::SeqCst) {
        let _ = device.poll(wgpu::PollType::Poll);
    }

    let data = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded_row_bytes) as usize;
        pixels.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    drop(data);
    buffer.unmap();

    Some(
        kompari::image::ImageBuffer::from_raw(width, height, pixels)
            .expect("RGBA buffer size should match image dimensions"),
    )
}

#[test]
fn snapshots() {
    let mut errors = Vec::new();

    let w_u32 = u32::from(DEFAULT_WIDTH);
    let h_u32 = u32::from(DEFAULT_HEIGHT);
    let w = f32::from(DEFAULT_WIDTH);
    let h = f32::from(DEFAULT_HEIGHT);

    let cases = selected_cases_for_backend("vello");
    let case0 = cases[0];
    let Some(image) = try_render_case(w_u32, h_u32, |backend| {
        let backend: &mut dyn ImagingBackend = backend;
        case0.run(backend, w, h);
    }) else {
        eprintln!("Skipping Vello GPU snapshots: no compatible wgpu device found.");
        return;
    };
    // Vello GPU can have tiny, hardware-dependent rasterization differences for some
    // AA-heavy cases. Allow a small, bounded number of different pixels to keep
    // the snapshot suite stable while still catching real regressions.
    common::check_snapshot_with_tolerance(
        "vello",
        case0.name(),
        &image,
        case0.vello_gpu_max_diff_pixels(),
        &mut errors,
    );
    for &case in &cases[1..] {
        let image = try_render_case(w_u32, h_u32, |backend| {
            let backend: &mut dyn ImagingBackend = backend;
            case.run(backend, w, h);
        })
        .expect("wgpu device already validated");
        common::check_snapshot_with_tolerance(
            "vello",
            case.name(),
            &image,
            case.vello_gpu_max_diff_pixels(),
            &mut errors,
        );
    }

    common::assert_no_snapshot_errors(errors);
}
