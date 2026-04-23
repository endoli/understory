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
use overstory::widgets::{DockPaneController, DockPaneIds, DockPaneStyle};
use overstory::{
    ButtonClass, Color, ElementId, Interaction, LayoutClass, ThemeKeys, Ui, default_theme,
};
use ui_events_winit::{WindowEventReducer, WindowEventTranslation};
use understory_display::{BoxConstraints, TextEngine};
use understory_examples::llm_api::{self, ApiConfig, StreamEvent};
use understory_examples::overstory_display::imaging_scene_from_display_tree;
use understory_inspector::{Inspector, InspectorConfig, InspectorModel};
use understory_outline::OutlineModel;
use understory_style::{
    IdSet, Selector, StyleBuilder, StyleCascade, StyleCascadeBuilder, StyleOrigin,
    StyleSheetBuilder, Theme, ThemeBuilder, TypeTag,
};
use understory_transcript::{EntryId, EntryKind, EntryStatus, MessageRole, NewEntry, Transcript};
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

// ---------------------------------------------------------------------------
// Element tree model for the inspector
// ---------------------------------------------------------------------------

/// Snapshot of the element tree for use with `understory_inspector`.
struct ElementTreeModel {
    nodes: Vec<ElementNode>,
    root: ElementId,
    /// Subtree roots to exclude from the tree view.
    excluded: Vec<ElementId>,
}

struct ElementNode {
    parent: Option<ElementId>,
    children: Vec<ElementId>,
    type_tag: TypeTag,
    label: Option<String>,
}

impl ElementTreeModel {
    fn from_ui(ui: &Ui, excluded: Vec<ElementId>) -> Self {
        let nodes = ui
            .elements()
            .iter()
            .map(|e| ElementNode {
                parent: e.parent(),
                children: e.children().to_vec(),
                type_tag: e.type_tag(),
                label: e.label().map(String::from),
            })
            .collect();
        Self {
            nodes,
            root: ui.root(),
            excluded,
        }
    }

    fn node(&self, key: &ElementId) -> Option<&ElementNode> {
        self.nodes.get(key.index())
    }

    fn is_excluded(&self, id: ElementId) -> bool {
        // Check if id is any excluded root or a descendant of one.
        let mut cur = Some(id);
        while let Some(c) = cur {
            if self.excluded.contains(&c) {
                return true;
            }
            cur = self.nodes.get(c.index()).and_then(|n| n.parent);
        }
        false
    }
}

impl OutlineModel for ElementTreeModel {
    type Key = ElementId;
    type Item = String;

    fn first_root_key(&self) -> Option<Self::Key> {
        Some(self.root)
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        key.index() < self.nodes.len() && !self.is_excluded(*key)
    }

    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
        let node = self.node(key)?;
        let parent = self.node(&node.parent?)?;
        let pos = parent.children.iter().position(|c| c == key)?;
        parent.children[pos + 1..]
            .iter()
            .find(|c| !self.is_excluded(**c))
            .copied()
    }

    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
        let node = self.node(key)?;
        node.children
            .iter()
            .find(|c| !self.is_excluded(**c))
            .copied()
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        let node = self.node(key)?;
        let name = type_tag_name(node.type_tag);
        if let Some(label) = &node.label {
            let short = if label.len() > 20 {
                format!("{}...", &label[..20])
            } else {
                label.clone()
            };
            Some(format!("{name} \"{short}\""))
        } else {
            Some(name.to_string())
        }
    }
}

impl InspectorModel for ElementTreeModel {
    fn parent_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.node(key)?.parent
    }
}

fn type_tag_name(tag: TypeTag) -> &'static str {
    match tag {
        overstory::TYPE_ROOT => "Root",
        overstory::TYPE_PANEL => "Panel",
        overstory::TYPE_ROW => "Row",
        overstory::TYPE_COLUMN => "Column",
        overstory::TYPE_BUTTON => "Button",
        overstory::TYPE_SPACER => "Spacer",
        overstory::TYPE_SCROLL_VIEW => "ScrollView",
        overstory::TYPE_SPLITTER => "Splitter",
        overstory::TYPE_TEXT_BLOCK => "TextBlock",
        overstory::TYPE_TEXT_INPUT => "TextInput",
        overstory::TYPE_TOOLTIP => "Tooltip",
        _ => "Unknown",
    }
}

// ---------------------------------------------------------------------------

#[derive(Debug)]
struct DemoIds {
    shell: ElementId,
    light: ElementId,
    dark: ElementId,
    roomy: ElementId,
    compact: ElementId,
    sidebar: ElementId,
    splitter: ElementId,
    content: ElementId,
    inspector_splitter: ElementId,
    inspector_panel: ElementId,
    inspector_toggle: ElementId,
    inspector_tree_label: ElementId,
    inspector_props_label: ElementId,
    search: ElementId,
    settings: ElementId,
    deploy: ElementId,
    messages: ElementId,
    input: ElementId,
    inspector_tree: ElementId,
    inspector_props: ElementId,
}

#[derive(Clone, Debug)]
struct PropertyBadge {
    label: String,
    background: Color,
    foreground: Color,
}

#[derive(Clone, Debug)]
enum PropertyValue {
    Text(String),
    Color(Color),
    Badges(Vec<PropertyBadge>),
}

#[derive(Clone, Debug)]
struct PropertyRow {
    name: String,
    value: PropertyValue,
}

#[derive(Clone, Debug)]
struct PropertyRowElements {
    row: ElementId,
    name: ElementId,
    detail: ElementId,
    swatch: ElementId,
    value: ElementId,
    chips: Vec<ElementId>,
}

#[allow(
    clippy::large_enum_variant,
    reason = "This is example-local window/render state; keeping the active path inline is clearer than boxing renderer fields."
)]
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
    /// Receiver for LLM API streaming events.
    api_receiver: Option<std::sync::mpsc::Receiver<StreamEvent>>,
    /// The transcript entry currently being streamed into.
    streaming_entry: Option<EntryId>,
    /// Tool calls with their results, waiting to be sent back to the API.
    pending_tool_calls: Vec<(String, String, serde_json::Value, String)>,
    /// Map from transcript entry to UI element, for live updates.
    entry_elements: Vec<(EntryId, ElementId)>,
    /// Messages sent to the LLM (for context continuity).
    api_messages: Vec<llm_api::Message>,
    /// Accumulated text for the current assistant stream.
    streaming_api_text: String,
    /// API configuration.
    api_config: ApiConfig,
    /// Monotonic epoch for converting between Instant and u64 nanos.
    epoch: std::time::Instant,
    /// Element tree inspector controller.
    inspector: Inspector<ElementTreeModel>,
    /// Currently selected element in the inspector.
    selected_element: Option<ElementId>,
    /// UI elements representing tree rows.
    tree_row_elements: Vec<ElementId>,
    /// UI elements representing property rows.
    prop_row_elements: Vec<PropertyRowElements>,
    /// Whether the inspector tree has been initially expanded.
    inspector_initialized: bool,
    inspector_dock: DockPaneController,
}

impl DemoApp {
    fn new() -> Self {
        let (ui, ids) = build_demo_ui();
        let config = ApiConfig::from_env();
        let mut transcript = Transcript::new();
        transcript.append(NewEntry::message(
            MessageRole::Assistant,
            format!(
                "Welcome to the Overstory chat harness (model: {}). \
                 Try asking me to switch themes or change the density — I have tools for that. \
                 Set LLM_BASE_URL / LLM_MODEL to configure.",
                config.model
            ),
        ));

        let inspector_model = ElementTreeModel::from_ui(&ui, vec![ids.inspector_panel]);
        let inspector = Inspector::new(inspector_model, InspectorConfig::fixed_rows(16.0, 200.0));
        let mut inspector_dock = DockPaneController::new(
            DockPaneIds {
                pane: ids.inspector_panel,
                splitter: ids.inspector_splitter,
                toggle: ids.inspector_toggle,
            },
            vec![
                ids.inspector_tree_label,
                ids.inspector_tree,
                ids.inspector_props_label,
                ids.inspector_props,
            ],
            current_inspector_width(true),
            inspector_rail_width(),
        );
        inspector_dock.set_style(inspector_dock_style(true));

        let mut app = Self {
            ui,
            ids,
            roomy: true,
            reducer: WindowEventReducer::default(),
            text: TextEngine::new(),
            render_state: RenderState::Suspended,
            transcript,
            message_count: 0,
            api_receiver: None,
            streaming_entry: None,
            pending_tool_calls: Vec::new(),
            entry_elements: Vec::new(),
            api_messages: Vec::new(),
            streaming_api_text: String::new(),
            api_config: config,
            epoch: std::time::Instant::now(),
            inspector,
            selected_element: None,
            tree_row_elements: Vec::new(),
            prop_row_elements: Vec::new(),
            inspector_initialized: false,
            inspector_dock,
        };
        app.sync_messages();
        app.apply_density(true);
        app.ui.set_now(app.now_nanos());
        app.ui.set_focus(app.ids.input);
        app.sync_inspector();
        app
    }

