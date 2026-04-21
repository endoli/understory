// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Windowed Overstory demo rendered through `imaging` + Vello Hybrid.
//!
//! This example keeps `overstory` renderer-agnostic:
//! - Overstory owns retained UI state, style resolution, layout, box-tree hit
//!   testing, and pointer interaction.
//! - This example lowers the resolved [`overstory::SceneSnapshot`] into
//!   a retained `understory_display::DisplayTree`.
//! - It then lowers that tree directly into `imaging::record::Scene`.
//! - `imaging_vello_hybrid` encodes the scene and renders it via `wgpu` to a
//!   GPU surface.
//!
//! Run:
//! - `cargo run -p understory_examples --example overstory_visual_demo`

use std::sync::Arc;

use imaging_vello_hybrid::VelloHybridRenderer;
use kurbo::Rect;
use overstory::peniko::color::palette;
use overstory::{
    ButtonClass, ElementId, ElementKind, Interaction, LayoutClass, ThemeKeys, Ui, default_theme,
};
use ui_events_winit::{WindowEventReducer, WindowEventTranslation};
use understory_display::{BoxConstraints, TextEngine};
use understory_examples::overstory_display::imaging_scene_from_display_tree;
use understory_style::{
    IdSet, Selector, StyleBuilder, StyleCascade, StyleCascadeBuilder, StyleOrigin,
    StyleSheetBuilder, Theme, ThemeBuilder,
};
use understory_transcript::{EntryKind, MessageRole, NewEntry, Transcript};
use wgpu::TextureFormat;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

fn main() {
    let event_loop = EventLoop::new().expect("create event loop");
    let mut app = DemoApp::new();
    event_loop.run_app(&mut app).expect("run app");
}

#[derive(Debug)]
struct DemoIds {
    shell: ElementId,
    warm: ElementId,
    cool: ElementId,
    roomy: ElementId,
    compact: ElementId,
    sidebar: ElementId,
    content: ElementId,
    search: ElementId,
    settings: ElementId,
    deploy: ElementId,
    messages: ElementId,
    input: ElementId,
}

enum RenderState {
    Active {
        window: Arc<Window>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        surface_config: wgpu::SurfaceConfiguration,
        renderer: VelloHybridRenderer,
        blit: BlitPipeline,
    },
    Suspended,
}

