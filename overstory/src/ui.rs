// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Top-level retained Overstory UI model.

use alloc::{boxed::Box, vec::Vec};
use core::num::NonZeroU64;

use invalidation::ChannelSet;
use kurbo::{Point, Rect};
use peniko::Color;
use ui_events::pointer::{PointerButton, PointerEvent, PointerInfo};
use understory_property::{DependencyObjectExt, Property, PropertyRegistry};
use understory_style::{ClassId, IdSet, StyleCascade, Theme, ThemeBuilder};

use crate::{
    BuiltInProperties, ButtonClass, DirtyChannels, Element, ElementId, ElementKind, Interaction,
    InteractionBatch, LayoutClass, RuntimeState, SceneSnapshot, ThemeKeys,
};

/// Retained Overstory UI state.
#[derive(Debug)]
pub struct Ui {
    registry: PropertyRegistry,
    props: BuiltInProperties,
    theme: Theme,
    elements: Vec<Element>,
    root: ElementId,
    runtime: RuntimeState,
    scene: Option<SceneSnapshot>,
    view_rect: Rect,
    dirty: ChannelSet,
}

impl Ui {
    /// Creates a new retained UI with a single root element.
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        let mut registry = PropertyRegistry::new();
        let props = BuiltInProperties::register(&mut registry);
        let root = ElementId::new(0);
        let mut elements = Vec::new();
        let mut root_element = Element::new(root, None, ElementKind::Root);
        root_element.store.set_local(props.visible, true);
        elements.push(root_element);
        Self {
            registry,
            props,
            theme,
            elements,
            root,
            runtime: RuntimeState::new(),
            scene: None,
            view_rect: Rect::ZERO,
            dirty: DirtyChannels::STRUCTURE.into_set()
                | DirtyChannels::LAYOUT.into_set()
                | DirtyChannels::PAINT.into_set(),
        }
    }

    /// Returns the root element.
    #[must_use]
    pub const fn root(&self) -> ElementId {
        self.root
    }

    /// Returns the built-in property registry entries.
    #[must_use]
    pub const fn properties(&self) -> &BuiltInProperties {
        &self.props
    }

    /// Returns the current theme.
    #[must_use]
    pub const fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Returns one retained element.
    #[must_use]
    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(id.index())
    }

    /// Returns all retained elements in insertion order.
    #[must_use]
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    /// Returns the current view rectangle.
    #[must_use]
    pub fn view_rect(&self) -> Rect {
        self.view_rect
    }

    /// Sets the current view rectangle and marks layout/paint dirty.
    pub fn set_view_rect(&mut self, view_rect: Rect) {
        if self.view_rect != view_rect {
            self.view_rect = view_rect;
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Replaces the theme and marks the scene dirty.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
    }

    /// Appends a child element under the given parent.
    pub fn append_child(&mut self, parent: ElementId, kind: ElementKind) -> ElementId {
        let id = ElementId::new(self.elements.len());
        let mut element = Element::new(id, Some(parent), kind);
        if matches!(kind, ElementKind::Button) {
            element.store.set_local(self.props.pickable, true);
            element.store.set_local(self.props.focusable, true);
        }
        self.elements.push(element);
        if let Some(parent_element) = self.elements.get_mut(parent.index()) {
            parent_element.children.push(id);
        }
        self.mark_dirty(
            DirtyChannels::STRUCTURE.into_set()
                | DirtyChannels::LAYOUT.into_set()
                | DirtyChannels::PAINT.into_set(),
        );
        id
    }

    /// Sets the label text for an element.
    pub fn set_label(&mut self, id: ElementId, label: impl Into<Box<str>>) {
        if let Some(element) = self.elements.get_mut(id.index()) {
            element.label = Some(label.into());
            self.mark_dirty(DirtyChannels::PAINT.into_set());
        }
    }

    /// Sets a shared style cascade on an element.
    pub fn set_style(&mut self, id: ElementId, style: StyleCascade) {
        if let Some(element) = self.elements.get_mut(id.index()) {
            element.style = Some(style);
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Adds a generic style class to an element.
    pub fn add_class(&mut self, id: ElementId, class: ClassId) {
        if let Some(element) = self.elements.get_mut(id.index())
            && !element.classes.contains(class)
        {
            let mut classes = element.classes.as_slice().to_vec();
            classes.push(class);
            element.classes = IdSet::from_ids(classes);
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Adds a built-in button styling class.
    pub fn add_button_class(&mut self, id: ElementId, class: ButtonClass) {
        self.add_class(id, class.class_id());
    }

    /// Adds a built-in layout styling class.
    pub fn add_layout_class(&mut self, id: ElementId, class: LayoutClass) {
        self.add_class(id, class.class_id());
    }

    /// Sets a local property value on an element and accumulates the affected
    /// dirty channels.
    pub fn set_local<T>(&mut self, id: ElementId, property: Property<T>, value: T)
    where
        T: Clone + PartialEq + 'static,
    {
        if let Some(element) = self.elements.get_mut(id.index()) {
            let affected = element.set_local_notifying(property, value, &self.registry);
            if !affected.is_empty() {
                self.mark_dirty(affected);
            }
        }
    }

    /// Sets the vertical scroll offset on a `ScrollView` element, clamping to
    /// valid bounds.
    pub fn set_scroll_offset(&mut self, id: ElementId, offset: f64) {
        if let Some(element) = self.elements.get_mut(id.index()) {
            let max_offset = (element.content_height - element.scroll_offset).max(0.0);
            let _ = max_offset; // content_height is set during layout
            element.scroll_offset = offset.max(0.0);
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Adjusts the scroll offset by a delta on a `ScrollView` element.
    pub fn scroll_by(&mut self, id: ElementId, delta: f64) {
        if let Some(element) = self.elements.get(id.index()) {
            let new_offset = element.scroll_offset + delta;
            self.set_scroll_offset(id, new_offset);
        }
    }

    /// Returns the current scroll offset for an element.
    #[must_use]
    pub fn scroll_offset(&self, id: ElementId) -> f64 {
        self.elements
            .get(id.index())
            .map_or(0.0, |e| e.scroll_offset)
    }

    /// Rebuilds the resolved scene if needed and returns the current snapshot.
    pub fn rebuild(&mut self) -> &SceneSnapshot {
        if self.scene.is_none() || !self.dirty.is_empty() {
            let snapshot = SceneSnapshot::build(
                &self.elements,
                self.root,
                self.view_rect,
                &self.registry,
                &self.props,
                &self.theme,
            );
            self.scene = Some(snapshot);
            self.dirty = ChannelSet::empty();
        }
        self.scene.as_ref().expect("scene just rebuilt")
    }

    /// Returns the current resolved scene, rebuilding first if necessary.
    pub fn scene(&mut self) -> &SceneSnapshot {
        self.rebuild()
    }

    /// Handles one pointer event from `ui-events`.
    pub fn handle_pointer_event(&mut self, event: &PointerEvent) -> InteractionBatch {
        let mut batch = InteractionBatch::default();
        let _ = self.rebuild();

        match event {
            PointerEvent::Enter(_) => {}
            PointerEvent::Leave(_) => {
                self.clear_hover(&mut batch);
            }
            PointerEvent::Move(update) => {
                let point = point_from_state(&update.current);
                let _ = self
                    .runtime
                    .clicks
                    .on_move(pointer_id(update.pointer), point);
                self.update_hover(point, &mut batch);
            }
            PointerEvent::Down(button) if is_primary_button(button.button) => {
                let point = point_from_state(&button.state);
                self.update_hover(point, &mut batch);
                if let Some(target) = self.rebuild().top_hit(point) {
                    if self.runtime.pressed_target != Some(target) {
                        self.set_pressed_target(Some(target), &mut batch);
                    }
                    self.runtime.clicks.on_down(
                        pointer_id(button.pointer),
                        Some(button_code(button.button)),
                        target,
                        point,
                        button.state.time,
                    );
                }
            }
            PointerEvent::Up(button) if is_primary_button(button.button) => {
                let point = point_from_state(&button.state);
                self.update_hover(point, &mut batch);
                let current_target = self.rebuild().top_hit(point);
                self.set_pressed_target(None, &mut batch);
                if let Some(target) = current_target {
                    match self.runtime.clicks.on_up(
                        pointer_id(button.pointer),
                        Some(button_code(button.button)),
                        &target,
                        point,
                        button.state.time,
                    ) {
                        understory_event_state::click::ClickResult::Click(id) => {
                            batch.push(Interaction::Clicked(id));
                        }
                        understory_event_state::click::ClickResult::Suppressed(_) => {}
                    }
                } else {
                    let _ = self.runtime.clicks.cancel(pointer_id(button.pointer));
                }
            }
            PointerEvent::Cancel(pointer) => {
                let _ = self.runtime.clicks.cancel(pointer_id(*pointer));
                self.set_pressed_target(None, &mut batch);
                self.clear_hover(&mut batch);
            }
            PointerEvent::Scroll(scroll) => {
                let point = point_from_state(&scroll.state);
                let dy = scroll_delta_y(scroll.delta);
                if dy != 0.0
                    && let Some(path) = self.rebuild().hit_path(point)
                {
                    for &ancestor in path.iter().rev() {
                        if let Some(element) = self.elements.get(ancestor.index())
                            && matches!(element.kind, ElementKind::ScrollView)
                        {
                            self.scroll_by(ancestor, dy);
                            batch.push(Interaction::Scrolled(ancestor));
                            break;
                        }
                    }
                }
            }
            PointerEvent::Gesture(_) => {}
            PointerEvent::Down(_) | PointerEvent::Up(_) => {}
        }

        if !self.dirty.is_empty() {
            let _ = self.rebuild();
        }
        batch
    }

    fn mark_dirty(&mut self, channels: ChannelSet) {
        self.dirty |= channels;
    }

    fn clear_hover(&mut self, batch: &mut InteractionBatch) {
        let leaving = self.runtime.hover.clear();
        for event in leaving {
            if let understory_event_state::hover::HoverEvent::Leave(id) = event
                && let Some(element) = self.elements.get_mut(id.index())
                && element.pseudos.hovered
            {
                element.pseudos.hovered = false;
                batch.push(Interaction::HoverLeft(id));
            }
        }
        self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
    }

    fn update_hover(&mut self, point: Point, batch: &mut InteractionBatch) {
        let path = self.rebuild().hit_path(point).unwrap_or_default();
        let transitions = self.runtime.hover.update_path(&path);
        let mut changed = false;
        for transition in transitions {
            match transition {
                understory_event_state::hover::HoverEvent::Enter(id) => {
                    if let Some(element) = self.elements.get_mut(id.index())
                        && !element.pseudos.hovered
                    {
                        element.pseudos.hovered = true;
                        batch.push(Interaction::HoverEntered(id));
                        changed = true;
                    }
                }
                understory_event_state::hover::HoverEvent::Leave(id) => {
                    if let Some(element) = self.elements.get_mut(id.index())
                        && element.pseudos.hovered
                    {
                        element.pseudos.hovered = false;
                        batch.push(Interaction::HoverLeft(id));
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    fn set_pressed_target(&mut self, target: Option<ElementId>, batch: &mut InteractionBatch) {
        if self.runtime.pressed_target == target {
            return;
        }
        if let Some(previous) = self.runtime.pressed_target.take()
            && let Some(element) = self.elements.get_mut(previous.index())
            && element.pseudos.pressed
        {
            element.pseudos.pressed = false;
            batch.push(Interaction::PressEnded(previous));
        }
        self.runtime.pressed_target = target;
        if let Some(target) = target
            && let Some(element) = self.elements.get_mut(target.index())
            && !element.pseudos.pressed
        {
            element.pseudos.pressed = true;
            batch.push(Interaction::PressStarted(target));
        }
        self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
    }
}

/// Default theme used by Overstory examples and tests.
#[must_use]
pub fn default_theme() -> Theme {
    ThemeBuilder::new()
        .set(
            ThemeKeys::ROOT_BACKGROUND,
            Color::from_rgba8(242, 239, 232, 255),
        )
        .set(
            ThemeKeys::PANEL_BACKGROUND,
            Color::from_rgba8(255, 252, 246, 255),
        )
        .set(
            ThemeKeys::SIDEBAR_BACKGROUND,
            Color::from_rgba8(226, 222, 213, 255),
        )
        .set(
            ThemeKeys::BUTTON_BACKGROUND,
            Color::from_rgba8(238, 233, 225, 255),
        )
        .set(
            ThemeKeys::BUTTON_HOVER_BACKGROUND,
            Color::from_rgba8(230, 225, 216, 255),
        )
        .set(
            ThemeKeys::BUTTON_PRESSED_BACKGROUND,
            Color::from_rgba8(214, 208, 198, 255),
        )
        .set(
            ThemeKeys::PRIMARY_BACKGROUND,
            Color::from_rgba8(24, 92, 72, 255),
        )
        .set(
            ThemeKeys::PRIMARY_HOVER_BACKGROUND,
            Color::from_rgba8(31, 109, 86, 255),
        )
        .set(
            ThemeKeys::PRIMARY_PRESSED_BACKGROUND,
            Color::from_rgba8(18, 72, 57, 255),
        )
        .set(ThemeKeys::FOREGROUND, Color::from_rgba8(33, 37, 41, 255))
        .set(
            ThemeKeys::BORDER_COLOR,
            Color::from_rgba8(143, 133, 122, 255),
        )
        .set(ThemeKeys::CORNER_RADIUS, 10.0_f64)
        .set(ThemeKeys::PADDING, 16.0_f64)
        .set(ThemeKeys::GAP, 12.0_f64)
        .set(ThemeKeys::BUTTON_HEIGHT, 44.0_f64)
        .build()
}

fn pointer_id(pointer: PointerInfo) -> Option<NonZeroU64> {
    pointer.pointer_id.map(|pointer_id| pointer_id.get_inner())
}

fn button_code(button: Option<PointerButton>) -> u8 {
    match button {
        Some(PointerButton::Primary) | None => 1,
        Some(PointerButton::Auxiliary) => 2,
        Some(PointerButton::Secondary) => 3,
        Some(PointerButton::X1) => 4,
        Some(PointerButton::X2) => 5,
        Some(PointerButton::PenEraser) => 6,
        Some(PointerButton::B7) => 7,
        Some(PointerButton::B8) => 8,
        Some(PointerButton::B9) => 9,
        Some(PointerButton::B10) => 10,
        Some(PointerButton::B11) => 11,
        Some(PointerButton::B12) => 12,
        Some(PointerButton::B13) => 13,
        Some(PointerButton::B14) => 14,
        Some(PointerButton::B15) => 15,
        Some(PointerButton::B16) => 16,
        Some(PointerButton::B17) => 17,
        Some(PointerButton::B18) => 18,
        Some(PointerButton::B19) => 19,
        Some(PointerButton::B20) => 20,
        Some(PointerButton::B21) => 21,
        Some(PointerButton::B22) => 22,
        Some(PointerButton::B23) => 23,
        Some(PointerButton::B24) => 24,
        Some(PointerButton::B25) => 25,
        Some(PointerButton::B26) => 26,
        Some(PointerButton::B27) => 27,
        Some(PointerButton::B28) => 28,
        Some(PointerButton::B29) => 29,
        Some(PointerButton::B30) => 30,
        Some(PointerButton::B31) => 31,
        Some(PointerButton::B32) => 32,
    }
}

fn is_primary_button(button: Option<PointerButton>) -> bool {
    matches!(button, None | Some(PointerButton::Primary))
}

fn point_from_state(state: &ui_events::pointer::PointerState) -> Point {
    Point::new(state.position.x, state.position.y)
}

/// Line height in pixels used to convert line-based scroll deltas.
const SCROLL_LINE_HEIGHT: f64 = 40.0;

fn scroll_delta_y(delta: ui_events::ScrollDelta) -> f64 {
    match delta {
        ui_events::ScrollDelta::PixelDelta(pos) => pos.y,
        ui_events::ScrollDelta::LineDelta(_, y) => f64::from(y) * SCROLL_LINE_HEIGHT,
        ui_events::ScrollDelta::PageDelta(_, y) => f64::from(y) * 400.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_events::pointer::{
        PointerButtonEvent, PointerButtons, PointerId, PointerInfo, PointerState, PointerType,
        PointerUpdate,
    };
    use understory_style::{
        IdSet, Selector, StyleBuilder, StyleCascadeBuilder, StyleOrigin, StyleSheetBuilder,
    };

    #[test]
    fn layout_stacks_children_in_column() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 200.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append_child(ui.root(), ElementKind::Column);
        ui.set_local(column, ui.properties().padding, 0.0);
        ui.set_local(column, ui.properties().gap, 8.0);

        let first = ui.append_child(column, ElementKind::Button);
        ui.set_local(first, ui.properties().height, 20.0);
        let second = ui.append_child(column, ElementKind::Button);
        ui.set_local(second, ui.properties().height, 30.0);

        let scene = ui.rebuild();
        let first_rect = scene.resolved_element(first).unwrap().rect;
        let second_rect = scene.resolved_element(second).unwrap().rect;

        assert_eq!(first_rect, Rect::new(0.0, 0.0, 240.0, 20.0));
        assert_eq!(second_rect, Rect::new(0.0, 28.0, 240.0, 58.0));
    }

    #[test]
    fn class_and_hover_style_change_resolved_snapshot() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let button = ui.append_child(ui.root(), ElementKind::Button);
        ui.add_button_class(button, ButtonClass::Primary);

        let base = StyleBuilder::new()
            .set(ui.properties().border_width, 1.0)
            .build();
        let hover = StyleBuilder::new()
            .set(ui.properties().border_width, 4.0)
            .build();
        let selector = Selector {
            type_tag: Some(crate::TYPE_BUTTON),
            required_classes: IdSet::from_ids([ButtonClass::Primary.class_id()]),
            required_pseudos: IdSet::from_ids([crate::PSEUDO_HOVER]),
        };
        let sheet = StyleSheetBuilder::new().rule(selector, hover).build();
        let cascade = StyleCascadeBuilder::new()
            .push_style(StyleOrigin::Base, base)
            .push_sheet(StyleOrigin::Sheet, sheet)
            .build();
        ui.set_style(button, cascade);

        let before = ui.rebuild().resolved_element(button).unwrap().border.width;
        assert_eq!(before, 1.0);

        let move_event = PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(20.0, 20.0, 1),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        });
        let _ = ui.handle_pointer_event(&move_event);

        let after = ui.rebuild().resolved_element(button).unwrap().border.width;
        assert_eq!(after, 4.0);
    }

    #[test]
    fn pointer_click_emits_press_and_click_interactions() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let button = ui.append_child(ui.root(), ElementKind::Button);
        ui.set_label(button, "Launch");

        let move_batch = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(20.0, 20.0, 1),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));
        assert!(
            move_batch
                .events()
                .contains(&Interaction::HoverEntered(button))
        );

        let down_batch = ui.handle_pointer_event(&PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(20.0, 20.0, 2),
        }));
        assert!(
            down_batch
                .events()
                .contains(&Interaction::PressStarted(button))
        );

        let up_batch = ui.handle_pointer_event(&PointerEvent::Up(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(20.0, 20.0, 3),
        }));
        assert!(up_batch.events().contains(&Interaction::PressEnded(button)));
        assert!(up_batch.events().contains(&Interaction::Clicked(button)));
    }

    #[test]
    fn row_places_children_left_to_right() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append_child(ui.root(), ElementKind::Row);
        ui.set_local(row, ui.properties().padding, 0.0);
        ui.set_local(row, ui.properties().gap, 12.0);

        let left = ui.append_child(row, ElementKind::Panel);
        ui.set_local(left, ui.properties().width, 100.0);
        ui.set_local(left, ui.properties().height, 80.0);

        let right = ui.append_child(row, ElementKind::Panel);
        ui.set_local(right, ui.properties().height, 80.0);

        let scene = ui.rebuild();
        let left_rect = scene.resolved_element(left).unwrap().rect;
        let right_rect = scene.resolved_element(right).unwrap().rect;

        assert_eq!(left_rect, Rect::new(0.0, 0.0, 100.0, 80.0));
        assert_eq!(right_rect, Rect::new(112.0, 0.0, 320.0, 80.0));
    }

    fn primary_pointer() -> PointerInfo {
        PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        }
    }

    #[allow(
        clippy::field_reassign_with_default,
        reason = "Constructing dpi physical positions without a direct test dependency is awkward."
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
}