    /// Returns true if the messages ScrollView is currently scrolled to the
    /// bottom (or content fits within the viewport).
    fn build_tools() -> Vec<llm_api::Tool> {
        vec![
            llm_api::Tool::function(
                "set_theme",
                "Switch the UI theme",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "theme": {
                            "type": "string",
                            "enum": ["light", "dark"],
                            "description": "The theme to switch to"
                        }
                    },
                    "required": ["theme"]
                }),
            ),
            llm_api::Tool::function(
                "set_density",
                "Switch the UI density",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "density": {
                            "type": "string",
                            "enum": ["roomy", "compact"],
                            "description": "The density mode to switch to"
                        }
                    },
                    "required": ["density"]
                }),
            ),
        ]
    }

    fn send_to_llm(&mut self) {
        let messages = self.current_api_request_messages();
        let tools = Self::build_tools();
        self.api_receiver = Some(llm_api::send_streaming(&self.api_config, messages, tools));
        self.streaming_api_text.clear();

        // Create an in-progress assistant entry as a typing indicator.
        let entry_id = self.transcript.append(
            NewEntry::message(MessageRole::Assistant, "").with_status(EntryStatus::InProgress),
        );
        self.streaming_entry = Some(entry_id);
        self.sync_messages();
    }

    fn entry_display_state(&self, entry_id: EntryId) -> Option<(String, bool)> {
        let entry = self.transcript.entry(entry_id)?;
        let EntryKind::Message(msg) = &entry.kind else {
            return None;
        };
        let body = msg.body.as_text().unwrap_or("");
        let visible = !(entry.status == EntryStatus::Complete && body.is_empty());
        let text = if entry.status == EntryStatus::InProgress && body.is_empty() {
            "...".to_string()
        } else {
            body.to_string()
        };
        Some((text, visible))
    }

    fn current_api_request_messages(&self) -> Vec<llm_api::Message> {
        let mut messages = vec![llm_api::Message::text(
            "system",
            "You are an AI assistant embedded in the Overstory UI demo. \
             You can help the user and also control the UI using tools. \
             Keep responses concise.",
        )];
        messages.extend(self.api_messages.clone());
        messages
    }

    fn refresh_entry_element(&mut self, entry_id: EntryId) {
        let Some(element) = self.element_for_entry(entry_id) else {
            return;
        };
        let Some((label, visible)) = self.entry_display_state(entry_id) else {
            return;
        };
        self.ui.set_label(element, label);
        self.ui
            .set_local(element, self.ui.properties().visible, visible);
    }

    fn finish_streaming_without_tools(&mut self) {
        self.complete_streaming_entry();
        if !self.streaming_api_text.is_empty() {
            self.api_messages.push(llm_api::Message::text(
                "assistant",
                &self.streaming_api_text,
            ));
        }
        self.streaming_api_text.clear();
    }

    fn poll_api(&mut self) {
        let Some(rx) = self.api_receiver.take() else {
            return;
        };

        let was_at_tail = self.is_at_tail();
        let mut needs_redraw = false;
        let mut done = false;

        while let Ok(event) = rx.try_recv() {
            match event {
                StreamEvent::TextDelta(text) => {
                    if let Some(eid) = self.streaming_entry {
                        let _ = self.transcript.append_chunk(eid, text.as_str());
                        self.streaming_api_text.push_str(&text);
                        self.refresh_entry_element(eid);
                    }
                    needs_redraw = true;
                }
                StreamEvent::ToolCall { id, name, input } => {
                    // Complete the streaming entry.
                    self.complete_streaming_entry();
                    // Execute the tool.
                    eprintln!("[tool call] {name}({input})");
                    let result = self.execute_tool(&name, &input);
                    eprintln!("[tool result] {result}");
                    // Add tool result to transcript for visibility.
                    self.transcript.append(NewEntry::message(
                        MessageRole::Assistant,
                        format!("[tool: {name}] {result}"),
                    ));
                    self.sync_messages();
                    self.pending_tool_calls.push((id, name, input, result));
                    needs_redraw = true;
                }
                StreamEvent::Done => {
                    // If there were tool calls, send tool results back.
                    if !self.pending_tool_calls.is_empty() {
                        self.complete_streaming_entry();
                        self.send_tool_results();
                    } else {
                        self.finish_streaming_without_tools();
                    }
                    done = true;
                    needs_redraw = true;
                }
                StreamEvent::Error(err) => {
                    // Mark the streaming entry as failed if it exists.
                    if let Some(eid) = self.streaming_entry.take() {
                        let _ = self.transcript.set_status(eid, EntryStatus::Failed);
                        if let Some(element) = self.element_for_entry(eid) {
                            self.ui.set_label(element, format!("[error: {err}]"));
                        }
                        self.streaming_api_text.clear();
                    } else {
                        self.transcript.append(NewEntry::message(
                            MessageRole::Assistant,
                            format!("[error: {err}]"),
                        ));
                        self.sync_messages();
                    }
                    done = true;
                    needs_redraw = true;
                }
            }
        }

        if done && self.pending_tool_calls.is_empty() {
            // Stream finished and no pending tool results — drop the receiver.
        } else {
            // Put the receiver back for continued polling.
            self.api_receiver = Some(rx);
        }

        if needs_redraw && was_at_tail {
            self.scroll_to_tail();
        }
    }

    fn element_for_entry(&self, entry_id: EntryId) -> Option<ElementId> {
        self.entry_elements
            .iter()
            .find(|(eid, _)| *eid == entry_id)
            .map(|(_, elem)| *elem)
    }

    fn complete_streaming_entry(&mut self) {
        if let Some(eid) = self.streaming_entry.take() {
            let _ = self.transcript.set_status(eid, EntryStatus::Complete);
            self.refresh_entry_element(eid);
        }
    }

    fn execute_tool(&mut self, name: &str, input: &serde_json::Value) -> String {
        match name {
            "set_theme" => {
                let theme_name = input["theme"].as_str().unwrap_or("light");
                match theme_name {
                    "dark" => {
                        self.ui.set_theme(dark_theme());
                        self.sync_density_selection();
                        "Switched to dark theme".into()
                    }
                    _ => {
                        self.ui.set_theme(light_theme());
                        self.sync_density_selection();
                        "Switched to light theme".into()
                    }
                }
            }
            "set_density" => {
                let density = input["density"].as_str().unwrap_or("roomy");
                match density {
                    "compact" => {
                        self.apply_density(false);
                        "Switched to compact density".into()
                    }
                    _ => {
                        self.apply_density(true);
                        "Switched to roomy density".into()
                    }
                }
            }
            _ => format!("Unknown tool: {name}"),
        }
    }

    fn send_tool_results(&mut self) {
        let tool_calls = core::mem::take(&mut self.pending_tool_calls);

        let content =
            (!self.streaming_api_text.is_empty()).then(|| self.streaming_api_text.clone());
        let tc_messages: Vec<llm_api::ToolCallMessage> = tool_calls
            .iter()
            .map(|(id, name, input, _)| llm_api::ToolCallMessage {
                id: id.clone(),
                call_type: "function".into(),
                function: llm_api::FunctionCall {
                    name: name.clone(),
                    arguments: serde_json::to_string(input).unwrap_or_default(),
                },
            })
            .collect();

        self.api_messages
            .push(llm_api::Message::assistant_with_tools(content, tc_messages));

        // Add individual tool result messages.
        for (id, _, _, result) in &tool_calls {
            self.api_messages
                .push(llm_api::Message::tool_result(id, result));
        }

        let messages = self.current_api_request_messages();
        let tools = Self::build_tools();
        self.api_receiver = Some(llm_api::send_streaming(&self.api_config, messages, tools));
        self.streaming_api_text.clear();

        // Create a new in-progress entry for the follow-up response.
        let entry_id = self.transcript.append(
            NewEntry::message(MessageRole::Assistant, "").with_status(EntryStatus::InProgress),
        );
        self.streaming_entry = Some(entry_id);
        self.sync_messages();
    }

    fn now_nanos(&self) -> u64 {
        self.epoch.elapsed().as_nanos() as u64
    }

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
        self.ui
            .set_scroll_offset(self.ids.messages, (content_h - viewport_h).max(0.0));
    }

    fn sync_messages(&mut self) {
        let entries: Vec<_> = self.transcript.entries().to_vec();
        for entry in entries.iter().skip(self.message_count) {
            if let EntryKind::Message(msg) = &entry.kind {
                let is_user = msg.role == MessageRole::User;
                let block = self
                    .ui
                    .append_child(self.ids.messages, overstory::TYPE_TEXT_BLOCK);
                self.ui
                    .set_local(block, self.ui.properties().label_padding, 8.0);
                self.ui.set_local(block, self.ui.properties().padding, 8.0);
                self.ui
                    .set_local(block, self.ui.properties().corner_radius, 8.0);
                if is_user {
                    self.ui
                        .add_class(block, overstory::MessageClass::User.class_id());
                }
                self.entry_elements.push((entry.id, block));
                self.refresh_entry_element(entry.id);
            }
        }
        self.message_count = entries.len();
    }

    fn sync_inspector(&mut self) {
        // Rebuild model from current UI state, excluding the inspector panels.
        *self.inspector.model_mut() =
            ElementTreeModel::from_ui(&self.ui, vec![self.ids.inspector_panel]);
        self.inspector.mark_dirty();
        self.inspector.sync();

        // Expand all nodes on first sync so the tree starts fully open.
        if !self.inspector_initialized {
            self.inspector_initialized = true;
            for _ in 0..10 {
                let unexpanded: Vec<_> = self
                    .inspector
                    .visible_rows()
                    .iter()
                    .filter(|r| r.has_children && !r.is_expanded)
                    .map(|r| r.key)
                    .collect();
                if unexpanded.is_empty() {
                    break;
                }
                for key in unexpanded {
                    let _ = self.inspector.expand(key);
                }
            }
        }

        let rows = self.inspector.visible_rows().to_vec();

        // Ensure we have enough TextBlock elements for the visible rows.
        while self.tree_row_elements.len() < rows.len() {
            let block = self
                .ui
                .append_child(self.ids.inspector_tree, overstory::TYPE_TEXT_BLOCK);
            self.ui
                .set_local(block, self.ui.properties().label_padding, 2.0);
            self.ui.set_local(block, self.ui.properties().padding, 1.0);
            self.ui
                .set_local(block, self.ui.properties().font_size, 11.0);
            self.ui
                .set_local(block, self.ui.properties().pickable, true);
            self.tree_row_elements.push(block);
        }

        // Update labels and hide excess rows.
        for (i, block) in self.tree_row_elements.iter().enumerate() {
            if let Some(row) = rows.get(i) {
                let item = self.inspector.item(&row.key).unwrap_or_default();
                let indent = "  ".repeat(row.depth);
                let disclosure = match (row.has_children, row.is_expanded) {
                    (true, true) => "▾ ",
                    (true, false) => "▸ ",
                    (false, _) => "  ",
                };
                let selected = self.selected_element == Some(row.key);
                let marker = if selected { "● " } else { "" };
                let label = format!("{indent}{disclosure}{marker}{item}");
                self.ui.set_label(*block, label);
                self.ui
                    .set_local(*block, self.ui.properties().visible, true);
            } else {
                self.ui.set_label(*block, "");
                self.ui
                    .set_local(*block, self.ui.properties().visible, false);
            }
        }
    }

    fn sync_property_grid(&mut self) {
        let primary_bg = *self
            .ui
            .theme()
            .get(ThemeKeys::ACCENT_BACKGROUND)
            .expect("primary background in theme");
        let button_bg = *self
            .ui
            .theme()
            .get(ThemeKeys::CONTROL_BACKGROUND)
            .expect("button background in theme");
        let foreground = *self
            .ui
            .theme()
            .get(ThemeKeys::FOREGROUND)
            .expect("foreground in theme");
        let props: Vec<PropertyRow> = if let Some(sel) = self.selected_element {
            let scene = self.ui.scene();
            if let Some(resolved) = scene.resolved_element(sel).cloned() {
                let name = type_tag_name(resolved.type_tag);
                let mut p = vec![
                    PropertyRow {
                        name: "type".into(),
                        value: PropertyValue::Text(name.into()),
                    },
                    PropertyRow {
                        name: "id".into(),
                        value: PropertyValue::Text(format!("{sel:?}")),
                    },
                ];
                if let Some(label) = &resolved.label {
                    p.push(PropertyRow {
                        name: "label".into(),
                        value: PropertyValue::Text(label.to_string()),
                    });
                }
                let r = resolved.rect;
                p.push(PropertyRow {
                    name: "x".into(),
                    value: PropertyValue::Text(format!("{:.0}", r.x0)),
                });
                p.push(PropertyRow {
                    name: "y".into(),
                    value: PropertyValue::Text(format!("{:.0}", r.y0)),
                });
                p.push(PropertyRow {
                    name: "width".into(),
                    value: PropertyValue::Text(format!("{:.0}", r.width())),
                });
                p.push(PropertyRow {
                    name: "height".into(),
                    value: PropertyValue::Text(format!("{:.0}", r.height())),
                });
                p.push(PropertyRow {
                    name: "background".into(),
                    value: PropertyValue::Color(resolved.background),
                });
                p.push(PropertyRow {
                    name: "foreground".into(),
                    value: PropertyValue::Color(resolved.foreground),
                });
                p.push(PropertyRow {
                    name: "font_size".into(),
                    value: PropertyValue::Text(format!("{:.1}", resolved.font_size)),
                });
                p.push(PropertyRow {
                    name: "font_family".into(),
                    value: PropertyValue::Text(resolved.font_family.to_string()),
                });
                p.push(PropertyRow {
                    name: "text_align".into(),
                    value: PropertyValue::Text(format!("{:?}", resolved.text_align)),
                });
                p.push(PropertyRow {
                    name: "corner_radius".into(),
                    value: PropertyValue::Text(format!("{:.1}", resolved.corner_radius)),
                });
                if resolved.border.width > 0.0 {
                    p.push(PropertyRow {
                        name: "border_width".into(),
                        value: PropertyValue::Text(format!("{:.1}", resolved.border.width)),
                    });
                    p.push(PropertyRow {
                        name: "border_color".into(),
                        value: PropertyValue::Color(resolved.border.color),
                    });
                }
                p.push(PropertyRow {
                    name: "clips".into(),
                    value: PropertyValue::Badges(vec![if resolved.clips_content {
                        active_badge("yes", primary_bg)
                    } else {
                        idle_badge("no", button_bg, foreground)
                    }]),
                });
                let mut states = Vec::new();
                if resolved.hovered {
                    states.push(active_badge("hovered", primary_bg));
                }
                if resolved.pressed {
                    states.push(active_badge("pressed", primary_bg));
                }
                if resolved.focused {
                    states.push(active_badge("focused", primary_bg));
                }
                if states.is_empty() {
                    states.push(idle_badge("idle", button_bg, foreground));
                }
                p.push(PropertyRow {
                    name: "state".into(),
                    value: PropertyValue::Badges(states),
                });
                if resolved.scroll_offset != 0.0 {
                    p.push(PropertyRow {
                        name: "scroll".into(),
                        value: PropertyValue::Text(format!("{:.1}", resolved.scroll_offset)),
                    });
                }
                p
            } else {
                vec![PropertyRow {
                    name: "(not visible)".into(),
                    value: PropertyValue::Text(String::new()),
                }]
            }
        } else {
            vec![PropertyRow {
                name: "(no selection)".into(),
                value: PropertyValue::Text(String::new()),
            }]
        };

        let border_color = *self
            .ui
            .theme()
            .get(ThemeKeys::BORDER_COLOR)
            .expect("border color in theme");

        // Ensure enough structured rows.
        while self.prop_row_elements.len() < props.len() {
            let row = self
                .ui
                .append_child(self.ids.inspector_props, overstory::TYPE_ROW);
            self.ui.set_local(row, self.ui.properties().padding, 1.0);
            self.ui.set_local(row, self.ui.properties().gap, 6.0);
            self.ui
                .set_local(row, self.ui.properties().background, Color::TRANSPARENT);

            let name = self.ui.append_child(row, overstory::TYPE_TEXT_BLOCK);
            self.ui.set_local(name, self.ui.properties().width, 84.0);
            self.ui
                .set_local(name, self.ui.properties().label_padding, 2.0);
            self.ui.set_local(name, self.ui.properties().padding, 1.0);
            self.ui
                .set_local(name, self.ui.properties().font_size, 11.0);
            self.ui
                .set_local(name, self.ui.properties().background, Color::TRANSPARENT);

            let detail = self.ui.append_child(row, overstory::TYPE_ROW);
            self.ui.set_local(detail, self.ui.properties().fill, true);
            self.ui.set_local(detail, self.ui.properties().padding, 0.0);
            self.ui.set_local(detail, self.ui.properties().gap, 6.0);
            self.ui
                .set_local(detail, self.ui.properties().background, Color::TRANSPARENT);

            let swatch = self.ui.append_child(detail, overstory::TYPE_PANEL);
            self.ui.set_local(swatch, self.ui.properties().width, 14.0);
            self.ui.set_local(swatch, self.ui.properties().height, 14.0);
            self.ui
                .set_local(swatch, self.ui.properties().corner_radius, 3.0);
            self.ui
                .set_local(swatch, self.ui.properties().border_width, 1.0);
            self.ui
                .set_local(swatch, self.ui.properties().border_color, border_color);

            let value = self.ui.append_child(detail, overstory::TYPE_TEXT_BLOCK);
            self.ui.set_local(value, self.ui.properties().fill, true);
            self.ui
                .set_local(value, self.ui.properties().label_padding, 2.0);
            self.ui.set_local(value, self.ui.properties().padding, 1.0);
            self.ui
                .set_local(value, self.ui.properties().font_size, 11.0);
            self.ui
                .set_local(value, self.ui.properties().background, Color::TRANSPARENT);

            self.prop_row_elements.push(PropertyRowElements {
                row,
                name,
                detail,
                swatch,
                value,
                chips: Vec::new(),
            });
        }

        for i in 0..self.prop_row_elements.len() {
            let row = &mut self.prop_row_elements[i];
            if let Some(prop) = props.get(i) {
                self.ui.set_label(row.name, prop.name.as_str());
                self.ui
                    .set_local(row.row, self.ui.properties().visible, true);
                match &prop.value {
                    PropertyValue::Text(value) => {
                        self.ui.set_label(row.value, value.as_str());
                        self.ui
                            .set_local(row.swatch, self.ui.properties().visible, false);
                        self.ui
                            .set_local(row.value, self.ui.properties().visible, true);
                        for chip in &row.chips {
                            self.ui
                                .set_local(*chip, self.ui.properties().visible, false);
                        }
                    }
                    PropertyValue::Color(color) => {
                        self.ui.set_label(row.value, format_color(*color));
                        self.ui
                            .set_local(row.swatch, self.ui.properties().visible, true);
                        self.ui
                            .set_local(row.swatch, self.ui.properties().background, *color);
                        self.ui
                            .set_local(row.value, self.ui.properties().visible, true);
                        for chip in &row.chips {
                            self.ui
                                .set_local(*chip, self.ui.properties().visible, false);
                        }
                    }
                    PropertyValue::Badges(chips) => {
                        self.ui
                            .set_local(row.swatch, self.ui.properties().visible, false);
                        self.ui
                            .set_local(row.value, self.ui.properties().visible, false);

                        while row.chips.len() < chips.len() {
                            let chip = self.ui.append_child(row.detail, overstory::TYPE_TEXT_BLOCK);
                            self.ui
                                .set_local(chip, self.ui.properties().label_padding, 3.0);
                            self.ui.set_local(chip, self.ui.properties().padding, 2.0);
                            self.ui
                                .set_local(chip, self.ui.properties().font_size, 10.0);
                            self.ui
                                .set_local(chip, self.ui.properties().corner_radius, 6.0);
                            row.chips.push(chip);
                        }

                        for (chip_id, chip) in row.chips.iter().zip(chips.iter()) {
                            let width = self
                                .text
                                .measure_text(&chip.label, 10.0, "sans-serif", None)
                                .width
                                + 14.0;
                            self.ui.set_label(*chip_id, chip.label.as_str());
                            self.ui
                                .set_local(*chip_id, self.ui.properties().width, width);
                            self.ui.set_local(
                                *chip_id,
                                self.ui.properties().background,
                                chip.background,
                            );
                            self.ui.set_local(
                                *chip_id,
                                self.ui.properties().foreground,
                                chip.foreground,
                            );
                            self.ui
                                .set_local(*chip_id, self.ui.properties().visible, true);
                        }
                        for chip_id in row.chips.iter().skip(chips.len()) {
                            self.ui
                                .set_local(*chip_id, self.ui.properties().visible, false);
                        }
                    }
                }
            } else {
                self.ui.set_label(row.name, "");
                self.ui.set_label(row.value, "");
                self.ui
                    .set_local(row.swatch, self.ui.properties().visible, false);
                self.ui
                    .set_local(row.value, self.ui.properties().visible, false);
                for chip in &row.chips {
                    self.ui
                        .set_local(*chip, self.ui.properties().visible, false);
                }
                self.ui
                    .set_local(row.row, self.ui.properties().visible, false);
            }
        }
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
        self.sync_window_cursor(window);
        window.request_redraw();
    }

    fn sync_window_cursor(&self, window: &Window) {
        window.set_cursor(
            self.ui
                .cursor_icon()
                .unwrap_or(overstory::CursorIcon::Default),
        );
    }

    fn apply_interactions(&mut self, interactions: &overstory::InteractionBatch) {
        for interaction in interactions.events() {
            if let Interaction::Clicked(target) = *interaction {
                match target {
                    id if id == self.ids.light => {
                        self.ui.set_theme(light_theme());
                        self.sync_density_selection();
                    }
                    id if id == self.ids.dark => {
                        self.ui.set_theme(dark_theme());
                        self.sync_density_selection();
                    }
                    id if id == self.ids.roomy => self.apply_density(true),
                    id if id == self.ids.compact => self.apply_density(false),
                    id if id == self.ids.inspector_toggle => self.toggle_inspector_dock(),
                    id if id == self.ids.search => {
                        self.ui.set_local(
                            self.ids.content,
                            self.ui.properties().background,
                            Color::from_rgba8(248, 249, 252, 255),
                        );
                    }
                    id if id == self.ids.settings => {
                        self.ui.set_local(
                            self.ids.content,
                            self.ui.properties().background,
                            Color::from_rgba8(245, 243, 239, 255),
                        );
                    }
                    id if id == self.ids.deploy => {
                        self.ui.set_local(
                            self.ids.content,
                            self.ui.properties().background,
                            Color::from_rgba8(235, 245, 241, 255),
                        );
                    }
                    _ => {
                        // Check if it's an inspector tree row click.
                        if let Some(row_index) =
                            self.tree_row_elements.iter().position(|el| *el == target)
                        {
                            let rows = self.inspector.visible_rows().to_vec();
                            if let Some(row) = rows.get(row_index) {
                                let key = row.key;
                                if row.has_children {
                                    let _ = self.inspector.toggle(key);
                                }
                                self.selected_element = Some(key);
                                self.sync_inspector();
                                self.sync_property_grid();
                            }
                        }
                    }
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
                    self.api_messages
                        .push(llm_api::Message::text("user", &text));
                    self.sync_messages();
                    if was_at_tail {
                        self.scroll_to_tail();
                    }
                    self.ui.clear_text_buffer(self.ids.input);
                    // Send to Claude API.
                    self.send_to_llm();
                }
            }
        }
    }

    fn apply_density(&mut self, roomy: bool) {
        self.roomy = roomy;
        let sidebar_width = if roomy { 188.0 } else { 152.0 };
        let root_padding = if roomy { 24.0 } else { 14.0 };
        let button_padding = if roomy { 14.0 } else { 10.0 };
        let button_height = if roomy { 48.0 } else { 36.0 };
        let splitter_width = if roomy { 16.0 } else { 12.0 };

        self.ui
            .set_local(self.ui.root(), self.ui.properties().padding, root_padding);
        self.ui
            .set_local(self.ids.shell, self.ui.properties().gap, 0.0);
        self.ui
            .set_local(self.ids.sidebar, self.ui.properties().width, sidebar_width);
        self.ui.set_local(
            self.ids.splitter,
            self.ui.properties().width,
            splitter_width,
        );
        self.ui.set_local(
            self.ids.inspector_splitter,
            self.ui.properties().width,
            splitter_width,
        );
        let panel_padding = if roomy { 18.0 } else { 12.0 };
        let panel_gap = if roomy { 12.0 } else { 8.0 };
        for id in [self.ids.sidebar, self.ids.content] {
            self.ui
                .set_local(id, self.ui.properties().padding, panel_padding);
            self.ui.set_local(id, self.ui.properties().gap, panel_gap);
        }

        for id in [
            self.ids.light,
            self.ids.dark,
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
        self.ui.set_local(
            self.ids.inspector_toggle,
            self.ui.properties().padding,
            button_padding,
        );
        self.ui.set_local(
            self.ids.inspector_toggle,
            self.ui.properties().height,
            button_height,
        );

        self.inspector_dock
            .set_style(inspector_dock_style(self.roomy));
        self.inspector_dock
            .set_expanded_extent(current_inspector_width(self.roomy));
        self.sync_shell_frame(root_padding);
        self.sync_inspector_dock();
        self.sync_density_selection();
    }

    fn resize_ui(&mut self, size: PhysicalSize<u32>, scale_factor: f64) {
        let width = f64::from(size.width.max(1)) / scale_factor;
        let height = f64::from(size.height.max(1)) / scale_factor;
        self.ui.set_view_rect(Rect::new(0.0, 0.0, width, height));
        self.sync_shell_frame(current_root_padding(self.roomy));
    }

    fn sync_shell_frame(&mut self, root_padding: f64) {
        let shell_width = (self.ui.view_rect().width() - root_padding * 2.0).max(0.0);
        let shell_height = (self.ui.view_rect().height() - root_padding * 2.0).max(0.0);
        self.ui
            .set_local(self.ids.shell, self.ui.properties().width, shell_width);
        self.ui
            .set_local(self.ids.shell, self.ui.properties().height, shell_height);
        self.ui
            .set_local(self.ids.sidebar, self.ui.properties().height, shell_height);
        self.ui
            .set_local(self.ids.splitter, self.ui.properties().height, shell_height);
        self.ui.set_local(
            self.ids.inspector_splitter,
            self.ui.properties().height,
            shell_height,
        );
        self.ui.set_local(
            self.ids.inspector_panel,
            self.ui.properties().height,
            shell_height,
        );
        self.ui
            .set_local(self.ids.content, self.ui.properties().height, shell_height);
    }

    fn sync_inspector_dock(&mut self) {
        let splitter_width = if self.roomy { 16.0 } else { 12.0 };
        let panel_width = self.inspector_dock.current_extent();
        let collapsed = self.inspector_dock.is_collapsed();
        if let Some(splitter) = self
            .ui
            .widget_mut::<overstory::widgets::Splitter>(self.ids.splitter)
        {
            splitter.set_min_secondary(
                340.0 + panel_width + if collapsed { 0.0 } else { splitter_width },
            );
        }
        self.inspector_dock.sync(&mut self.ui);
    }

    fn toggle_inspector_dock(&mut self) {
        if !self.inspector_dock.is_collapsed()
            && let Some(width) = self
                .ui
                .scene()
                .resolved_element(self.ids.inspector_panel)
                .map(|resolved| resolved.rect.width())
        {
            self.inspector_dock
                .set_expanded_extent(width.max(current_inspector_width(self.roomy)));
        }
        self.inspector_dock.toggle();
        self.sync_inspector_dock();
    }

    fn sync_density_selection(&mut self) {
        let button_bg = *self
            .ui
            .theme()
            .get(ThemeKeys::CONTROL_BACKGROUND)
            .expect("button background in theme");
        let primary_bg = *self
            .ui
            .theme()
            .get(ThemeKeys::ACCENT_BACKGROUND)
            .expect("primary background in theme");
        let accent_fg = *self
            .ui
            .theme()
            .get(ThemeKeys::ACCENT_FOREGROUND)
            .expect("accent foreground in theme");
        let foreground = *self
            .ui
            .theme()
            .get(ThemeKeys::FOREGROUND)
            .expect("foreground in theme");

        let (roomy_bg, roomy_fg, compact_bg, compact_fg) = if self.roomy {
            (primary_bg, accent_fg, button_bg, foreground)
        } else {
            (button_bg, foreground, primary_bg, accent_fg)
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
        self.ui.refresh_editors();
        self.ui.update_tooltips();
        let plan = self.ui.surface_plan();
        let (mut display_tree, view_rect) = plan
            .flatten_to_display_tree()
            .expect("surface plan should have a root display surface");
        display_tree.layout(
            &mut self.text,
            view_rect.origin(),
            BoxConstraints::tight(view_rect.size()),
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
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
        self.sync_window_cursor(&window);
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

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        // Update monotonic time for timer-dependent operations.
        self.ui.set_now(self.now_nanos());

        // Fire expired timers on timer wakeup.
        if matches!(cause, winit::event::StartCause::ResumeTimeReached { .. })
            && self.ui.tick(self.now_nanos())
            && let RenderState::Active { window, .. } = &self.render_state
        {
            window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Poll for Claude API streaming events.
        if self.api_receiver.is_some() {
            self.poll_api();
            if let RenderState::Active { window, .. } = &self.render_state {
                window.request_redraw();
            }
            // While streaming, use a short poll interval.
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                std::time::Instant::now() + std::time::Duration::from_millis(16),
            ));
            return;
        }

        if let Some(deadline_nanos) = self.ui.next_deadline() {
            let deadline = self.epoch + std::time::Duration::from_nanos(deadline_nanos);
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
        }
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
                    Color::from_rgba8(255, 252, 246, 255),
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

    let shell = ui.append_child(ui.root(), overstory::TYPE_ROW);
    ui.set_local(shell, ui.properties().padding, 0.0);
    ui.set_local(shell, ui.properties().gap, 0.0);

    let sidebar = ui.append_child(shell, overstory::TYPE_PANEL);
    ui.add_layout_class(sidebar, LayoutClass::Sidebar);
    ui.set_local(sidebar, ui.properties().width, 176.0);
    ui.set_local(sidebar, ui.properties().padding, 16.0);
    ui.set_local(sidebar, ui.properties().gap, 10.0);

    let sidebar_column = ui.append_child(sidebar, overstory::TYPE_COLUMN);
    ui.set_local(sidebar_column, ui.properties().padding, 0.0);
    ui.set_local(sidebar_column, ui.properties().gap, 10.0);

    let light = append_button(
        &mut ui,
        sidebar_column,
        &button_cascade,
        "Light theme",
        false,
    );
    let dark = append_button(
        &mut ui,
        sidebar_column,
        &button_cascade,
        "Dark theme",
        false,
    );
    let roomy = append_button(&mut ui, sidebar_column, &button_cascade, "Roomy", false);
    let compact = append_button(&mut ui, sidebar_column, &button_cascade, "Compact", false);

    let splitter = ui.append_child_with(
        shell,
        overstory::TYPE_SPLITTER,
        Some(Box::new(
            overstory::widgets::Splitter::vertical(sidebar)
                .with_min_primary(152.0)
                .with_min_secondary(340.0),
        )),
    );
    ui.set_local(splitter, ui.properties().width, 16.0);

    let content = ui.append_child(shell, overstory::TYPE_PANEL);
    ui.set_local(content, ui.properties().fill, true);
    ui.set_local(content, ui.properties().padding, 18.0);
    ui.set_local(content, ui.properties().gap, 12.0);

    let inspector_splitter = ui.append_child_with(
        shell,
        overstory::TYPE_SPLITTER,
        Some(Box::new(
            overstory::widgets::Splitter::default()
                .with_min_primary(260.0)
                .with_min_secondary(420.0),
        )),
    );
    ui.set_local(inspector_splitter, ui.properties().width, 16.0);

    let inspector_panel = ui.append_child(shell, overstory::TYPE_PANEL);
    ui.set_local(
        inspector_panel,
        ui.properties().width,
        current_inspector_width(true),
    );
    ui.set_local(inspector_panel, ui.properties().padding, 18.0);
    ui.set_local(inspector_panel, ui.properties().gap, 12.0);
    if let Some(splitter) = ui.widget_mut::<overstory::widgets::Splitter>(inspector_splitter) {
        splitter.set_side(overstory::widgets::SplitterSide::Trailing);
        splitter.set_target(inspector_panel);
    }

    let content_column = ui.append_child(content, overstory::TYPE_COLUMN);
    ui.set_local(content_column, ui.properties().padding, 0.0);
    ui.set_local(content_column, ui.properties().gap, 12.0);
    ui.set_local(content_column, ui.properties().fill, true);

    let inspector_column = ui.append_child(inspector_panel, overstory::TYPE_COLUMN);
    ui.set_local(inspector_column, ui.properties().padding, 0.0);
    ui.set_local(inspector_column, ui.properties().gap, 10.0);
    ui.set_local(inspector_column, ui.properties().fill, true);

    let inspector_header = ui.append_child(inspector_column, overstory::TYPE_ROW);
    ui.set_local(inspector_header, ui.properties().padding, 0.0);
    ui.set_local(inspector_header, ui.properties().gap, 8.0);

    let inspector_toggle = append_button(
        &mut ui,
        inspector_header,
        &button_cascade,
        "Inspector ⟩",
        false,
    );
    ui.set_local(inspector_toggle, ui.properties().width, 112.0);

    let inspector_tree_label = ui.append_child(inspector_column, overstory::TYPE_TEXT_BLOCK);
    ui.set_label(inspector_tree_label, "Element Tree");
    ui.set_local(inspector_tree_label, ui.properties().font_size, 11.0);
    ui.set_local(inspector_tree_label, ui.properties().label_padding, 2.0);

    let inspector_tree = ui.append_child(inspector_column, overstory::TYPE_SCROLL_VIEW);
    ui.set_local(inspector_tree, ui.properties().fill, true);
    ui.set_local(inspector_tree, ui.properties().padding, 4.0);
    ui.set_local(inspector_tree, ui.properties().gap, 0.0);
    ui.set_local(
        inspector_tree,
        ui.properties().background,
        Color::TRANSPARENT,
    );

    let inspector_props_label = ui.append_child(inspector_column, overstory::TYPE_TEXT_BLOCK);
    ui.set_label(inspector_props_label, "Properties");
    ui.set_local(inspector_props_label, ui.properties().font_size, 11.0);
    ui.set_local(inspector_props_label, ui.properties().label_padding, 2.0);

    let inspector_props = ui.append_child(inspector_column, overstory::TYPE_SCROLL_VIEW);
    ui.set_local(inspector_props, ui.properties().height, 220.0);
    ui.set_local(inspector_props, ui.properties().padding, 4.0);
    ui.set_local(inspector_props, ui.properties().gap, 0.0);
    ui.set_local(
        inspector_props,
        ui.properties().background,
        Color::TRANSPARENT,
    );

    // Action button row at the top of the content area.
    let button_row = ui.append_child(content_column, overstory::TYPE_ROW);
    ui.set_local(button_row, ui.properties().padding, 0.0);
    ui.set_local(button_row, ui.properties().gap, 8.0);

    let search = append_button(&mut ui, button_row, &button_cascade, "Search", false);
    ui.set_local(search, ui.properties().fill, true);
    let settings = append_button(&mut ui, button_row, &button_cascade, "Settings", false);
    ui.set_local(settings, ui.properties().fill, true);
    let deploy = append_button(&mut ui, button_row, &button_cascade, "Deploy", true);
    ui.set_local(deploy, ui.properties().fill, true);
    ui.set_local(deploy, ui.properties().foreground, palette::css::WHITE);

    // Tooltip on the Deploy button — shows on hover, positioned below trigger.
    let tooltip = ui.append_child_with(
        ui.root(),
        overstory::TYPE_TOOLTIP,
        Some(Box::new(overstory::widgets::Tooltip::new(deploy))),
    );
    ui.set_label(tooltip, "Ships to production");
    ui.set_local(tooltip, ui.properties().height, 28.0);
    ui.set_local(tooltip, ui.properties().width, 150.0);
    ui.set_local(tooltip, ui.properties().corner_radius, 4.0);
    ui.set_local(tooltip, ui.properties().border_width, 1.0);

    // Scrollable message area demonstrating ScrollView + TextBlock.
    let messages = ui.append_child(content_column, overstory::TYPE_SCROLL_VIEW);
    ui.set_local(messages, ui.properties().fill, true);
    ui.set_local(messages, ui.properties().padding, 12.0);
    ui.set_local(messages, ui.properties().gap, 10.0);
    ui.set_local(messages, ui.properties().background, Color::TRANSPARENT);

    // Messages are populated from the transcript via DemoApp::sync_messages.

    // Text input at the bottom.
    let input = ui.append_child(content_column, overstory::TYPE_TEXT_INPUT);
    ui.set_local(input, ui.properties().padding, 8.0);
    ui.set_local(input, ui.properties().border_width, 1.0);
    ui.set_local(input, ui.properties().corner_radius, 6.0);
    if let Some(w) = ui.widget_mut::<overstory::widgets::TextInput>(input) {
        w.set_placeholder("Type a message... (Cmd+Enter to send)");
    }

    (
        ui,
        DemoIds {
            shell,
            light,
            dark,
            roomy,
            compact,
            sidebar,
            splitter,
            content,
            inspector_splitter,
            inspector_panel,
            inspector_toggle,
            inspector_tree_label,
            inspector_props_label,
            search,
            settings,
            deploy,
            messages,
            input,
            inspector_tree,
            inspector_props,
        },
    )
}

fn current_root_padding(roomy: bool) -> f64 {
    if roomy { 24.0 } else { 14.0 }
}

fn current_inspector_width(roomy: bool) -> f64 {
    if roomy { 320.0 } else { 280.0 }
}

fn inspector_rail_width() -> f64 {
    44.0
}

fn inspector_dock_style(roomy: bool) -> DockPaneStyle {
    DockPaneStyle {
        expanded_label: "Inspector ⟩".into(),
        collapsed_label: "⟨".into(),
        expanded_toggle_width: 112.0,
        collapsed_toggle_width: inspector_rail_width() - 12.0,
        expanded_padding: if roomy { 18.0 } else { 12.0 },
        expanded_gap: if roomy { 12.0 } else { 8.0 },
        collapsed_padding: 6.0,
        collapsed_gap: 0.0,
    }
}

fn format_color(color: Color) -> String {
    let rgba = color.to_rgba8();
    format!("#{:02x}{:02x}{:02x}{:02x}", rgba.r, rgba.g, rgba.b, rgba.a)
}

fn active_badge(label: impl Into<String>, background: Color) -> PropertyBadge {
    PropertyBadge {
        label: label.into(),
        background,
        foreground: palette::css::WHITE,
    }
}

fn idle_badge(label: impl Into<String>, background: Color, foreground: Color) -> PropertyBadge {
    PropertyBadge {
        label: label.into(),
        background,
        foreground,
    }
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

        let splitter_rect = scene
            .resolved_element(app.ids.splitter)
            .expect("resolved element")
            .rect;
        assert_eq!(splitter_rect.width(), 16.0);
        assert_eq!(splitter_rect.height(), expected_height);

        let content_rect = scene
            .resolved_element(app.ids.content)
            .expect("resolved element")
            .rect;
        let inspector_splitter_rect = scene
            .resolved_element(app.ids.inspector_splitter)
            .expect("resolved element")
            .rect;
        let inspector_panel_rect = scene
            .resolved_element(app.ids.inspector_panel)
            .expect("resolved element")
            .rect;
        assert_eq!(
            content_rect.width(),
            expected_width
                - sidebar_rect.width()
                - splitter_rect.width()
                - inspector_splitter_rect.width()
                - inspector_panel_rect.width()
        );
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
        let compact_splitter = compact_scene
            .resolved_element(app.ids.splitter)
            .expect("resolved element")
            .rect;
        let compact_content = compact_scene
            .resolved_element(app.ids.content)
            .expect("resolved element")
            .rect;
        let compact_inspector_splitter = compact_scene
            .resolved_element(app.ids.inspector_splitter)
            .expect("resolved element")
            .rect;
        let compact_inspector_panel = compact_scene
            .resolved_element(app.ids.inspector_panel)
            .expect("resolved element")
            .rect;
        assert_eq!(
            compact_shell.width(),
            960.0 - current_root_padding(false) * 2.0
        );
        assert_eq!(compact_shell.height(), compact_height);
        assert_eq!(compact_sidebar.width(), 152.0);
        assert_eq!(compact_sidebar.height(), compact_height);
        assert_eq!(compact_splitter.width(), 12.0);
        assert_eq!(compact_splitter.height(), compact_height);
        assert_eq!(
            compact_content.width(),
            compact_shell.width()
                - compact_sidebar.width()
                - compact_splitter.width()
                - compact_inspector_splitter.width()
                - compact_inspector_panel.width()
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
        let roomy_splitter = roomy_scene
            .resolved_element(app.ids.splitter)
            .expect("resolved element")
            .rect;
        let roomy_content = roomy_scene
            .resolved_element(app.ids.content)
            .expect("resolved element")
            .rect;
        let roomy_inspector_splitter = roomy_scene
            .resolved_element(app.ids.inspector_splitter)
            .expect("resolved element")
            .rect;
        let roomy_inspector_panel = roomy_scene
            .resolved_element(app.ids.inspector_panel)
            .expect("resolved element")
            .rect;
        assert_eq!(roomy_shell.height(), roomy_height);
        assert_eq!(roomy_sidebar.width(), 188.0);
        assert_eq!(roomy_sidebar.height(), roomy_height);
        assert_eq!(roomy_splitter.width(), 16.0);
        assert_eq!(roomy_splitter.height(), roomy_height);
        assert_eq!(
            roomy_content.width(),
            roomy_shell.width()
                - roomy_sidebar.width()
                - roomy_splitter.width()
                - roomy_inspector_splitter.width()
                - roomy_inspector_panel.width()
        );
        assert_eq!(roomy_content.height(), roomy_height);
    }

    #[test]
    fn inspector_toggle_collapses_to_rail() {
        let mut app = DemoApp::new();
        app.resize_ui(PhysicalSize::new(960, 640), 1.0);

        app.toggle_inspector_dock();

        let scene = app.ui.scene();
        let inspector_panel = scene
            .resolved_element(app.ids.inspector_panel)
            .expect("inspector panel")
            .rect;

        assert_eq!(inspector_panel.width(), inspector_rail_width());
        assert!(scene.resolved_element(app.ids.inspector_splitter).is_none());
    }

    #[test]
    fn dragging_splitter_resizes_sidebar_and_preserves_content() {
        use overstory::ui_events::pointer::{
            PointerButton, PointerButtonEvent, PointerButtons, PointerId, PointerInfo,
            PointerState, PointerType, PointerUpdate,
        };

        fn pointer_info() -> PointerInfo {
            PointerInfo {
                pointer_id: Some(PointerId::PRIMARY),
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            }
        }

        #[allow(
            clippy::field_reassign_with_default,
            reason = "Constructing pointer state is clearer via Default + explicit fields."
        )]
        fn pointer_state(x: f64, y: f64, time: u64) -> PointerState {
            let mut state = PointerState::default();
            state.time = time;
            state.position.x = x;
            state.position.y = y;
            state.buttons = PointerButtons::new();
            state.count = 1;
            state.scale_factor = 1.0;
            state
        }

        let mut app = DemoApp::new();
        app.resize_ui(PhysicalSize::new(960, 640), 1.0);

        let before = app.ui.scene();
        let splitter = before
            .resolved_element(app.ids.splitter)
            .expect("splitter rect")
            .rect;
        let start_x = splitter.center().x;
        let start_y = splitter.center().y;

        let _ = app
            .ui
            .handle_pointer_event(&overstory::ui_events::pointer::PointerEvent::Move(
                PointerUpdate {
                    pointer: pointer_info(),
                    current: pointer_state(start_x, start_y, 1),
                    coalesced: Vec::new(),
                    predicted: Vec::new(),
                },
            ));
        let _ = app
            .ui
            .handle_pointer_event(&overstory::ui_events::pointer::PointerEvent::Down(
                PointerButtonEvent {
                    button: Some(PointerButton::Primary),
                    pointer: pointer_info(),
                    state: pointer_state(start_x, start_y, 2),
                },
            ));
        let _ = app
            .ui
            .handle_pointer_event(&overstory::ui_events::pointer::PointerEvent::Move(
                PointerUpdate {
                    pointer: pointer_info(),
                    current: pointer_state(start_x + 48.0, start_y, 3),
                    coalesced: Vec::new(),
                    predicted: Vec::new(),
                },
            ));
        let _ = app
            .ui
            .handle_pointer_event(&overstory::ui_events::pointer::PointerEvent::Up(
                PointerButtonEvent {
                    button: Some(PointerButton::Primary),
                    pointer: pointer_info(),
                    state: pointer_state(start_x + 48.0, start_y, 4),
                },
            ));

        let after = app.ui.scene();
        let sidebar = after
            .resolved_element(app.ids.sidebar)
            .expect("sidebar rect")
            .rect;
        let splitter = after
            .resolved_element(app.ids.splitter)
            .expect("splitter rect")
            .rect;
        let content = after
            .resolved_element(app.ids.content)
            .expect("content rect")
            .rect;

        assert!(sidebar.width() > 188.0);
        assert_eq!(splitter.x0, sidebar.x1);
        assert_eq!(content.x0, splitter.x1);
        assert!(content.width() >= 340.0);
    }

    #[test]
    fn fill_child_takes_remaining_space() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 400.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append_child(ui.root(), overstory::TYPE_COLUMN);
        ui.set_local(column, ui.properties().padding, 0.0);
        ui.set_local(column, ui.properties().gap, 0.0);
        ui.set_local(column, ui.properties().height, 400.0);

        let top = ui.append_child(column, overstory::TYPE_BUTTON);
        ui.set_local(top, ui.properties().height, 50.0);

        let middle = ui.append_child(column, overstory::TYPE_PANEL);
        ui.set_local(middle, ui.properties().fill, true);

        let bottom = ui.append_child(column, overstory::TYPE_BUTTON);
        ui.set_local(bottom, ui.properties().height, 50.0);

        let scene = ui.scene();
        assert_eq!(scene.resolved_element(top).unwrap().rect.height(), 50.0);
        assert_eq!(scene.resolved_element(middle).unwrap().rect.height(), 300.0);
        assert_eq!(scene.resolved_element(bottom).unwrap().rect.height(), 50.0);
        assert_eq!(scene.resolved_element(bottom).unwrap().rect.y0, 350.0);
    }

    #[test]
    fn multiple_fill_children_share_space() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 300.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append_child(ui.root(), overstory::TYPE_COLUMN);
        ui.set_local(column, ui.properties().padding, 0.0);
        ui.set_local(column, ui.properties().gap, 0.0);
        ui.set_local(column, ui.properties().height, 300.0);

        let first = ui.append_child(column, overstory::TYPE_PANEL);
        ui.set_local(first, ui.properties().fill, true);

        let second = ui.append_child(column, overstory::TYPE_PANEL);
        ui.set_local(second, ui.properties().fill, true);

        let scene = ui.scene();
        assert_eq!(scene.resolved_element(first).unwrap().rect.height(), 150.0);
        assert_eq!(scene.resolved_element(second).unwrap().rect.height(), 150.0);
        assert_eq!(scene.resolved_element(second).unwrap().rect.y0, 150.0);
    }

    #[test]
    fn scroll_view_offsets_children() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 200.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let scroll = ui.append_child(ui.root(), overstory::TYPE_SCROLL_VIEW);
        ui.set_local(scroll, ui.properties().padding, 0.0);
        ui.set_local(scroll, ui.properties().gap, 0.0);
        ui.set_local(scroll, ui.properties().height, 200.0);

        let a = ui.append_child(scroll, overstory::TYPE_BUTTON);
        ui.set_local(a, ui.properties().height, 100.0);
        let b = ui.append_child(scroll, overstory::TYPE_BUTTON);
        ui.set_local(b, ui.properties().height, 100.0);
        let c = ui.append_child(scroll, overstory::TYPE_BUTTON);
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

        let scroll = ui.append_child(ui.root(), overstory::TYPE_SCROLL_VIEW);
        ui.set_local(scroll, ui.properties().padding, 0.0);
        ui.set_local(scroll, ui.properties().gap, 0.0);
        ui.set_local(scroll, ui.properties().height, 200.0);

        let a = ui.append_child(scroll, overstory::TYPE_BUTTON);
        ui.set_local(a, ui.properties().height, 100.0);
        let b = ui.append_child(scroll, overstory::TYPE_BUTTON);
        ui.set_local(b, ui.properties().height, 100.0);
        let c = ui.append_child(scroll, overstory::TYPE_BUTTON);
        ui.set_local(c, ui.properties().height, 100.0);

        let _ = ui.scene();
        assert_eq!(ui.content_height(scroll), 300.0);
    }

    #[test]
    fn scroll_offset_clamps_to_zero() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 200.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);

        let scroll = ui.append_child(ui.root(), overstory::TYPE_SCROLL_VIEW);
        ui.set_local(scroll, ui.properties().height, 200.0);

        ui.set_scroll_offset(scroll, -50.0);
        assert_eq!(ui.scroll_offset(scroll), 0.0);
    }

    #[test]
    fn custom_font_size_in_resolved_element() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 200.0, 100.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);

        let button = ui.append_child(ui.root(), overstory::TYPE_BUTTON);
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

        let button = ui.append_child(ui.root(), overstory::TYPE_BUTTON);
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

        let column = ui.append_child(ui.root(), overstory::TYPE_COLUMN);
        ui.set_local(column, ui.properties().padding, 0.0);
        ui.set_local(column, ui.properties().gap, 8.0);

        let short = ui.append_child(column, overstory::TYPE_TEXT_BLOCK);
        ui.set_label(short, "Hello");

        let long = ui.append_child(column, overstory::TYPE_TEXT_BLOCK);
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

        let column = ui.append_child(ui.root(), overstory::TYPE_COLUMN);
        ui.set_local(column, ui.properties().padding, 0.0);
        ui.set_local(column, ui.properties().gap, 0.0);

        let a = ui.append_child(column, overstory::TYPE_TEXT_BLOCK);
        ui.set_label(a, "First message");

        let b = ui.append_child(column, overstory::TYPE_TEXT_BLOCK);
        ui.set_label(b, "Second message");

        let scene = ui.scene();
        let a_rect = scene.resolved_element(a).unwrap().rect;
        let b_rect = scene.resolved_element(b).unwrap().rect;

        assert_eq!(a_rect.y0, 0.0);
        assert_eq!(
            b_rect.y0, a_rect.y1,
            "second block should start where first ends"
        );
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
        use overstory::ui_events::ScrollDelta;
        use overstory::ui_events::pointer::{
            PointerButtons, PointerId, PointerInfo, PointerScrollEvent, PointerState, PointerType,
        };

        let mut app = DemoApp::new();
        app.resize_ui(PhysicalSize::new(960, 640), 1.0);

        // Add enough messages to overflow the scroll view.
        for i in 0..20 {
            app.transcript.append(NewEntry::message(
                MessageRole::User,
                format!("Message {i} with enough text to take up space"),
            ));
        }
        app.sync_messages();

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

        let scroll_event =
            overstory::ui_events::pointer::PointerEvent::Scroll(PointerScrollEvent {
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

        let input = ui.append_child(ui.root(), overstory::TYPE_TEXT_INPUT);
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

        // Plain Enter inserts newline now; Cmd+Enter submits.
        let _ = ui.handle_keyboard_event(&key_event(Key::Named(NamedKey::Enter)));
        assert_eq!(ui.text_buffer(input), "H\n");

        let submit_event = KeyboardEvent {
            key: Key::Named(NamedKey::Enter),
            code: Code::Unidentified,
            state: KeyState::Down,
            modifiers: Modifiers::META,
            location: Location::Standard,
            repeat: false,
            is_composing: false,
        };
        let batch = ui.handle_keyboard_event(&submit_event);
        assert!(
            batch
                .events()
                .iter()
                .any(|e| matches!(e, Interaction::Submitted(_)))
        );
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
            .get(ThemeKeys::CONTROL_BACKGROUND)
            .expect("button background in theme");
        let theme_primary = *app
            .ui
            .theme()
            .get(ThemeKeys::ACCENT_BACKGROUND)
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

    #[test]
    fn empty_streaming_entry_hides_when_completed() {
        let mut app = DemoApp::new();

        let entry = app.transcript.append(
            NewEntry::message(MessageRole::Assistant, "").with_status(EntryStatus::InProgress),
        );
        app.sync_messages();

        let block = app
            .element_for_entry(entry)
            .expect("streaming entry should have a UI element");
        let scene = app.ui.scene();
        let resolved = scene
            .resolved_element(block)
            .expect("typing indicator should be visible while in progress");
        assert_eq!(resolved.label.as_deref(), Some("..."));

        let _ = app.transcript.set_status(entry, EntryStatus::Complete);
        app.refresh_entry_element(entry);

        let scene = app.ui.scene();
        assert!(
            scene.resolved_element(block).is_none(),
            "completed empty typing indicator should disappear"
        );
    }

    #[test]
    fn property_grid_shows_color_rows_with_swatches() {
        let mut app = DemoApp::new();
        app.selected_element = Some(app.ids.deploy);
        app.sync_property_grid();

        let background_row = app
            .prop_row_elements
            .iter()
            .find(|row| app.ui.element(row.name).and_then(|e| e.label()) == Some("background"))
            .cloned()
            .expect("background row should exist");

        let scene = app.ui.scene();
        let deploy = scene
            .resolved_element(app.ids.deploy)
            .expect("deploy button should be visible");
        let swatch = scene
            .resolved_element(background_row.swatch)
            .expect("background swatch should be visible");
        let expected_label = format_color(deploy.background);
        assert_eq!(swatch.background, deploy.background);
        assert_eq!(
            app.ui.element(background_row.value).and_then(|e| e.label()),
            Some(expected_label.as_str())
        );
    }

    #[test]
    fn property_grid_shows_state_and_clip_badges() {
        let mut app = DemoApp::new();
        app.selected_element = Some(app.ids.input);
        app.sync_property_grid();

        let state_row = app
            .prop_row_elements
            .iter()
            .find(|row| app.ui.element(row.name).and_then(|e| e.label()) == Some("state"))
            .cloned()
            .expect("state row should exist");
        let state_chip_info: Vec<_> = state_row
            .chips
            .iter()
            .filter_map(|chip| {
                app.ui
                    .element(*chip)
                    .and_then(|e| e.label())
                    .map(|label| (*chip, label.to_string()))
            })
            .collect();
        let scene = app.ui.scene();
        let visible_state_labels: Vec<_> = state_row
            .chips
            .iter()
            .filter(|chip| scene.resolved_element(**chip).is_some())
            .filter_map(|chip| {
                state_chip_info
                    .iter()
                    .find(|(id, _)| id == chip)
                    .map(|(_, label)| label.as_str())
            })
            .collect();
        assert!(
            visible_state_labels.contains(&"focused"),
            "focused input should show a focused state badge, got {:?}",
            visible_state_labels
        );

        let clips_row = app
            .prop_row_elements
            .iter()
            .find(|row| app.ui.element(row.name).and_then(|e| e.label()) == Some("clips"))
            .cloned()
            .expect("clips row should exist");
        let clip_chip_info: Vec<_> = clips_row
            .chips
            .iter()
            .filter_map(|chip| {
                app.ui
                    .element(*chip)
                    .and_then(|e| e.label())
                    .map(|label| (*chip, label.to_string()))
            })
            .collect();
        let scene = app.ui.scene();
        let visible_clip_labels: Vec<_> = clips_row
            .chips
            .iter()
            .filter(|chip| scene.resolved_element(**chip).is_some())
            .filter_map(|chip| {
                clip_chip_info
                    .iter()
                    .find(|(id, _)| id == chip)
                    .map(|(_, label)| label.as_str())
            })
            .collect();
        assert_eq!(visible_clip_labels, vec!["no"]);
    }

    #[test]
    fn api_request_messages_ignore_ui_only_transcript_rows() {
        let mut app = DemoApp::new();

        assert_eq!(
            app.current_api_request_messages().len(),
            1,
            "initial transcript welcome should not be sent back to the model"
        );

        app.transcript.append(NewEntry::message(
            MessageRole::Assistant,
            "[tool: set_theme] Switched to dark theme",
        ));
        app.sync_messages();

        assert_eq!(
            app.current_api_request_messages().len(),
            1,
            "UI-only transcript rows should not affect protocol message history"
        );
    }

    #[test]
    fn finishing_plain_stream_appends_assistant_to_api_messages() {
        let mut app = DemoApp::new();
        let entry = app.transcript.append(
            NewEntry::message(MessageRole::Assistant, "").with_status(EntryStatus::InProgress),
        );
        app.streaming_entry = Some(entry);
        app.sync_messages();
        app.streaming_api_text = "hello from assistant".into();

        app.finish_streaming_without_tools();

        assert!(app.streaming_entry.is_none());
        assert!(app.streaming_api_text.is_empty());
        assert_eq!(app.api_messages.len(), 1);
        assert_eq!(app.api_messages[0].role, "assistant");
        assert_eq!(
            app.api_messages[0].content.as_deref(),
            Some("hello from assistant")
        );
    }
}

fn append_button(
    ui: &mut Ui,
    parent: ElementId,
    cascade: &StyleCascade,
    label: &str,
    primary: bool,
) -> ElementId {
    let button = ui.append_child(parent, overstory::TYPE_BUTTON);
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

fn light_theme() -> Theme {
    default_theme()
}

fn dark_theme() -> Theme {
    ThemeBuilder::new()
        .set(
            ThemeKeys::APP_BACKGROUND,
            Color::from_rgba8(22, 22, 24, 255),
        )
        .set(
            ThemeKeys::SURFACE_BACKGROUND,
            Color::from_rgba8(32, 32, 35, 255),
        )
        .set(
            ThemeKeys::SURFACE_MUTED_BACKGROUND,
            Color::from_rgba8(26, 26, 29, 255),
        )
        .set(
            ThemeKeys::CONTROL_BACKGROUND,
            Color::from_rgba8(44, 44, 48, 255),
        )
        .set(
            ThemeKeys::CONTROL_BACKGROUND_EMPHASIZED,
            Color::from_rgba8(54, 54, 58, 255),
        )
        .set(
            ThemeKeys::CONTROL_BACKGROUND_STRONG,
            Color::from_rgba8(38, 38, 42, 255),
        )
        .set(
            ThemeKeys::ACCENT_BACKGROUND,
            Color::from_rgba8(36, 140, 106, 255),
        )
        .set(
            ThemeKeys::ACCENT_BACKGROUND_EMPHASIZED,
            Color::from_rgba8(42, 158, 120, 255),
        )
        .set(
            ThemeKeys::ACCENT_BACKGROUND_STRONG,
            Color::from_rgba8(28, 115, 86, 255),
        )
        .set(ThemeKeys::ACCENT_FOREGROUND, Color::WHITE)
        .set(ThemeKeys::FOREGROUND, Color::from_rgba8(210, 212, 216, 255))
        .set(ThemeKeys::BORDER_COLOR, Color::from_rgba8(58, 58, 62, 255))
        .set(ThemeKeys::CORNER_RADIUS, 10.0_f64)
        .set(ThemeKeys::PADDING, 16.0_f64)
        .set(ThemeKeys::GAP, 12.0_f64)
        .set(ThemeKeys::BUTTON_HEIGHT, 44.0_f64)
        .set(
            ThemeKeys::DIVIDER_BACKGROUND_EMPHASIZED,
            Color::from_rgba8(210, 212, 216, 24),
        )
        .set(
            ThemeKeys::DIVIDER_BACKGROUND_STRONG,
            Color::from_rgba8(210, 212, 216, 42),
        )
        .build()
}