/// Simple full-screen blit from an Rgba8Unorm texture to the surface format.
struct BlitPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl BlitPipeline {
    fn new(device: &wgpu::Device, surface_format: TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit shader"),
            source: wgpu::ShaderSource::Wgsl(
                r"
@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4f {
    let uv = vec2f(f32((idx << 1u) & 2u), f32(idx & 2u));
    return vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
}

@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;

@fragment
fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {
    let dims = vec2f(textureDimensions(t));
    let uv = pos.xy / dims;
    return textureSample(t, s, uv);
}
"
                .into(),
            ),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            pipeline,
            bind_group_layout,
            sampler,
        }
    }

    fn blit(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
    ) {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("blit encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }
}

struct DemoApp {
    ui: Ui,
    ids: DemoIds,
    roomy: bool,
    reducer: WindowEventReducer,
    text: TextEngine,
    render_state: RenderState,
    transcript: Transcript,
    message_count: usize,
}

impl DemoApp {
    fn new() -> Self {
        let (ui, ids) = build_demo_ui();
        let mut transcript = Transcript::new();
        let sample_messages = [
            (MessageRole::Assistant, "Welcome to the Overstory demo. This message area is driven by understory_transcript."),
            (MessageRole::User, "Can I type messages?"),
            (MessageRole::Assistant, "Each message is a TextBlock element that wraps its text within the available width."),
            (MessageRole::Assistant, "The message list is a ScrollView with fill layout, so it stretches to fill the space between the button row and the bottom of the content panel."),
            (MessageRole::User, "What about scrolling?"),
            (MessageRole::Assistant, "Try scrolling with the mouse wheel or trackpad to see the scroll offset in action."),
            (MessageRole::Assistant, "You can also switch between Warm and Cool themes using the sidebar buttons. The text properties cascade through the style system."),
            (MessageRole::Assistant, "Switching between Roomy and Compact density adjusts padding, gaps, and button sizes throughout the UI."),
            (MessageRole::Assistant, "This is a longer message to demonstrate text wrapping. When a message exceeds the available width, Parley shapes the text with a max advance constraint and the glyphs wrap onto multiple lines."),
            (MessageRole::User, "Nice."),
        ];
        for (role, text) in &sample_messages {
            transcript.append(NewEntry::message(*role, *text));
        }

        let mut app = Self {
            ui,
            ids,
            roomy: true,
            reducer: WindowEventReducer::default(),
            text: TextEngine::new(),
            render_state: RenderState::Suspended,
            transcript,
            message_count: 0,
        };
        app.sync_messages();
        app.apply_density(true);
        app.ui.set_focus(app.ids.input);
        app
    }

    /// Returns true if the messages ScrollView is currently scrolled to the
    /// bottom (or content fits within the viewport).
    fn is_at_tail(&self) -> bool {
        let offset = self.ui.scroll_offset(self.ids.messages);
        let content_h = self.ui.content_height(self.ids.messages);
        let viewport_h = self.ui.viewport_height(self.ids.messages);
        content_h <= viewport_h || offset + viewport_h >= content_h - 1.0
    }

    /// Scrolls the messages ScrollView to the bottom.
    fn scroll_to_tail(&mut self) {
        let _ = self.ui.scene(); // rebuild to get updated content_height
        let content_h = self.ui.content_height(self.ids.messages);
        let viewport_h = self.ui.viewport_height(self.ids.messages);
        self.ui.set_scroll_offset(
            self.ids.messages,
            (content_h - viewport_h).max(0.0),
        );
    }

    fn sync_messages(&mut self) {
        let entries: Vec<_> = self.transcript.entries().to_vec();
        for entry in entries.iter().skip(self.message_count) {
            if let EntryKind::Message(msg) = &entry.kind {
                let text = msg.body.as_text().unwrap_or("");
                let is_user = msg.role == MessageRole::User;
                let block = self
                    .ui
                    .append_child(self.ids.messages, ElementKind::TextBlock);
                self.ui.set_label(block, text);
                self.ui
                    .set_local(block, self.ui.properties().label_padding, 8.0);
                self.ui
                    .set_local(block, self.ui.properties().padding, 8.0);
                if is_user {
                    self.ui.set_local(
                        block,
                        self.ui.properties().background,
                        overstory::Color::from_rgba8(220, 235, 220, 255),
                    );
                    self.ui.set_local(
                        block,
                        self.ui.properties().corner_radius,
                        8.0,
                    );
                }
            }
        }
        self.message_count = entries.len();
    }

    fn process_pointer_translation(
        &mut self,
        pointer: ui_events_winit::WindowEventTranslation,
        window: &Window,
    ) {
        let WindowEventTranslation::Pointer(event) = pointer else {
            return;
        };
        let interactions = self.ui.handle_pointer_event(&event);
        self.apply_interactions(&interactions);
        window.request_redraw();
    }

    fn apply_interactions(&mut self, interactions: &overstory::InteractionBatch) {
        for interaction in interactions.events() {
            if let Interaction::Clicked(target) = *interaction {
                match target {
                    id if id == self.ids.warm => {
                        self.ui.set_theme(warm_theme());
                        self.sync_density_selection();
                    }
                    id if id == self.ids.cool => {
                        self.ui.set_theme(cool_theme());
                        self.sync_density_selection();
                    }
                    id if id == self.ids.roomy => self.apply_density(true),
                    id if id == self.ids.compact => self.apply_density(false),
                    id if id == self.ids.search => {
                        self.ui.set_local(
                            self.ids.content,
                            self.ui.properties().background,
                            overstory::Color::from_rgba8(248, 249, 252, 255),
                        );
                    }
                    id if id == self.ids.settings => {
                        self.ui.set_local(
                            self.ids.content,
                            self.ui.properties().background,
                            overstory::Color::from_rgba8(245, 243, 239, 255),
                        );
                    }
                    id if id == self.ids.deploy => {
                        self.ui.set_local(
                            self.ids.content,
                            self.ui.properties().background,
                            overstory::Color::from_rgba8(235, 245, 241, 255),
                        );
                    }
                    _ => {}
                }
            }
            if let Interaction::Submitted(target) = *interaction
                && target == self.ids.input
            {
                let text = self.ui.text_buffer(self.ids.input).to_owned();
                if !text.is_empty() {
                    let was_at_tail = self.is_at_tail();
                    self.transcript
                        .append(NewEntry::message(MessageRole::User, text.as_str()));
                    self.sync_messages();
                    if was_at_tail {
                        self.scroll_to_tail();
                    }
                    self.ui.clear_text_buffer(self.ids.input);
                }
            }
        }
    }

    fn apply_density(&mut self, roomy: bool) {
        self.roomy = roomy;
        let sidebar_width = if roomy { 188.0 } else { 152.0 };
        let root_padding = if roomy { 24.0 } else { 14.0 };
        let shell_gap = if roomy { 18.0 } else { 10.0 };
        let panel_padding = if roomy { 18.0 } else { 12.0 };
        let panel_gap = if roomy { 12.0 } else { 8.0 };
        let button_padding = if roomy { 14.0 } else { 10.0 };
        let button_height = if roomy { 48.0 } else { 36.0 };

        self.ui
            .set_local(self.ui.root(), self.ui.properties().padding, root_padding);
        self.ui
            .set_local(self.ids.shell, self.ui.properties().gap, shell_gap);
        self.ui
            .set_local(self.ids.sidebar, self.ui.properties().width, sidebar_width);
        self.ui.set_local(
            self.ids.sidebar,
            self.ui.properties().padding,
            panel_padding,
        );
        self.ui
            .set_local(self.ids.sidebar, self.ui.properties().gap, panel_gap);
        self.ui.set_local(
            self.ids.content,
            self.ui.properties().padding,
            panel_padding,
        );
        self.ui
            .set_local(self.ids.content, self.ui.properties().gap, panel_gap);

        for id in [
            self.ids.warm,
            self.ids.cool,
            self.ids.search,
            self.ids.settings,
            self.ids.deploy,
        ] {
            self.ui
                .set_local(id, self.ui.properties().padding, button_padding);
            self.ui
                .set_local(id, self.ui.properties().height, button_height);
        }
        for id in [self.ids.roomy, self.ids.compact] {
            self.ui
                .set_local(id, self.ui.properties().padding, button_padding);
            self.ui
                .set_local(id, self.ui.properties().height, button_height);
        }

        self.sync_shell_frame(root_padding);
        self.sync_density_selection();
    }

    fn resize_ui(&mut self, size: PhysicalSize<u32>, scale_factor: f64) {
        let width = f64::from(size.width.max(1)) / scale_factor;
        let height = f64::from(size.height.max(1)) / scale_factor;
        self.ui.set_view_rect(Rect::new(0.0, 0.0, width, height));
        self.sync_shell_frame(current_root_padding(self.roomy));
    }

    fn sync_shell_frame(&mut self, root_padding: f64) {
        let shell_gap = if self.roomy { 18.0 } else { 10.0 };
        let sidebar_width = if self.roomy { 188.0 } else { 152.0 };
        let shell_width = (self.ui.view_rect().width() - root_padding * 2.0).max(0.0);
        let shell_height = (self.ui.view_rect().height() - root_padding * 2.0).max(0.0);
        let content_width = (shell_width - sidebar_width - shell_gap).max(0.0);
        self.ui
            .set_local(self.ids.shell, self.ui.properties().width, shell_width);
        self.ui
            .set_local(self.ids.shell, self.ui.properties().height, shell_height);
        self.ui
            .set_local(self.ids.sidebar, self.ui.properties().width, sidebar_width);
        self.ui
            .set_local(self.ids.sidebar, self.ui.properties().height, shell_height);
        self.ui
            .set_local(self.ids.content, self.ui.properties().width, content_width);
        self.ui
            .set_local(self.ids.content, self.ui.properties().height, shell_height);
    }

    fn sync_density_selection(&mut self) {
        let button_bg = *self
            .ui
            .theme()
            .get(ThemeKeys::BUTTON_BACKGROUND)
            .expect("button background in theme");
        let primary_bg = *self
            .ui
            .theme()
            .get(ThemeKeys::PRIMARY_BACKGROUND)
            .expect("primary background in theme");
        let foreground = *self
            .ui
            .theme()
            .get(ThemeKeys::FOREGROUND)
            .expect("foreground in theme");

        let (roomy_bg, roomy_fg, compact_bg, compact_fg) = if self.roomy {
            (primary_bg, palette::css::WHITE, button_bg, foreground)
        } else {
            (button_bg, foreground, primary_bg, palette::css::WHITE)
        };

        self.ui
            .set_local(self.ids.roomy, self.ui.properties().background, roomy_bg);
        self.ui
            .set_local(self.ids.roomy, self.ui.properties().foreground, roomy_fg);
        self.ui.set_local(
            self.ids.compact,
            self.ui.properties().background,
            compact_bg,
        );
        self.ui.set_local(
            self.ids.compact,
            self.ui.properties().foreground,
            compact_fg,
        );
    }

    fn resize_active_surface(&mut self, size: PhysicalSize<u32>, scale_factor: f64) {
        self.resize_ui(size, scale_factor);

        let RenderState::Active {
            window,
            device,
            surface,
            surface_config,
            ..
        } = &mut self.render_state
        else {
            return;
        };

        if size.width == 0 || size.height == 0 {
            return;
        }
        surface_config.width = size.width;
        surface_config.height = size.height;
        surface.configure(device, surface_config);
        window.request_redraw();
    }

    fn redraw_active(&mut self) {
        let RenderState::Active {
            window,
            device,
            queue,
            surface,
            renderer,
            blit,
            ..
        } = &mut self.render_state
        else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        let scale_factor = window.scale_factor();
        let snapshot = self.ui.scene();
        let mut display_tree = snapshot.display_tree();
        display_tree.layout(
            &mut self.text,
            snapshot.view_rect().origin(),
            BoxConstraints::tight(snapshot.view_rect().size()),
        );
        let imaging_scene = imaging_scene_from_display_tree(&display_tree, scale_factor);

        let width = u16::try_from(size.width).expect("window width exceeds u16");
        let height = u16::try_from(size.height).expect("window height exceeds u16");
        let native = renderer
            .encode_scene(&imaging_scene, width, height)
            .expect("encode imaging scene");

        // Render to an offscreen Rgba8Unorm texture (the format vello_hybrid uses),
        // then blit to the surface texture (which may be Bgra8UnormSrgb on macOS).
        let offscreen = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width: u32::from(width),
                height: u32::from(height),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let offscreen_view = offscreen.create_view(&wgpu::TextureViewDescriptor::default());
        renderer
            .render_to_texture_view(
                &native,
                &offscreen_view,
                u32::from(width),
                u32::from(height),
            )
            .expect("render to offscreen");

        let frame = surface
            .get_current_texture()
            .expect("get current surface texture");
        let surface_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        blit.blit(device, queue, &offscreen_view, &surface_view);
        frame.present();
    }
}

impl ApplicationHandler for DemoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if matches!(self.render_state, RenderState::Active { .. }) {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("Overstory + imaging")
            .with_inner_size(winit::dpi::LogicalSize::new(960, 640));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let size = window.inner_size();

        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .expect("create wgpu surface");
        let (device, queue, surface_format) = pollster::block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::default(),
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                })
                .await
                .expect("request wgpu adapter");
            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .find(|f| **f == TextureFormat::Rgba8Unorm)
                .copied()
                .unwrap_or(caps.formats[0]);
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("overstory demo"),
                    ..Default::default()
                })
                .await
                .expect("request wgpu device");
            (device, queue, format)
        });
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        surface.configure(&device, &surface_config);

        self.resize_ui(size, window.scale_factor());
        let blit = BlitPipeline::new(&device, surface_format);
        let renderer = VelloHybridRenderer::new(device.clone(), queue.clone());
        window.request_redraw();
        self.render_state = RenderState::Active {
            window,
            device,
            queue,
            surface,
            surface_config,
            renderer,
            blit,
        };
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.render_state = RenderState::Suspended;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = (match &self.render_state {
            RenderState::Active { window, .. } if window.id() == window_id => Some(window.clone()),
            _ => None,
        }) else {
            return;
        };

        if let Some(translation) = self.reducer.reduce(window.scale_factor(), &event) {
            match translation {
                WindowEventTranslation::Keyboard(ref keyboard) => {
                    if keyboard.state.is_down()
                        && matches!(
                            keyboard.key,
                            overstory::ui_events::keyboard::Key::Named(
                                overstory::ui_events::keyboard::NamedKey::Escape
                            )
                        )
                    {
                        event_loop.exit();
                        return;
                    }
                    let interactions = self.ui.handle_keyboard_event(keyboard);
                    self.apply_interactions(&interactions);
                    window.request_redraw();
                    return;
                }
                WindowEventTranslation::Pointer(_) => {
                    self.process_pointer_translation(translation, &window);
                    return;
                }
            }
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.resize_active_surface(size, window.scale_factor());
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Space),
                        state: winit::event::ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                self.apply_density(true);
                self.ui.set_theme(default_theme());
                self.ui.set_local(
                    self.ids.content,
                    self.ui.properties().background,
                    overstory::Color::from_rgba8(255, 252, 246, 255),
                );
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw_active(),
            _ => {}
        }
    }
}

