// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Top-level retained Overstory UI model.

use alloc::{boxed::Box, vec::Vec};
use core::num::NonZeroU64;

use invalidation::ChannelSet;
use kurbo::{Point, Rect};
use peniko::Color;
use ui_events::pointer::{PointerButton, PointerEvent, PointerInfo};
use understory_box_tree::NodeFlags;
use understory_display::{TextAlign, TextEngine};
use understory_property::{DependencyObjectExt, Property, PropertyRegistry};
use understory_style::{ClassId, IdSet, StyleCascade, Theme, ThemeBuilder, TypeTag};

use crate::{
    BuiltInProperties, ButtonClass, DirtyChannels, Element, ElementId, Interaction,
    InteractionBatch, LayoutClass, RuntimeState, SceneSnapshot, TYPE_BUTTON, TYPE_COLUMN,
    TYPE_PANEL, TYPE_ROOT, TYPE_ROW, TYPE_SCROLL_VIEW, TYPE_SPACER, TYPE_TEXT_BLOCK,
    TYPE_TEXT_INPUT, ThemeKeys, Widget, WidgetArena,
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
    widget_arena: WidgetArena,
    timers: crate::TimerQueue,
    /// Current monotonic time in nanoseconds, set by the host before
    /// each event cycle via `set_now`.
    now: u64,
}

impl Ui {
    /// Creates a new retained UI with a single root element.
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        let mut registry = PropertyRegistry::new();
        let props = BuiltInProperties::register(&mut registry);
        let root = ElementId::new(0);
        let mut elements = Vec::new();
        let mut root_element = Element::new(root, None, TYPE_ROOT);
        root_element.store.set_local(props.visible, true);
        root_element.is_root = true;
        root_element.is_container = true;
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
            widget_arena: WidgetArena::new(),
            timers: crate::TimerQueue::new(),
            now: 0,
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

