// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::num::NonZeroUsize;
use std::sync::Arc;

use ui_events::{keyboard::KeyboardEvent, pointer::PointerEvent};
use ui_events_winit::{WindowEventReducer, WindowEventTranslation};
use vello::peniko::Color;
use vello::util::{RenderContext, RenderSurface};
use vello::wgpu;
use vello::{AaConfig, Renderer, RendererOptions, Scene};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// Shared render state for Vello + winit examples.
pub enum RenderState<'s> {
    /// Winit is suspended; cache a window if we had one.
    Suspended(Option<Arc<Window>>),
    /// Active window + surface.
    Active {
        surface: Box<RenderSurface<'s>>,
        window: Arc<Window>,
    },
}

/// Minimal interface that a Vello demo needs to provide.
pub trait VelloDemo {
    /// Title of the window.
    fn window_title(&self) -> &'static str;

    /// Initial logical size of the window (width, height).
    fn initial_logical_size(&self) -> (f64, f64);

    /// Handle a pointer event.
    fn handle_pointer_event(&mut self, _e: PointerEvent) {}

    /// Handle a keyboard event.
    fn handle_keyboard_event(&mut self, _e: KeyboardEvent) {}

    /// Rebuild the Vello scene for the current frame.
    ///
    /// `scale_factor` is the window's scale factor, typically used
    /// to map logical coordinates into device pixels.
    fn rebuild_scene(&mut self, scene: &mut Scene, scale_factor: f64);
}

/// Generic winit application harness for Vello demos.
pub struct VelloWinitApp<'s, D: VelloDemo> {
    pub context: RenderContext,
    pub renderer: Option<Renderer>,
    pub state: RenderState<'s>,
    pub scene: Scene,
    pub reducer: WindowEventReducer,
    pub demo: D,
    pub use_cpu: bool,
}

impl<'s, D: VelloDemo> VelloWinitApp<'s, D> {
    /// Create a new app with the given demo implementation.
    pub fn new(demo: D) -> Self {
        Self {
            context: RenderContext::new(),
            renderer: None,
            state: RenderState::Suspended(None),
            scene: Scene::new(),
            reducer: WindowEventReducer::default(),
            demo,
            use_cpu: false,
        }
    }

    /// Create a new app that uses Vello's CPU-oriented execution path.
    pub fn new_cpu(demo: D) -> Self {
        Self {
            use_cpu: true,
            ..Self::new(demo)
        }
    }
}

impl<D: VelloDemo> ApplicationHandler for VelloWinitApp<'_, D> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let RenderState::Suspended(cached_window) = &mut self.state else {
            return;
        };

        let window = cached_window.take().unwrap_or_else(|| {
            let (w, h) = self.demo.initial_logical_size();
            create_winit_window(event_loop, self.demo.window_title(), w, h)
        });

        let size = window.inner_size();
        let surface_future = self.context.create_surface(
            window.clone(),
            size.width,
            size.height,
            wgpu::PresentMode::AutoVsync,
        );
        let surface = pollster::block_on(surface_future).expect("create surface");

        if self.renderer.is_none() {
            let renderer = if self.use_cpu {
                create_vello_cpu_renderer(&self.context, &surface)
            } else {
                create_vello_renderer(&self.context, &surface)
            };
            self.renderer = Some(renderer);
        }

        self.state = RenderState::Active {
            surface: Box::new(surface),
            window,
        };
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let RenderState::Active { window, .. } = &self.state {
            self.state = RenderState::Suspended(Some(window.clone()));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let (surface, window) = match &mut self.state {
            RenderState::Active { surface, window } if window.id() == window_id => {
                (surface, &**window)
            }
            _ => return,
        };

        if let Some(t) = self.reducer.reduce(window.scale_factor(), &event) {
            match t {
                WindowEventTranslation::Pointer(e) => self.demo.handle_pointer_event(e),
                WindowEventTranslation::Keyboard(k) => self.demo.handle_keyboard_event(k),
            }
            window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.context
                    .resize_surface(surface, size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                self.scene.reset();
                let scale = window.scale_factor();
                self.demo.rebuild_scene(&mut self.scene, scale);

                let wgpu::SurfaceConfiguration { width, height, .. } = surface.config;
                let device_handle = &self.context.devices[surface.dev_id];

                let surface_texture = surface
                    .surface
                    .get_current_texture()
                    .expect("get surface texture");

                self.renderer
                    .as_mut()
                    .expect("renderer")
                    .render_to_texture(
                        &device_handle.device,
                        &device_handle.queue,
                        &self.scene,
                        &surface.target_view,
                        &vello::RenderParams {
                            base_color: Color::BLACK,
                            width,
                            height,
                            antialiasing_method: AaConfig::Area,
                        },
                    )
                    .expect("render to texture");

                let mut encoder =
                    device_handle
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Surface Blit"),
                        });
                surface.blitter.copy(
                    &device_handle.device,
                    &mut encoder,
                    &surface.target_view,
                    &surface_texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                );
                device_handle.queue.submit([encoder.finish()]);
                surface_texture.present();

                let _ = device_handle.device.poll(wgpu::PollType::Poll);
            }
            _ => {}
        }
    }
}

fn create_winit_window(
    event_loop: &ActiveEventLoop,
    title: &str,
    width: f64,
    height: f64,
) -> Arc<Window> {
    let attr = Window::default_attributes()
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true)
        .with_title(title.to_string());
    Arc::new(event_loop.create_window(attr).expect("create window"))
}

fn create_vello_renderer(render_cx: &RenderContext, surface: &RenderSurface<'_>) -> Renderer {
    Renderer::new(
        &render_cx.devices[surface.dev_id].device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: vello::AaSupport::area_only(),
            num_init_threads: NonZeroUsize::new(1),
            pipeline_cache: None,
        },
    )
    .expect("create renderer")
}

fn create_vello_cpu_renderer(render_cx: &RenderContext, surface: &RenderSurface<'_>) -> Renderer {
    Renderer::new(
        &render_cx.devices[surface.dev_id].device,
        RendererOptions {
            use_cpu: true,
            antialiasing_support: vello::AaSupport::area_only(),
            num_init_threads: NonZeroUsize::new(1),
            pipeline_cache: None,
        },
    )
    .expect("create CPU renderer")
}