fn build_demo_ui() -> (Ui, DemoIds) {
    let mut ui = Ui::new(default_theme());

    let button_cascade = make_button_cascade(&ui);

    let shell = ui.append_child(ui.root(), ElementKind::Row);
    ui.set_local(shell, ui.properties().padding, 0.0);
    ui.set_local(shell, ui.properties().gap, 16.0);

    let sidebar = ui.append_child(shell, ElementKind::Panel);
    ui.add_layout_class(sidebar, LayoutClass::Sidebar);
    ui.set_local(sidebar, ui.properties().width, 176.0);
    ui.set_local(sidebar, ui.properties().padding, 16.0);
    ui.set_local(sidebar, ui.properties().gap, 10.0);

    let sidebar_column = ui.append_child(sidebar, ElementKind::Column);
    ui.set_local(sidebar_column, ui.properties().padding, 0.0);
    ui.set_local(sidebar_column, ui.properties().gap, 10.0);

    let warm = append_button(
        &mut ui,
        sidebar_column,
        &button_cascade,
        "Warm theme",
        false,
    );
    let cool = append_button(
        &mut ui,
        sidebar_column,
        &button_cascade,
        "Cool theme",
        false,
    );
    let roomy = append_button(&mut ui, sidebar_column, &button_cascade, "Roomy", false);
    let compact = append_button(&mut ui, sidebar_column, &button_cascade, "Compact", false);

    let content = ui.append_child(shell, ElementKind::Panel);
    ui.set_local(content, ui.properties().padding, 18.0);
    ui.set_local(content, ui.properties().gap, 12.0);

    let content_column = ui.append_child(content, ElementKind::Column);
    ui.set_local(content_column, ui.properties().padding, 0.0);
    ui.set_local(content_column, ui.properties().gap, 12.0);
    ui.set_local(content_column, ui.properties().fill, true);

    // Action button row at the top of the content area.
    let button_row = ui.append_child(content_column, ElementKind::Row);
    ui.set_local(button_row, ui.properties().padding, 0.0);
    ui.set_local(button_row, ui.properties().gap, 8.0);

    let search = append_button(&mut ui, button_row, &button_cascade, "Search", false);
    ui.set_local(search, ui.properties().fill, true);
    let settings = append_button(&mut ui, button_row, &button_cascade, "Settings", false);
    ui.set_local(settings, ui.properties().fill, true);
    let deploy = append_button(&mut ui, button_row, &button_cascade, "Deploy", true);
    ui.set_local(deploy, ui.properties().fill, true);
    ui.set_local(deploy, ui.properties().foreground, palette::css::WHITE);

    // Scrollable message area demonstrating ScrollView + TextBlock.
    let messages = ui.append_child(content_column, ElementKind::ScrollView);
    ui.set_local(messages, ui.properties().fill, true);
    ui.set_local(messages, ui.properties().padding, 12.0);
    ui.set_local(messages, ui.properties().gap, 10.0);
    ui.set_local(
        messages,
        ui.properties().background,
        overstory::Color::TRANSPARENT,
    );

    // Messages are populated from the transcript via DemoApp::sync_messages.

    // Text input at the bottom.
    let input = ui.append_child(content_column, ElementKind::TextInput);
    ui.set_local(input, ui.properties().height, 40.0);
    ui.set_local(input, ui.properties().padding, 8.0);
    ui.set_local(input, ui.properties().border_width, 1.0);
    ui.set_local(input, ui.properties().corner_radius, 6.0);

    (
        ui,
        DemoIds {
            shell,
            warm,
            cool,
            roomy,
            compact,
            sidebar,
            content,
            search,
            settings,
            deploy,
            messages,
            input,
        },
    )
}