    /// Appends a child element under the given parent with a type tag and
    /// optional widget.
    pub fn append_child_with(
        &mut self,
        parent: ElementId,
        type_tag: TypeTag,
        widget: Option<Box<dyn Widget>>,
    ) -> ElementId {
        let id = ElementId::new(self.elements.len());
        let mut element = Element::new(id, Some(parent), type_tag);
        if let Some(w) = &widget {
            if w.default_pickable() {
                element.store.set_local(self.props.pickable, true);
            }
            if w.default_focusable() {
                element.store.set_local(self.props.focusable, true);
            }
        }
        if let Some(w) = widget {
            let handle = self.widget_arena.insert(w);
            element.widget = Some(handle);
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

    /// Appends a container child (Panel, Column, Row, etc.) with no widget.
    pub fn append_container(
        &mut self,
        parent: ElementId,
        type_tag: TypeTag,
        horizontal: bool,
    ) -> ElementId {
        let id = self.append_child_with(parent, type_tag, None);
        if let Some(element) = self.elements.get_mut(id.index()) {
            element.is_container = true;
            element.horizontal = horizontal;
        }
        id
    }

    /// Appends a child element with a built-in element type.
    ///
    /// This is a convenience wrapper that creates the appropriate widget and
    /// sets structural flags based on the type tag.
    pub fn append_child(&mut self, parent: ElementId, type_tag: TypeTag) -> ElementId {
        match type_tag {
            TYPE_ROOT | TYPE_PANEL | TYPE_COLUMN => self.append_container(parent, type_tag, false),
            TYPE_ROW => self.append_container(parent, type_tag, true),
            TYPE_SCROLL_VIEW => {
                let widget = crate::widgets::ScrollViewWidget::new();
                let id = self.append_container(parent, type_tag, false);
                let handle = self.widget_arena.insert(Box::new(widget));
                if let Some(element) = self.elements.get_mut(id.index()) {
                    element.widget = Some(handle);
                }
                id
            }
            TYPE_BUTTON => self.append_child_with(
                parent,
                type_tag,
                Some(Box::new(crate::widgets::ButtonWidget::new())),
            ),
            TYPE_TEXT_BLOCK => self.append_child_with(
                parent,
                type_tag,
                Some(Box::new(crate::widgets::TextBlockWidget::new())),
            ),
            TYPE_TEXT_INPUT => self.append_child_with(
                parent,
                type_tag,
                Some(Box::new(crate::widgets::TextInputWidget::new(16.0))),
            ),
            TYPE_SPACER => self.append_child_with(parent, type_tag, None),
            _ => self.append_child_with(parent, type_tag, None),
        }
    }

    /// Sets the label text for an element.
    pub fn set_label(&mut self, id: ElementId, label: impl Into<Box<str>>) {
        if let Some(element) = self.elements.get_mut(id.index()) {
            element.label = Some(label.into());
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
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
        if let Some(w) = self.widget_mut::<crate::widgets::ScrollViewWidget>(id) {
            w.set_scroll_offset(offset);
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Returns the measured content height of a `ScrollView` element.
    #[must_use]
    pub fn content_height(&self, id: ElementId) -> f64 {
        self.widget::<crate::widgets::ScrollViewWidget>(id)
            .map_or(0.0, |w| w.content_height())
    }

    /// Returns the viewport height of a `ScrollView` element from last layout.
    #[must_use]
    pub fn viewport_height(&self, id: ElementId) -> f64 {
        self.widget::<crate::widgets::ScrollViewWidget>(id)
            .map_or(0.0, |w| w.viewport_height())
    }

    /// Adjusts the scroll offset by a delta on a `ScrollView` element.
    pub fn scroll_by(&mut self, id: ElementId, delta: f64) {
        if let Some(w) = self.widget_mut::<crate::widgets::ScrollViewWidget>(id) {
            w.scroll_by(delta);
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Returns a typed reference to the widget attached to an element.
    #[must_use]
    pub fn widget<W: Widget + 'static>(&self, id: ElementId) -> Option<&W> {
        let handle = self.elements.get(id.index())?.widget?;
        self.widget_arena.get(handle)?.as_any().downcast_ref::<W>()
    }

    /// Returns a typed mutable reference to the widget attached to an element.
    #[must_use]
    pub fn widget_mut<W: Widget + 'static>(&mut self, id: ElementId) -> Option<&mut W> {
        let handle = self.elements.get(id.index())?.widget?;
        self.widget_arena
            .get_mut(handle)?
            .as_any_mut()
            .downcast_mut::<W>()
    }

    /// Returns a reference to the widget arena.
    #[must_use]
    pub fn widget_arena(&self) -> &WidgetArena {
        &self.widget_arena
    }

    /// Sets the current monotonic time. Call this before processing events
    /// each frame so timer-dependent operations (focus, blink) use the
    /// correct time.
    pub fn set_now(&mut self, now_nanos: u64) {
        self.now = now_nanos;
    }

    /// Requests a timer for a widget. `now` is the current monotonic time
    /// in nanoseconds. `delay` is in nanoseconds. If `repeat` is true, the
    /// timer re-arms after firing.
    pub fn request_timer(
        &mut self,
        element_id: ElementId,
        now: u64,
        delay: u64,
        repeat: bool,
    ) -> crate::TimerId {
        self.timers.request(
            element_id,
            now,
            delay,
            if repeat { Some(delay) } else { None },
        )
    }

    /// Cancels a pending timer.
    pub fn cancel_timer(&mut self, id: crate::TimerId) {
        self.timers.cancel(id);
    }

    /// Returns the next timer deadline in nanoseconds, or `None` if no
    /// timers are pending.
    #[must_use]
    pub fn next_deadline(&self) -> Option<u64> {
        self.timers.next_deadline()
    }

    /// Advances timers to `now_nanos`, firing expired timers by calling
    /// `Widget::on_timer` on each. Returns `true` if any timer fired
    /// (the caller should request a redraw).
    pub fn tick(&mut self, now_nanos: u64) -> bool {
        let fired = self.timers.drain_expired(now_nanos);
        if fired.is_empty() {
            return false;
        }
        for (timer_id, element_id) in &fired {
            if let Some(handle) = self.elements.get(element_id.index()).and_then(|e| e.widget)
                && let Some(widget) = self.widget_arena.get_mut(handle)
            {
                widget.on_timer(*timer_id, now_nanos);
            }
        }
        self.mark_dirty(DirtyChannels::PAINT.into_set());
        true
    }

    /// Returns the current scroll offset for an element.
    #[must_use]
    pub fn scroll_offset(&self, id: ElementId) -> f64 {
        self.widget::<crate::widgets::ScrollViewWidget>(id)
            .map_or(0.0, |w| w.scroll_offset())
    }

    /// Sets keyboard focus to an element.
    /// Sets keyboard focus to an element. Uses the current monotonic time
    /// (set via `set_now`) to start cursor blink timers.
    pub fn set_focus(&mut self, id: ElementId) {
        let now = self.now;
        if self.runtime.focused == Some(id) {
            return;
        }
        // Stop blink on previously focused element.
        if let Some(prev) = self.runtime.focused.take() {
            if let Some(element) = self.elements.get_mut(prev.index()) {
                element.pseudos.focused = false;
            }
            if let Some(handle) = self.elements.get(prev.index()).and_then(|e| e.widget)
                && let Some(w) = self.widget_arena.get_mut(handle)
                && let Some(ti) = w.as_any_mut().downcast_mut::<crate::widgets::TextInputWidget>()
            {
                ti.stop_blink(&mut self.timers);
            }
        }
        self.runtime.focused = Some(id);
        if let Some(element) = self.elements.get_mut(id.index()) {
            element.pseudos.focused = true;
        }
        // Start blink on newly focused TextInput.
        if let Some(handle) = self.elements.get(id.index()).and_then(|e| e.widget)
            && let Some(w) = self.widget_arena.get_mut(handle)
            && let Some(ti) = w.as_any_mut().downcast_mut::<crate::widgets::TextInputWidget>()
        {
            ti.start_blink(&mut self.timers, id, now);
        }
        self.mark_dirty(DirtyChannels::PAINT.into_set());
    }

    /// Returns the current text buffer for a `TextInput` element.
    #[must_use]
    pub fn text_buffer(&self, id: ElementId) -> &str {
        self.widget::<crate::widgets::TextInputWidget>(id)
            .map_or("", |w| w.text())
    }

    /// Clears the text buffer for a `TextInput` element.
    pub fn clear_text_buffer(&mut self, id: ElementId, text: &mut TextEngine) {
        if let Some(w) = self.widget_mut::<crate::widgets::TextInputWidget>(id) {
            w.clear(text);
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Handles one keyboard event from `ui-events`.
    ///
    /// Delegates to the focused element's widget if it has one.
    pub fn handle_keyboard_event(
        &mut self,
        event: &ui_events::keyboard::KeyboardEvent,
        text: &mut TextEngine,
    ) -> InteractionBatch {
        let mut batch = InteractionBatch::default();
        let Some(focused) = self.runtime.focused else {
            return batch;
        };
        if let Some(handle) = self.elements.get(focused.index()).and_then(|e| e.widget)
            && let Some(widget) = self.widget_arena.get_mut(handle)
        {
            let handled = widget.keyboard_event(focused, event, text, &mut batch);
            if handled {
                self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
            }
        }
        batch
    }

    /// Updates tooltip visibility and positioning based on current hover state.
    ///
    /// Tooltips become visible when their trigger element is hovered and
    /// are positioned below the trigger's resolved rect.
    pub fn update_tooltips(&mut self, text: &mut TextEngine) {
        // Collect tooltip info: (tooltip_id, trigger_id).
        let tooltips: Vec<(ElementId, ElementId)> = self
            .elements
            .iter()
            .filter_map(|el| {
                let handle = el.widget?;
                let widget = self.widget_arena.get(handle)?;
                let tw = widget
                    .as_any()
                    .downcast_ref::<crate::widgets::TooltipWidget>()?;
                Some((el.id, tw.trigger()))
            })
            .collect();

        if tooltips.is_empty() {
            return;
        }

        // Rebuild to get current hover state.
        let _ = self.rebuild(text);
        let snapshot = self.scene.as_ref().expect("scene rebuilt");

        // Collect trigger state before mutating widgets.
        let trigger_state: Vec<(ElementId, bool, Option<Rect>)> = tooltips
            .iter()
            .map(|(tooltip_id, trigger_id)| {
                let resolved = snapshot.resolved_element(*trigger_id);
                let hovered = resolved.is_some_and(|r| r.hovered);
                let rect = resolved.map(|r| r.rect);
                (*tooltip_id, hovered, rect)
            })
            .collect();

        // Apply visibility and positioning.
        let mut changed = false;
        for (tooltip_id, hovered, trigger_rect) in &trigger_state {
            if let Some(tw) = self.widget_mut::<crate::widgets::TooltipWidget>(*tooltip_id) {
                if tw.is_visible() != *hovered {
                    tw.set_visible(*hovered);
                    changed = true;
                }
                if *hovered && let Some(rect) = trigger_rect {
                    tw.set_position(Point::new(rect.x0, rect.y1 + 4.0));
                }
            }
        }
        if changed {
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Refreshes widget layouts (e.g., text editor glyph positions) before
    /// cursor/selection geometry.
    pub fn refresh_editors(&mut self, text: &mut TextEngine) {
        for (_handle, widget) in self.widget_arena.iter_mut() {
            widget.refresh_layout(text);
        }
    }

    /// Rebuilds the resolved scene if needed and returns the current snapshot.
    pub fn rebuild(&mut self, text: &mut TextEngine) -> &SceneSnapshot {
        if self.scene.is_none() || !self.dirty.is_empty() {
            let (snapshot, scroll_metrics) = SceneSnapshot::build(
                &self.elements,
                self.root,
                self.view_rect,
                &self.registry,
                &self.props,
                &self.theme,
                &self.widget_arena,
                text,
            );
            let mut needs_rebuild = false;
            for (id, content_h, viewport_h) in &scroll_metrics {
                if let Some(w) = self.widget_mut::<crate::widgets::ScrollViewWidget>(*id) {
                    let old_offset = w.scroll_offset();
                    w.set_layout_metrics(*content_h, *viewport_h);
                    if (w.scroll_offset() - old_offset).abs() > f64::EPSILON {
                        needs_rebuild = true;
                    }
                }
            }
            if needs_rebuild {
                let (snapshot, _) = SceneSnapshot::build(
                    &self.elements,
                    self.root,
                    self.view_rect,
                    &self.registry,
                    &self.props,
                    &self.theme,
                    &self.widget_arena,
                    text,
                );
                self.scene = Some(snapshot);
            } else {
                self.scene = Some(snapshot);
            }
            self.dirty = ChannelSet::empty();
        }
        self.scene.as_ref().expect("scene just rebuilt")
    }

    /// Returns the current resolved scene, rebuilding first if necessary.
    pub fn scene(&mut self, text: &mut TextEngine) -> &SceneSnapshot {
        self.rebuild(text)
    }

    /// Rebuilds the scene if needed and returns a display tree with widget
    /// rendering applied.
    pub fn display_tree(
        &mut self,
        text: &mut TextEngine,
    ) -> (understory_display::DisplayTree, Rect) {
        let _ = self.rebuild(text);
        let snapshot = self.scene.as_ref().expect("scene just rebuilt");
        let tree = snapshot.display_tree(&self.widget_arena);
        let view_rect = snapshot.view_rect();
        (tree, view_rect)
    }

    /// Builds a surface plan — the semantic visual output of Overstory.
    ///
    /// The plan contains one or more surfaces in painter/compositing order.
    /// Currently produces a single root surface. Overlay surfaces (popups,
    /// tooltips) will be added as widgets request promotion.
    ///
    /// Use `SurfacePlan::flatten_to_display_tree()` for compatibility with
    /// hosts that don't support layered composition.
    pub fn surface_plan(&mut self, text: &mut TextEngine) -> crate::SurfacePlan {
        let _ = self.rebuild(text);
        let snapshot = self.scene.as_ref().expect("scene just rebuilt");
        let view_rect = snapshot.view_rect();

        // Find elements whose widgets request surface promotion.
        let mut promoted = Vec::new();
        for element in &self.elements {
            if let Some(handle) = element.widget
                && let Some(widget) = self.widget_arena.get(handle)
                && let Some(role) = widget.surface_role()
            {
                promoted.push((element.id, role));
            }
        }

        let promoted_ids: Vec<_> = promoted.iter().map(|(id, _)| *id).collect();

        // Build root surface excluding promoted elements.
        let root_tree = snapshot.display_tree_excluding(&self.widget_arena, &promoted_ids);

        let mut plan = crate::SurfacePlan::new();
        plan.push(crate::SurfaceEntry {
            element_id: self.root,
            role: crate::SurfaceRole::Root,
            transform: kurbo::Affine::IDENTITY,
            bounds: view_rect,
            clip: None,
            opacity: 1.0,
            blend: crate::BlendModeHint::Normal,
            anchor: None,
            content: crate::SurfaceContent::Display(Box::new(root_tree)),
        });

        // Build overlay surfaces for promoted elements.
        for (id, role) in &promoted {
            if let Some(tree) = snapshot.display_tree_for(&self.widget_arena, *id) {
                let layout_rect = snapshot
                    .resolved_element(*id)
                    .map_or(Rect::ZERO, |r| r.rect);
                // Use the widget's desired position if set (e.g., tooltip
                // positioning relative to trigger), otherwise use layout rect.
                let bounds = self
                    .widget::<crate::widgets::TooltipWidget>(*id)
                    .and_then(|tw| {
                        tw.position()
                            .map(|pos| Rect::from_origin_size(pos, layout_rect.size()))
                    })
                    .unwrap_or(layout_rect);
                plan.push(crate::SurfaceEntry {
                    element_id: *id,
                    role: *role,
                    transform: kurbo::Affine::IDENTITY,
                    bounds,
                    clip: None,
                    opacity: 1.0,
                    blend: crate::BlendModeHint::Normal,
                    anchor: None,
                    content: crate::SurfaceContent::Display(Box::new(tree)),
                });
            }
        }

        plan
    }

    /// Handles one pointer event from `ui-events`.
    pub fn handle_pointer_event(
        &mut self,
        event: &PointerEvent,
        text: &mut TextEngine,
    ) -> InteractionBatch {
        let mut batch = InteractionBatch::default();
        let _ = self.rebuild(text);

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
                self.update_hover(point, &mut batch, text);
            }
            PointerEvent::Down(button) if is_primary_button(button.button) => {
                let point = point_from_state(&button.state);
                self.update_hover(point, &mut batch, text);
                if let Some(target) = self.rebuild(text).top_hit(point) {
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
                self.update_hover(point, &mut batch, text);
                let current_target = self.rebuild(text).top_hit(point);
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
                            let scene = self.rebuild(text);
                            let is_focusable = scene
                                .node_for(id)
                                .and_then(|node| scene.box_tree().flags(node))
                                .is_some_and(|f| f.contains(NodeFlags::FOCUSABLE));
                            if is_focusable {
                                self.set_focus(id);
                                batch.push(Interaction::FocusChanged(id));
                                // Delegate click-to-position to widget.
                                if let Some(resolved) =
                                    self.rebuild(text).resolved_element(id).cloned()
                                    && let Some(handle) =
                                        self.elements.get(id.index()).and_then(|e| e.widget)
                                    && let Some(widget) = self.widget_arena.get_mut(handle)
                                {
                                    widget.click(id, point, &resolved, text);
                                    self.mark_dirty(DirtyChannels::PAINT.into_set());
                                }
                            }
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
                    && let Some(path) = self.rebuild(text).hit_path(point)
                {
                    for &ancestor in path.iter().rev() {
                        if self
                            .widget::<crate::widgets::ScrollViewWidget>(ancestor)
                            .is_some()
                        {
                            self.scroll_by(ancestor, -dy);
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
            let _ = self.rebuild(text);
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

    fn update_hover(&mut self, point: Point, batch: &mut InteractionBatch, text: &mut TextEngine) {
        let path = self.rebuild(text).hit_path(point).unwrap_or_default();
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
        .set(ThemeKeys::FONT_SIZE, 16.0_f64)
        .set(ThemeKeys::LABEL_PADDING, 12.0_f64)
        .set(ThemeKeys::FONT_FAMILY, Box::<str>::from("sans-serif"))
        .set(ThemeKeys::TEXT_ALIGN, TextAlign::Start)
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
    let scale = state.scale_factor.max(1.0);
    Point::new(state.position.x / scale, state.position.y / scale)
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
    use crate::PSEUDO_HOVER;
    use ui_events::pointer::{
        PointerButtonEvent, PointerButtons, PointerId, PointerInfo, PointerState, PointerType,
        PointerUpdate,
    };
    use understory_display::TextEngine;
    use understory_style::{
        IdSet, Selector, StyleBuilder, StyleCascadeBuilder, StyleOrigin, StyleSheetBuilder,
    };

    #[test]
    fn layout_stacks_children_in_column() {
        let mut ui = Ui::new(default_theme());
        let mut text = TextEngine::new();
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 200.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append_child(ui.root(), TYPE_COLUMN);
        ui.set_local(column, ui.properties().padding, 0.0);
        ui.set_local(column, ui.properties().gap, 8.0);

        let first = ui.append_child(column, TYPE_BUTTON);
        ui.set_local(first, ui.properties().height, 20.0);
        let second = ui.append_child(column, TYPE_BUTTON);
        ui.set_local(second, ui.properties().height, 30.0);

        let scene = ui.rebuild(&mut text);
        let first_rect = scene.resolved_element(first).unwrap().rect;
        let second_rect = scene.resolved_element(second).unwrap().rect;

        assert_eq!(first_rect, Rect::new(0.0, 0.0, 240.0, 20.0));
        assert_eq!(second_rect, Rect::new(0.0, 28.0, 240.0, 58.0));
    }

    #[test]
    fn class_and_hover_style_change_resolved_snapshot() {
        let mut ui = Ui::new(default_theme());
        let mut text = TextEngine::new();
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let button = ui.append_child(ui.root(), TYPE_BUTTON);
        ui.add_button_class(button, ButtonClass::Primary);

        let base = StyleBuilder::new()
            .set(ui.properties().border_width, 1.0)
            .build();
        let hover = StyleBuilder::new()
            .set(ui.properties().border_width, 4.0)
            .build();
        let selector = Selector {
            type_tag: Some(TYPE_BUTTON),
            required_classes: IdSet::from_ids([ButtonClass::Primary.class_id()]),
            required_pseudos: IdSet::from_ids([PSEUDO_HOVER]),
        };
        let sheet = StyleSheetBuilder::new().rule(selector, hover).build();
        let cascade = StyleCascadeBuilder::new()
            .push_style(StyleOrigin::Base, base)
            .push_sheet(StyleOrigin::Sheet, sheet)
            .build();
        ui.set_style(button, cascade);

        let before = ui
            .rebuild(&mut text)
            .resolved_element(button)
            .unwrap()
            .border
            .width;
        assert_eq!(before, 1.0);

        let move_event = PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(20.0, 20.0, 1),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        });
        let mut text = TextEngine::new();
        let _ = ui.handle_pointer_event(&move_event, &mut text);

        let after = ui
            .rebuild(&mut text)
            .resolved_element(button)
            .unwrap()
            .border
            .width;
        assert_eq!(after, 4.0);
    }

    #[test]
    fn pointer_click_emits_press_and_click_interactions() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let button = ui.append_child(ui.root(), TYPE_BUTTON);
        ui.set_label(button, "Launch");

        let mut text = TextEngine::new();

        let move_batch = ui.handle_pointer_event(
            &PointerEvent::Move(PointerUpdate {
                pointer: primary_pointer(),
                current: pointer_state(20.0, 20.0, 1),
                coalesced: Vec::new(),
                predicted: Vec::new(),
            }),
            &mut text,
        );
        assert!(
            move_batch
                .events()
                .contains(&Interaction::HoverEntered(button))
        );

        let down_batch = ui.handle_pointer_event(
            &PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                pointer: primary_pointer(),
                state: pointer_state(20.0, 20.0, 2),
            }),
            &mut text,
        );
        assert!(
            down_batch
                .events()
                .contains(&Interaction::PressStarted(button))
        );

        let up_batch = ui.handle_pointer_event(
            &PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                pointer: primary_pointer(),
                state: pointer_state(20.0, 20.0, 3),
            }),
            &mut text,
        );
        assert!(up_batch.events().contains(&Interaction::PressEnded(button)));
        assert!(up_batch.events().contains(&Interaction::Clicked(button)));
    }

    #[test]
    fn row_places_children_left_to_right() {
        let mut ui = Ui::new(default_theme());
        let mut text = TextEngine::new();
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append_child(ui.root(), TYPE_ROW);
        ui.set_local(row, ui.properties().padding, 0.0);
        ui.set_local(row, ui.properties().gap, 12.0);

        let left = ui.append_child(row, TYPE_PANEL);
        ui.set_local(left, ui.properties().width, 100.0);
        ui.set_local(left, ui.properties().height, 80.0);

        let right = ui.append_child(row, TYPE_PANEL);
        ui.set_local(right, ui.properties().height, 80.0);

        let scene = ui.rebuild(&mut text);
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
