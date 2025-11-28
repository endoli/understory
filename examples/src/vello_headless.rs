// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal headless Vello renderer helper for examples.
//!
//! This module provides a small utility to render a `vello::Scene` into
//! an offscreen texture and write it as a PNG without creating a window,
//! mirroring the approach used by Vello's own headless example.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use png::{BitDepth, ColorType, Encoder};
use vello::peniko::color::palette;
use vello::util::RenderContext as VelloRenderContext;
use vello::wgpu::{
    self, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, TextureDescriptor,
    TextureFormat, TextureUsages,
};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

/// Render a scene built by `build_scene` into an offscreen texture and
/// write it as a PNG at `path`.
pub fn render_scene_to_png<F>(
    width: u32,
    height: u32,
    path: &str,
    build_scene: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnOnce(&mut Scene),
{
    let mut ctx = VelloRenderContext::new();
    let device_id = pollster::block_on(ctx.device(None))
        .ok_or("No compatible wgpu device found for Vello headless")?;
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
    )?;

    let mut scene = Scene::new();
    build_scene(&mut scene);

    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&TextureDescriptor {
        label: Some("Understory headless target"),
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
        base_color: palette::css::BLACK,
        width,
        height,
        antialiasing_method: AaConfig::Area,
    };
    renderer.render_to_texture(device, queue, &scene, &view, &params)?;

    let padded_row_bytes = (width * 4).next_multiple_of(256);
    let buffer_size = padded_row_bytes as u64 * height as u64;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Understory headless readback"),
        size: buffer_size,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("Understory headless copy encoder"),
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

    // Map the buffer using a simple spin + poll loop.
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

    let file = std::fs::File::create(path)?;
    let mut encoder = Encoder::new(file, width, height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&pixels)?;

    Ok(())
}