fn current_root_padding(roomy: bool) -> f64 {
    if roomy { 24.0 } else { 14.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_sets_shell_and_panels_to_viewport_height() {
        let mut app = DemoApp::new();
        app.resize_ui(PhysicalSize::new(960, 640), 1.0);

        let scene = app.ui.scene();
        let expected_width = 960.0 - current_root_padding(true) * 2.0;
        let expected_height = 640.0 - current_root_padding(true) * 2.0;

        let shell_rect = scene
            .resolved_element(app.ids.shell)
            .expect("resolved element")
            .rect;
        assert_eq!(shell_rect.width(), expected_width);
        assert_eq!(shell_rect.height(), expected_height);

        let sidebar_rect = scene
            .resolved_element(app.ids.sidebar)
            .expect("resolved element")
            .rect;
        assert_eq!(sidebar_rect.width(), 188.0);
        assert_eq!(sidebar_rect.height(), expected_height);

        let content_rect = scene
            .resolved_element(app.ids.content)
            .expect("resolved element")
            .rect;
        assert_eq!(content_rect.width(), expected_width - 188.0 - 18.0);
        assert_eq!(content_rect.height(), expected_height);
    }

    #[test]
    fn density_toggle_updates_shell_frame() {
        let mut app = DemoApp::new();
        app.resize_ui(PhysicalSize::new(960, 640), 1.0);
        app.apply_density(false);

        let compact_height = 640.0 - current_root_padding(false) * 2.0;
        let compact_scene = app.ui.scene();
        let compact_shell = compact_scene
            .resolved_element(app.ids.shell)
            .expect("resolved element")
            .rect;
        let compact_sidebar = compact_scene
            .resolved_element(app.ids.sidebar)
            .expect("resolved element")
            .rect;
        let compact_content = compact_scene
            .resolved_element(app.ids.content)
            .expect("resolved element")
            .rect;
        assert_eq!(
            compact_shell.width(),
            960.0 - current_root_padding(false) * 2.0
        );
        assert_eq!(compact_shell.height(), compact_height);
        assert_eq!(compact_sidebar.width(), 152.0);
        assert_eq!(compact_sidebar.height(), compact_height);
        assert_eq!(
            compact_content.width(),
            compact_shell.width() - compact_sidebar.width() - 10.0
        );
        assert_eq!(compact_content.height(), compact_height);

        app.apply_density(true);
        let roomy_height = 640.0 - current_root_padding(true) * 2.0;
        let roomy_scene = app.ui.scene();
        let roomy_shell = roomy_scene
            .resolved_element(app.ids.shell)
            .expect("resolved element")
            .rect;
        let roomy_sidebar = roomy_scene
            .resolved_element(app.ids.sidebar)
            .expect("resolved element")
            .rect;
        let roomy_content = roomy_scene
            .resolved_element(app.ids.content)
            .expect("resolved element")
            .rect;
        assert_eq!(roomy_shell.height(), roomy_height);
        assert_eq!(roomy_sidebar.width(), 188.0);
        assert_eq!(roomy_sidebar.height(), roomy_height);
        assert_eq!(
            roomy_content.width(),
            roomy_shell.width() - roomy_sidebar.width() - 18.0
        );
        assert_eq!(roomy_content.height(), roomy_height);
    }

    #[test]
    fn fill_child_takes_remaining_space() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 400.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append_child(ui.root(), ElementKind::Column);
        ui.set_local(column, ui.properties().padding, 0.0);
        ui.set_local(column, ui.properties().gap, 0.0);
        ui.set_local(column, ui.properties().height, 400.0);

        let top = ui.append_child(column, ElementKind::Button);
        ui.set_local(top, ui.properties().height, 50.0);

        let middle = ui.append_child(column, ElementKind::Panel);
        ui.set_local(middle, ui.properties().fill, true);

        let bottom = ui.append_child(column, ElementKind::Button);
        ui.set_local(bottom, ui.properties().height, 50.0);

        let scene = ui.scene();
        assert_eq!(scene.resolved_element(top).unwrap().rect.height(), 50.0);
        assert_eq!(
            scene.resolved_element(middle).unwrap().rect.height(),
            300.0
        );
        assert_eq!(
            scene.resolved_element(bottom).unwrap().rect.height(),
            50.0
        );
        assert_eq!(scene.resolved_element(bottom).unwrap().rect.y0, 350.0);
    }

    #[test]
    fn multiple_fill_children_share_space() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 300.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append_child(ui.root(), ElementKind::Column);
        ui.set_local(column, ui.properties().padding, 0.0);
        ui.set_local(column, ui.properties().gap, 0.0);
        ui.set_local(column, ui.properties().height, 300.0);

        let first = ui.append_child(column, ElementKind::Panel);
        ui.set_local(first, ui.properties().fill, true);

        let second = ui.append_child(column, ElementKind::Panel);
        ui.set_local(second, ui.properties().fill, true);

        let scene = ui.scene();
        assert_eq!(
            scene.resolved_element(first).unwrap().rect.height(),
            150.0
        );
        assert_eq!(
            scene.resolved_element(second).unwrap().rect.height(),
            150.0
        );
        assert_eq!(scene.resolved_element(second).unwrap().rect.y0, 150.0);
    }

    #[test]
    fn scroll_view_offsets_children() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 200.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let scroll = ui.append_child(ui.root(), ElementKind::ScrollView);
        ui.set_local(scroll, ui.properties().padding, 0.0);
        ui.set_local(scroll, ui.properties().gap, 0.0);
        ui.set_local(scroll, ui.properties().height, 200.0);

        let a = ui.append_child(scroll, ElementKind::Button);
        ui.set_local(a, ui.properties().height, 100.0);
        let b = ui.append_child(scroll, ElementKind::Button);
        ui.set_local(b, ui.properties().height, 100.0);
        let c = ui.append_child(scroll, ElementKind::Button);
        ui.set_local(c, ui.properties().height, 100.0);

        // No scroll: first child at y=0
        let scene = ui.scene();
        assert_eq!(scene.resolved_element(a).unwrap().rect.y0, 0.0);
        assert_eq!(scene.resolved_element(c).unwrap().rect.y0, 200.0);

        // After scrolling, the resolved elements keep their layout positions
        // but the scroll_offset is recorded on the scroll view.
        ui.set_scroll_offset(scroll, 50.0);
        assert_eq!(ui.scroll_offset(scroll), 50.0);
    }

    #[test]
    fn scroll_view_tracks_content_height() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 200.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let scroll = ui.append_child(ui.root(), ElementKind::ScrollView);
        ui.set_local(scroll, ui.properties().padding, 0.0);
        ui.set_local(scroll, ui.properties().gap, 0.0);
        ui.set_local(scroll, ui.properties().height, 200.0);

        let a = ui.append_child(scroll, ElementKind::Button);
        ui.set_local(a, ui.properties().height, 100.0);
        let b = ui.append_child(scroll, ElementKind::Button);
        ui.set_local(b, ui.properties().height, 100.0);
        let c = ui.append_child(scroll, ElementKind::Button);
        ui.set_local(c, ui.properties().height, 100.0);

        let _ = ui.scene();
        assert_eq!(ui.content_height(scroll), 300.0);
    }

    #[test]
    fn scroll_offset_clamps_to_zero() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 200.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);

        let scroll = ui.append_child(ui.root(), ElementKind::ScrollView);
        ui.set_local(scroll, ui.properties().height, 200.0);

        ui.set_scroll_offset(scroll, -50.0);
        assert_eq!(ui.scroll_offset(scroll), 0.0);
    }

    #[test]
    fn custom_font_size_in_resolved_element() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 100.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);

        let button = ui.append_child(ui.root(), ElementKind::Button);
        ui.set_label(button, "Big");
        ui.set_local(button, ui.properties().font_size, 32.0);

        let scene = ui.scene();
        let resolved = scene.resolved_element(button).unwrap();
        assert_eq!(resolved.font_size, 32.0);
    }

    #[test]
    fn theme_font_size_used_as_default() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 100.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);

        let button = ui.append_child(ui.root(), ElementKind::Button);
        ui.set_label(button, "Normal");

        let scene = ui.scene();
        let resolved = scene.resolved_element(button).unwrap();
        assert_eq!(resolved.font_size, 16.0);
        assert_eq!(resolved.label_padding, 12.0);
    }

    #[test]
    fn text_block_measures_height_from_label() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 400.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append_child(ui.root(), ElementKind::Column);
        ui.set_local(column, ui.properties().padding, 0.0);
        ui.set_local(column, ui.properties().gap, 8.0);

        let short = ui.append_child(column, ElementKind::TextBlock);
        ui.set_label(short, "Hello");

        let long = ui.append_child(column, ElementKind::TextBlock);
        ui.set_label(
            long,
            "This is a much longer message that should wrap to multiple lines in a narrow container",
        );

        let scene = ui.scene();
        let short_rect = scene.resolved_element(short).unwrap().rect;
        let long_rect = scene.resolved_element(long).unwrap().rect;

        assert!(short_rect.height() > 0.0, "short text should have height");
        assert!(
            long_rect.height() > short_rect.height(),
            "longer text should be taller: short={} long={}",
            short_rect.height(),
            long_rect.height()
        );
    }

    #[test]
    fn text_block_stacks_in_column() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 300.0, 600.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append_child(ui.root(), ElementKind::Column);
        ui.set_local(column, ui.properties().padding, 0.0);
        ui.set_local(column, ui.properties().gap, 0.0);

        let a = ui.append_child(column, ElementKind::TextBlock);
        ui.set_label(a, "First message");

        let b = ui.append_child(column, ElementKind::TextBlock);
        ui.set_label(b, "Second message");

        let scene = ui.scene();
        let a_rect = scene.resolved_element(a).unwrap().rect;
        let b_rect = scene.resolved_element(b).unwrap().rect;

        assert_eq!(a_rect.y0, 0.0);
        assert_eq!(b_rect.y0, a_rect.y1, "second block should start where first ends");
    }

    #[test]
    fn message_scroll_view_fills_content_area() {
        let mut app = DemoApp::new();
        app.resize_ui(PhysicalSize::new(960, 640), 1.0);

        let scene = app.ui.scene();
        let content_rect = scene
            .resolved_element(app.ids.content)
            .expect("content panel")
            .rect;
        let messages_rect = scene
            .resolved_element(app.ids.messages)
            .expect("messages scroll view")
            .rect;

        // The messages ScrollView should be inside the content panel.
        assert!(messages_rect.y0 >= content_rect.y0);
        assert!(messages_rect.y1 <= content_rect.y1 + 1.0);
        // It should fill most of the content height (below the button row).
        assert!(
            messages_rect.height() > content_rect.height() * 0.5,
            "messages should fill most of content: messages_h={} content_h={}",
            messages_rect.height(),
            content_rect.height()
        );
    }

    #[test]
    fn scroll_event_over_messages_updates_offset() {
        use overstory::ui_events::pointer::{
            PointerButtons, PointerId, PointerInfo, PointerScrollEvent, PointerState, PointerType,
        };
        use overstory::ui_events::ScrollDelta;

        let mut app = DemoApp::new();
        app.resize_ui(PhysicalSize::new(960, 640), 1.0);

        // Find where the messages scroll view is.
        let scene = app.ui.scene();
        let msg_rect = scene
            .resolved_element(app.ids.messages)
            .expect("messages scroll view")
            .rect;
        let mid_x = (msg_rect.x0 + msg_rect.x1) / 2.0;
        let mid_y = (msg_rect.y0 + msg_rect.y1) / 2.0;

        assert_eq!(app.ui.scroll_offset(app.ids.messages), 0.0);

        // Synthesize a scroll event over the messages area.
        let mut state = PointerState::default();
        state.position.x = mid_x;
        state.position.y = mid_y;
        state.buttons = PointerButtons::new();
        state.count = 0;
        state.scale_factor = 1.0;
        state.time = 100;

        let scroll_event = overstory::ui_events::pointer::PointerEvent::Scroll(PointerScrollEvent {
            pointer: PointerInfo {
                pointer_id: Some(PointerId::PRIMARY),
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
            delta: ScrollDelta::LineDelta(0.0, -3.0),
            state,
        });

        let batch = app.ui.handle_pointer_event(&scroll_event);
        assert!(
            app.ui.scroll_offset(app.ids.messages) > 0.0,
            "scroll offset should have changed, got {}",
            app.ui.scroll_offset(app.ids.messages)
        );
        assert!(
            batch
                .events()
                .iter()
                .any(|e| matches!(e, Interaction::Scrolled(_))),
            "should emit Scrolled interaction"
        );
    }

    #[test]
    fn text_input_keyboard_appends_and_submits() {
        use overstory::ui_events::keyboard::{
            Code, Key, KeyState, KeyboardEvent, Location, Modifiers, NamedKey,
        };

        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 400.0, 200.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);

        let input = ui.append_child(ui.root(), ElementKind::TextInput);
        ui.set_local(input, ui.properties().height, 40.0);
        ui.set_focus(input);

        let key_event = |key: Key| KeyboardEvent {
            key,
            code: Code::Unidentified,
            state: KeyState::Down,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            repeat: false,
            is_composing: false,
        };

        let _ = ui.handle_keyboard_event(&key_event(Key::Character("H".into())));
        let _ = ui.handle_keyboard_event(&key_event(Key::Character("i".into())));
        assert_eq!(ui.text_buffer(input), "Hi");

        let _ = ui.handle_keyboard_event(&key_event(Key::Named(NamedKey::Backspace)));
        assert_eq!(ui.text_buffer(input), "H");

        let batch = ui.handle_keyboard_event(&key_event(Key::Named(NamedKey::Enter)));
        assert!(batch.events().iter().any(|e| matches!(e, Interaction::Submitted(_))));
    }

    #[test]
    fn density_selection_follows_current_mode() {
        let mut app = DemoApp::new();

        let theme_fg = *app
            .ui
            .theme()
            .get(ThemeKeys::FOREGROUND)
            .expect("foreground in theme");
        let theme_button = *app
            .ui
            .theme()
            .get(ThemeKeys::BUTTON_BACKGROUND)
            .expect("button background in theme");
        let theme_primary = *app
            .ui
            .theme()
            .get(ThemeKeys::PRIMARY_BACKGROUND)
            .expect("primary background in theme");

        let scene = app.ui.scene();
        let roomy = scene
            .resolved_element(app.ids.roomy)
            .expect("roomy resolved element");
        let compact = scene
            .resolved_element(app.ids.compact)
            .expect("compact resolved element");
        assert_eq!(roomy.background, theme_primary);
        assert_eq!(compact.background, theme_button);

        app.apply_density(false);
        let scene = app.ui.scene();
        let roomy = scene
            .resolved_element(app.ids.roomy)
            .expect("roomy resolved element");
        let compact = scene
            .resolved_element(app.ids.compact)
            .expect("compact resolved element");
        assert_eq!(roomy.background, theme_button);
        assert_eq!(compact.background, theme_primary);
        assert_eq!(roomy.foreground, theme_fg);
    }
}

