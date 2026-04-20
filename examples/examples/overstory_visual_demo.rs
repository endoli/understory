// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Windowed Overstory demo rendered through `imaging`.
//!
//! This example keeps `overstory` renderer-agnostic:
//! - Overstory owns retained UI state, style resolution, layout, box-tree hit
//!   testing, and pointer interaction.
//! - This example lowers the resolved [`overstory::SceneSnapshot`] into
//!   a retained `understory_display::DisplayTree`.
//! - It then lowers that tree directly into `imaging::record::Scene`.
//! - `imaging_vello_cpu` rasterizes that scene into an RGBA buffer.
//! - `softbuffer` presents the result in a `winit` window.
//!
//! Run:
//! - `cargo run -p understory_examples --example overstory_visual_demo`

use std::boxed::Box;
use std::num::NonZeroU32;
use std::rc::Rc;

use imaging_vello_cpu::VelloCpuRenderer;
use kurbo::Rect;
use overstory::peniko::color::palette;
use overstory::{
    ButtonClass, ElementId, ElementKind, Interaction, LayoutClass, ThemeKeys, Ui, default_theme,
};
use softbuffer::Surface;
use ui_events_winit::{WindowEventReducer, WindowEventTranslation};
use understory_display::{BoxConstraints, TextEngine};
use understory_examples::overstory_display::imaging_scene_from_display_tree;
use understory_style::{
    IdSet, Selector, StyleBuilder, StyleCascade, StyleCascadeBuilder, StyleOrigin,
    StyleSheetBuilder, Theme, ThemeBuilder,
};
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
}

#[derive(Debug)]
enum RenderState {
    Active {
        window: Rc<Window>,
        surface: Surface<Rc<Window>, Rc<Window>>,
        renderer: Box<VelloCpuRenderer>,
    },
    Suspended,
}

struct DemoApp {
    ui: Ui,
    ids: DemoIds,
    roomy: bool,
    reducer: WindowEventReducer,
    text: TextEngine,
    render_state: RenderState,
}

impl DemoApp {
    fn new() -> Self {
        let (ui, ids) = build_demo_ui();
        let mut app = Self {
            ui,
            ids,
            roomy: true,
            reducer: WindowEventReducer::default(),
            text: TextEngine::new(),
            render_state: RenderState::Suspended,
        };
        app.apply_density(true);
        app
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

    fn resize_ui(&mut self, size: PhysicalSize<u32>) {
        let width = size.width.max(1);
        let height = size.height.max(1);
        self.ui
            .set_view_rect(Rect::new(0.0, 0.0, f64::from(width), f64::from(height)));
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

    fn resize_active_surface(&mut self, size: PhysicalSize<u32>) {
        self.resize_ui(size);

        let RenderState::Active {
            window,
            surface,
            renderer,
        } = &mut self.render_state
        else {
            return;
        };

        if size.width == 0 || size.height == 0 {
            return;
        }
        surface
            .resize(
                NonZeroU32::new(size.width).expect("non-zero width"),
                NonZeroU32::new(size.height).expect("non-zero height"),
            )
            .expect("resize surface");
        **renderer = VelloCpuRenderer::new(
            u16::try_from(size.width).expect("window width exceeds u16"),
            u16::try_from(size.height).expect("window height exceeds u16"),
        );
        window.request_redraw();
    }

    fn redraw_active(&mut self) {
        let RenderState::Active {
            window,
            surface,
            renderer,
        } = &mut self.render_state
        else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        let snapshot = self.ui.scene();
        let mut display_tree = snapshot.display_tree();
        display_tree.layout(
            &mut self.text,
            snapshot.view_rect().origin(),
            BoxConstraints::tight(snapshot.view_rect().size()),
        );
        let imaging_scene = imaging_scene_from_display_tree(&display_tree);
        let rgba = renderer
            .render_scene(
                &imaging_scene,
                u16::try_from(size.width).expect("window width exceeds u16"),
                u16::try_from(size.height).expect("window height exceeds u16"),
            )
            .expect("render imaging scene");

        let mut buffer = surface.buffer_mut().expect("lock softbuffer");
        for (pixel, rgba) in buffer.iter_mut().zip(rgba.data.chunks_exact(4)) {
            *pixel = u32::from_le_bytes([rgba[2], rgba[1], rgba[0], 0]);
        }
        buffer.present().expect("present softbuffer buffer");
    }
}

impl ApplicationHandler for DemoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if matches!(self.render_state, RenderState::Active { .. }) {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("Overstory + imaging")
            .with_inner_size(PhysicalSize::new(960, 640));
        let window = Rc::new(event_loop.create_window(attrs).expect("create window"));
        let context = softbuffer::Context::new(window.clone()).expect("create softbuffer context");
        let mut surface =
            softbuffer::Surface::new(&context, window.clone()).expect("create softbuffer surface");
        let size = window.inner_size();
        surface
            .resize(
                NonZeroU32::new(size.width.max(1)).expect("non-zero width"),
                NonZeroU32::new(size.height.max(1)).expect("non-zero height"),
            )
            .expect("resize initial surface");
        self.resize_ui(size);
        let renderer = Box::new(VelloCpuRenderer::new(
            u16::try_from(size.width.max(1)).expect("window width exceeds u16"),
            u16::try_from(size.height.max(1)).expect("window height exceeds u16"),
        ));
        window.request_redraw();
        self.render_state = RenderState::Active {
            window,
            surface,
            renderer,
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
                WindowEventTranslation::Keyboard(keyboard) => {
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
                }
                WindowEventTranslation::Pointer(_) => {
                    self.process_pointer_translation(translation, &window);
                    return;
                }
            }
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize_active_surface(size),
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

    let search = append_button(&mut ui, content_column, &button_cascade, "Search", false);
    let settings = append_button(&mut ui, content_column, &button_cascade, "Settings", false);
    let deploy = append_button(&mut ui, content_column, &button_cascade, "Deploy", true);

    ui.set_local(deploy, ui.properties().foreground, palette::css::WHITE);

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
        app.resize_ui(PhysicalSize::new(960, 640));

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
        app.resize_ui(PhysicalSize::new(960, 640));
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