fn append_button(
    ui: &mut Ui,
    parent: ElementId,
    cascade: &StyleCascade,
    label: &str,
    primary: bool,
) -> ElementId {
    let button = ui.append_child(parent, ElementKind::Button);
    ui.set_label(button, label);
    ui.set_style(button, cascade.clone());
    ui.set_local(button, ui.properties().height, 42.0);
    if primary {
        ui.add_button_class(button, ButtonClass::Primary);
        ui.set_local(button, ui.properties().foreground, palette::css::WHITE);
    }
    button
}

fn make_button_cascade(ui: &Ui) -> StyleCascade {
    let base = StyleBuilder::new()
        .set(ui.properties().border_width, 1.0)
        .set(ui.properties().padding, 12.0)
        .build();
    let hover = StyleBuilder::new()
        .set(ui.properties().border_width, 2.0)
        .build();
    let pressed = StyleBuilder::new()
        .set(ui.properties().border_width, 3.0)
        .build();
    let selector_hover = Selector {
        type_tag: Some(overstory::TYPE_BUTTON),
        required_classes: IdSet::default(),
        required_pseudos: IdSet::from_ids([overstory::PSEUDO_HOVER]),
    };
    let selector_pressed = Selector {
        type_tag: Some(overstory::TYPE_BUTTON),
        required_classes: IdSet::default(),
        required_pseudos: IdSet::from_ids([overstory::PSEUDO_PRESSED]),
    };
    let sheet = StyleSheetBuilder::new()
        .rule(selector_hover, hover)
        .rule(selector_pressed, pressed)
        .build();
    StyleCascadeBuilder::new()
        .push_style(StyleOrigin::Base, base)
        .push_sheet(StyleOrigin::Sheet, sheet)
        .build()
}

fn warm_theme() -> Theme {
    default_theme()
}

fn cool_theme() -> Theme {
    ThemeBuilder::new()
        .set(
            ThemeKeys::ROOT_BACKGROUND,
            overstory::Color::from_rgba8(232, 239, 247, 255),
        )
        .set(
            ThemeKeys::PANEL_BACKGROUND,
            overstory::Color::from_rgba8(244, 248, 252, 255),
        )
        .set(
            ThemeKeys::SIDEBAR_BACKGROUND,
            overstory::Color::from_rgba8(214, 227, 240, 255),
        )
        .set(
            ThemeKeys::BUTTON_BACKGROUND,
            overstory::Color::from_rgba8(229, 237, 245, 255),
        )
        .set(
            ThemeKeys::BUTTON_HOVER_BACKGROUND,
            overstory::Color::from_rgba8(220, 231, 242, 255),
        )
        .set(
            ThemeKeys::BUTTON_PRESSED_BACKGROUND,
            overstory::Color::from_rgba8(202, 218, 232, 255),
        )
        .set(
            ThemeKeys::PRIMARY_BACKGROUND,
            overstory::Color::from_rgba8(36, 82, 138, 255),
        )
        .set(
            ThemeKeys::PRIMARY_HOVER_BACKGROUND,
            overstory::Color::from_rgba8(48, 101, 165, 255),
        )
        .set(
            ThemeKeys::PRIMARY_PRESSED_BACKGROUND,
            overstory::Color::from_rgba8(28, 66, 112, 255),
        )
        .set(
            ThemeKeys::FOREGROUND,
            overstory::Color::from_rgba8(25, 33, 42, 255),
        )
        .set(
            ThemeKeys::BORDER_COLOR,
            overstory::Color::from_rgba8(123, 141, 160, 255),
        )
        .set(ThemeKeys::CORNER_RADIUS, 10.0_f64)
        .set(ThemeKeys::PADDING, 16.0_f64)
        .set(ThemeKeys::GAP, 12.0_f64)
        .set(ThemeKeys::BUTTON_HEIGHT, 44.0_f64)
        .build()
}
