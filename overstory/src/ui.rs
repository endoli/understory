// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Top-level retained Overstory UI model.

use alloc::{boxed::Box, vec::Vec};
use core::num::NonZeroU64;

use cursor_icon::CursorIcon;
use invalidation::ChannelSet;
use kurbo::{Point, Rect};
use peniko::Color;
use ui_events::pointer::{PointerButton, PointerEvent, PointerInfo};
use understory_box_tree::NodeFlags;
use understory_display::{TextAlign, TextEngine};
use understory_focus::{DefaultPolicy, FocusEntry, FocusPolicy, FocusSpace, Navigation, WrapMode};
use understory_property::{DependencyObjectExt, Property, PropertyRegistry};
use understory_style::{ClassId, IdSet, StyleCascade, Theme, ThemeBuilder, TypeTag};

use crate::{
    AppendSpec, BuiltInProperties, ButtonClass, DirtyChannels, Element, ElementId, Interaction,
    InteractionBatch, LayoutClass, RuntimeState, SceneSnapshot, TYPE_BUTTON, TYPE_COLUMN,
    TYPE_DIVIDER, TYPE_PANEL, TYPE_ROOT, TYPE_ROW, TYPE_SCROLL_VIEW, TYPE_SPACER, TYPE_SPINNER,
    TYPE_SPLITTER, TYPE_TEXT_BLOCK, TYPE_TEXT_INPUT, ThemeKeys, Widget, WidgetArena,
    built_in_styles::BuiltInStyles,
};

/// Retained Overstory UI state.
///
/// `Ui` owns:
/// - the retained element tree,
/// - built-in dependency properties,
/// - the active theme,
/// - built-in style defaults for Overstory's element/widget vocabulary,
/// - runtime interaction state.
///
/// Styling is resolved by combining:
/// 1. built-in Overstory cascades,
/// 2. per-element cascades set by the host,
/// 3. semantic theme resources from the active [`Theme`].
#[derive(Debug)]
pub struct Ui {
    registry: PropertyRegistry,
    props: BuiltInProperties,
    theme: Theme,
    text: Option<TextEngine>,
    elements: Vec<Element>,
    root: ElementId,
    runtime: RuntimeState,
    scene: Option<SceneSnapshot>,
    view_rect: Rect,
    dirty: ChannelSet,
    widget_arena: WidgetArena,
    built_in_styles: BuiltInStyles,
    timers: crate::TimerQueue,
    /// Current monotonic time in nanoseconds, set by the host before
    /// each event cycle via `set_now`.
    now: u64,
}

impl Ui {
    /// Appends one typed element/widget spec under `parent`.
    ///
    /// This is the preferred authoring seam for built-in Overstory elements
    /// and widgets. Lower-level retained tree APIs like [`Self::append_child`]
    /// and [`Self::set_local`] remain available as escape hatches.
    pub fn append<S: AppendSpec>(&mut self, parent: ElementId, spec: S) -> ElementId {
        spec.append_to(self, parent)
    }

    /// Appends one custom widget with an explicit type tag.
    ///
    /// Prefer [`Self::append`] for built-in Overstory specs. This is the
    /// typed lower-level seam for embedders that define their own widget types.
    pub fn append_widget<W: Widget + 'static>(
        &mut self,
        parent: ElementId,
        type_tag: TypeTag,
        widget: W,
    ) -> ElementId {
        self.append_child_with(parent, type_tag, Some(Box::new(widget)))
    }

    /// Creates a new retained UI with a single root element.
    ///
    /// The returned UI also initializes Overstory's built-in style defaults,
    /// which map built-in element/widget states onto semantic [`ThemeKeys`].
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        let mut registry = PropertyRegistry::new();
        let props = BuiltInProperties::register(&mut registry);
        let built_in_styles = BuiltInStyles::new(&props);
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
            text: Some(TextEngine::new()),
            elements,
            root,
            runtime: RuntimeState::new(),
            scene: None,
            view_rect: Rect::ZERO,
            dirty: DirtyChannels::STRUCTURE.into_set()
                | DirtyChannels::LAYOUT.into_set()
                | DirtyChannels::PAINT.into_set(),
            widget_arena: WidgetArena::new(),
            built_in_styles,
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

    /// Returns the current cursor hint from the active hover or press target.
    #[must_use]
    pub fn cursor_icon(&self) -> Option<CursorIcon> {
        let target = self
            .runtime
            .captured_target
            .or(self.runtime.pressed_target)
            .or_else(|| self.runtime.hover.current_path().last().copied())?;
        self.cursor_icon_for(target)
    }

    /// Returns the currently focused element, if any.
    #[must_use]
    pub fn focused_element(&self) -> Option<ElementId> {
        self.runtime.focused
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
    ///
    /// This is a low-level retained-tree escape hatch. Prefer [`Self::append`]
    /// or [`Self::append_widget`] when you want authored widget/container
    /// construction instead of manual tree surgery.
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

    pub(crate) fn append_container_with_widget(
        &mut self,
        parent: ElementId,
        type_tag: TypeTag,
        horizontal: bool,
        widget: Box<dyn Widget>,
    ) -> ElementId {
        let id = self.append_child_with(parent, type_tag, Some(widget));
        if let Some(element) = self.elements.get_mut(id.index()) {
            element.is_container = true;
            element.horizontal = horizontal;
        }
        id
    }

    /// Appends a child element with a built-in element type.
    ///
    /// This is a convenience wrapper that creates the appropriate widget and
    /// sets structural flags based on the type tag. Prefer [`Self::append`]
    /// for typed built-in authoring.
    pub fn append_child(&mut self, parent: ElementId, type_tag: TypeTag) -> ElementId {
        match type_tag {
            TYPE_ROOT | TYPE_PANEL | TYPE_COLUMN => self.append_container(parent, type_tag, false),
            TYPE_ROW => self.append_container(parent, type_tag, true),
            TYPE_SCROLL_VIEW => {
                let widget = crate::widgets::ScrollView::new();
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
                Some(Box::new(crate::widgets::Button::new())),
            ),
            TYPE_DIVIDER => self.append_child_with(
                parent,
                type_tag,
                Some(Box::new(crate::widgets::Divider::default())),
            ),
            TYPE_SPINNER => self.append_child_with(
                parent,
                type_tag,
                Some(Box::new(crate::widgets::Spinner::default())),
            ),
            TYPE_TEXT_BLOCK => self.append_child_with(
                parent,
                type_tag,
                Some(Box::new(crate::widgets::TextBlock::new())),
            ),
            TYPE_TEXT_INPUT => self.append_child_with(
                parent,
                type_tag,
                Some(Box::new(crate::widgets::TextInput::new(16.0))),
            ),
            TYPE_SPLITTER => self.append_child_with(
                parent,
                type_tag,
                Some(Box::new(crate::widgets::Splitter::default())),
            ),
            TYPE_SPACER => self.append_child_with(parent, type_tag, None),
            _ => self.append_child_with(parent, type_tag, None),
        }
    }

    /// Sets the human-readable display name for an element.
    ///
    /// This does not mutate widget-owned text content.
    pub fn set_label(&mut self, id: ElementId, label: impl Into<Box<str>>) {
        if let Some(element) = self.elements.get_mut(id.index()) {
            element.display_name = Some(label.into());
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Returns a short human-readable name for one element.
    ///
    /// Prefer this over reading [`crate::Element::label`] directly when you
    /// want something suitable for inspectors or debug UIs.
    #[must_use]
    pub fn display_name(&self, id: ElementId) -> Option<&str> {
        let element = self.elements.get(id.index())?;
        element
            .widget
            .and_then(|handle| self.widget_arena.get(handle))
            .and_then(crate::widget::widget_text)
            .or_else(|| element.display_name())
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
        if let Some(w) = self.widget_mut::<crate::widgets::ScrollView>(id) {
            w.set_scroll_offset(offset);
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Returns the measured content height of a `ScrollView` element.
    #[must_use]
    pub fn content_height(&self, id: ElementId) -> f64 {
        self.widget::<crate::widgets::ScrollView>(id)
            .map_or(0.0, |w| w.content_height())
    }

    /// Returns the viewport height of a `ScrollView` element from last layout.
    #[must_use]
    pub fn viewport_height(&self, id: ElementId) -> f64 {
        self.widget::<crate::widgets::ScrollView>(id)
            .map_or(0.0, |w| w.viewport_height())
    }

    /// Adjusts the scroll offset by a delta on a `ScrollView` element.
    pub fn scroll_by(&mut self, id: ElementId, delta: f64) {
        if let Some(w) = self.widget_mut::<crate::widgets::ScrollView>(id) {
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
        self.widget::<crate::widgets::ScrollView>(id)
            .map_or(0.0, |w| w.scroll_offset())
    }

    /// Sets keyboard focus to an element.
    /// Sets keyboard focus to an element. Uses the current monotonic time
    /// (set via `set_now`) to start cursor blink timers.
    pub fn set_focus(&mut self, id: ElementId) {
        self.set_focus_visible(id, true);
    }

    fn set_focus_visible(&mut self, id: ElementId, focus_visible: bool) {
        let now = self.now;
        if self.runtime.focused == Some(id) {
            if let Some(element) = self.elements.get_mut(id.index()) {
                element.pseudos.focus_visible = focus_visible;
            }
            self.mark_dirty(DirtyChannels::PAINT.into_set());
            return;
        }
        if !self
            .elements
            .get(id.index())
            .is_some_and(|element| element.get_effective_local(self.props.enabled, &self.registry))
        {
            return;
        }
        // Stop blink on previously focused element.
        if let Some(prev) = self.runtime.focused.take() {
            if let Some(element) = self.elements.get_mut(prev.index()) {
                element.pseudos.focused = false;
                element.pseudos.focus_visible = false;
                if self.runtime.pressed_target != Some(prev) {
                    element.pseudos.pressed = false;
                }
            }
            if let Some(handle) = self.elements.get(prev.index()).and_then(|e| e.widget)
                && let Some(w) = self.widget_arena.get_mut(handle)
                && let Some(ti) = w.as_any_mut().downcast_mut::<crate::widgets::TextInput>()
            {
                ti.stop_blink(&mut self.timers);
            }
        }
        self.runtime.focused = Some(id);
        if let Some(element) = self.elements.get_mut(id.index()) {
            element.pseudos.focused = true;
            element.pseudos.focus_visible = focus_visible;
        }
        // Start blink on newly focused TextInput.
        if let Some(handle) = self.elements.get(id.index()).and_then(|e| e.widget)
            && let Some(w) = self.widget_arena.get_mut(handle)
            && let Some(ti) = w.as_any_mut().downcast_mut::<crate::widgets::TextInput>()
        {
            ti.start_blink(&mut self.timers, id, now);
        }
        self.mark_dirty(DirtyChannels::PAINT.into_set());
    }

    /// Returns the current text buffer for a `TextInput` element.
    #[must_use]
    pub fn text_buffer(&self, id: ElementId) -> &str {
        self.widget::<crate::widgets::TextInput>(id)
            .map_or("", |w| w.text())
    }

    /// Replaces the current text buffer for a `TextInput` element.
    pub fn set_text_buffer(&mut self, id: ElementId, text: &str) {
        self.with_text_engine(|ui, engine| ui.set_text_buffer_with(id, text, engine));
    }

    /// Clears the text buffer for a `TextInput` element.
    pub fn clear_text_buffer(&mut self, id: ElementId) {
        self.with_text_engine(|ui, text| ui.clear_text_buffer_with(id, text));
    }

    /// Sets whether a `Dropdown` surface is open.
    pub fn set_dropdown_open(&mut self, id: ElementId, open: bool) {
        if let Some(dropdown) = self.widget_mut::<crate::widgets::Dropdown>(id) {
            dropdown.set_open(open);
        }
        self.set_local(id, self.props.visible, open);
        self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
    }

    /// Sets the desired root-space origin for a `Dropdown` surface.
    pub fn set_dropdown_position(&mut self, id: ElementId, x: f64, y: f64) {
        if let Some(dropdown) = self.widget_mut::<crate::widgets::Dropdown>(id) {
            dropdown.set_position(Point::new(x, y));
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Starts the animation timer for a `Spinner` element.
    pub fn start_spinner(&mut self, id: ElementId) {
        let now = self.now;
        let mut timers = core::mem::take(&mut self.timers);
        if let Some(spinner) = self.widget_mut::<crate::widgets::Spinner>(id) {
            spinner.start(&mut timers, id, now);
            self.mark_dirty(DirtyChannels::PAINT.into_set());
        }
        self.timers = timers;
    }

    /// Stops the animation timer for a `Spinner` element.
    pub fn stop_spinner(&mut self, id: ElementId) {
        let mut timers = core::mem::take(&mut self.timers);
        if let Some(spinner) = self.widget_mut::<crate::widgets::Spinner>(id) {
            spinner.stop(&mut timers);
            self.mark_dirty(DirtyChannels::PAINT.into_set());
        }
        self.timers = timers;
    }

    fn clear_text_buffer_with(&mut self, id: ElementId, text: &mut TextEngine) {
        if let Some(w) = self.widget_mut::<crate::widgets::TextInput>(id) {
            w.clear(text);
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    fn set_text_buffer_with(&mut self, id: ElementId, value: &str, text: &mut TextEngine) {
        if let Some(w) = self.widget_mut::<crate::widgets::TextInput>(id) {
            w.set_text(value);
            w.refresh_layout(text);
            self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
        }
    }

    /// Handles one keyboard event from `ui-events`.
    ///
    /// Delegates to the focused element's widget first. If the focused widget
    /// does not consume the event, Overstory applies built-in focus traversal
    /// for `Tab` / `Shift+Tab` and directional arrows using `understory_focus`.
    pub fn handle_keyboard_event(
        &mut self,
        event: &ui_events::keyboard::KeyboardEvent,
    ) -> InteractionBatch {
        self.with_text_engine(|ui, text| ui.handle_keyboard_event_with(event, text))
    }

    fn handle_keyboard_event_with(
        &mut self,
        event: &ui_events::keyboard::KeyboardEvent,
        text: &mut TextEngine,
    ) -> InteractionBatch {
        let mut batch = InteractionBatch::default();
        self.rebuild_with(text);
        if let Some(focused) = self.runtime.focused
            && let Some(scene) = self.scene.as_ref()
            && scene
                .node_for(focused)
                .and_then(|node| scene.box_tree().flags(node))
                .is_none_or(|flags| !flags.contains(NodeFlags::FOCUSABLE))
        {
            self.runtime.focused = None;
            if let Some(element) = self.elements.get_mut(focused.index()) {
                element.pseudos.focused = false;
                element.pseudos.focus_visible = false;
                element.pseudos.pressed = false;
            }
        }
        let focused = self.runtime.focused;
        let mut handled = false;
        if let Some(focused) = focused
            && let Some(handle) = self.elements.get(focused.index()).and_then(|e| e.widget)
            && let Some(scene) = self.scene.as_ref()
        {
            let resolved_slice = scene.resolved();
            let mut ctx = crate::KeyboardEventCtx::new(
                focused,
                &mut self.elements,
                &self.registry,
                &self.props,
                &mut self.dirty,
                resolved_slice,
            );
            let Some(widget) = self.widget_arena.get_mut(handle) else {
                return batch;
            };
            handled = widget.keyboard_event(focused, event, &mut ctx, text, &mut batch);
            if handled {
                self.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());
            }
        }
        if !handled
            && let Some(navigation) = navigation_for_key_event(event)
            && let Some(next) = self.next_focus_target(navigation)
        {
            self.set_focus_visible(next, true);
            batch.push(Interaction::FocusChanged(next));
        }
        batch
    }

    fn next_focus_target(&self, navigation: Navigation) -> Option<ElementId> {
        let scene = self.scene.as_ref()?;
        let (entries, autofocus) = self.focus_entries(scene);
        if entries.is_empty() {
            return None;
        }
        let space = FocusSpace {
            nodes: &entries,
            autofocus,
        };
        let policy = DefaultPolicy {
            wrap: WrapMode::Scope,
        };
        match self.runtime.focused {
            Some(origin) => policy
                .next(origin, navigation, &space)
                .or_else(|| policy.initial(navigation, &space)),
            None => policy.initial(navigation, &space),
        }
    }

    fn focus_entries(
        &self,
        scene: &SceneSnapshot,
    ) -> (Vec<FocusEntry<ElementId>>, Option<ElementId>) {
        let mut entries = Vec::new();
        let mut autofocus = None;
        for resolved in scene.resolved() {
            let Some(node) = scene.node_for(resolved.id) else {
                continue;
            };
            let Some(flags) = scene.box_tree().flags(node) else {
                continue;
            };
            if !flags.contains(NodeFlags::FOCUSABLE | NodeFlags::VISIBLE) {
                continue;
            }
            let Some(element) = self.elements.get(resolved.id.index()) else {
                continue;
            };
            let order = element
                .store
                .get_effective_local(self.props.focus_order, &self.registry);
            let group = element
                .store
                .get_effective_local(self.props.focus_group, &self.registry);
            let entry_autofocus = element
                .store
                .get_effective_local(self.props.autofocus, &self.registry);
            if entry_autofocus && autofocus.is_none() {
                autofocus = Some(resolved.id);
            }
            entries.push(FocusEntry {
                id: resolved.id,
                rect: resolved.rect,
                order,
                group,
                enabled: true,
                scope_depth: u8::try_from(resolved.depth).unwrap_or(u8::MAX),
            });
        }
        (entries, autofocus)
    }

    /// Updates tooltip visibility and positioning based on current hover state.
    ///
    /// Tooltips become visible when their trigger element is hovered and
    /// are positioned below the trigger's resolved rect.
    pub fn update_tooltips(&mut self) {
        self.with_text_engine(|ui, text| ui.update_tooltips_with(text));
    }

    fn update_tooltips_with(&mut self, text: &mut TextEngine) {
        // Collect tooltip info: (tooltip_id, trigger_id).
        let tooltips: Vec<(ElementId, ElementId)> = self
            .elements
            .iter()
            .filter_map(|el| {
                let handle = el.widget?;
                let widget = self.widget_arena.get(handle)?;
                let tw = widget.as_any().downcast_ref::<crate::widgets::Tooltip>()?;
                Some((el.id, tw.trigger()))
            })
            .collect();

        if tooltips.is_empty() {
            return;
        }

        // Rebuild to get current hover state.
        self.rebuild_with(text);
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
            if let Some(tw) = self.widget_mut::<crate::widgets::Tooltip>(*tooltip_id) {
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
    pub fn refresh_editors(&mut self) {
        self.with_text_engine(|ui, text| ui.refresh_editors_with(text));
    }

    fn refresh_editors_with(&mut self, text: &mut TextEngine) {
        for (_handle, widget) in self.widget_arena.iter_mut() {
            widget.refresh_layout(text);
        }
    }

    /// Rebuilds the resolved scene if needed and returns the current snapshot.
    pub fn rebuild(&mut self) -> &SceneSnapshot {
        self.with_text_engine(|ui, text| ui.rebuild_with(text));
        self.scene.as_ref().expect("scene just rebuilt")
    }

    fn rebuild_with(&mut self, text: &mut TextEngine) {
        if self.scene.is_none() || !self.dirty.is_empty() {
            let (snapshot, scroll_metrics) = SceneSnapshot::build(
                &self.elements,
                self.root,
                self.view_rect,
                &self.registry,
                &self.props,
                &self.theme,
                &self.built_in_styles,
                &self.widget_arena,
                text,
            );
            let mut needs_rebuild = false;
            for (id, content_h, viewport_h) in &scroll_metrics {
                if let Some(w) = self.widget_mut::<crate::widgets::ScrollView>(*id) {
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
                    &self.built_in_styles,
                    &self.widget_arena,
                    text,
                );
                self.scene = Some(snapshot);
            } else {
                self.scene = Some(snapshot);
            }
            self.dirty = ChannelSet::empty();
        }
    }

    /// Returns the current resolved scene, rebuilding first if necessary.
    pub fn scene(&mut self) -> &SceneSnapshot {
        self.rebuild()
    }

    /// Rebuilds the scene if needed and returns a display tree with widget
    /// rendering applied.
    pub fn display_tree(&mut self) -> (understory_display::DisplayTree, Rect) {
        self.rebuild();
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
    pub fn surface_plan(&mut self) -> crate::SurfacePlan {
        self.rebuild();
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
                let bounds = self.overlay_surface_bounds(*id, layout_rect);
                let anchor = self.overlay_surface_anchor(*id);
                plan.push(crate::SurfaceEntry {
                    element_id: *id,
                    role: *role,
                    transform: kurbo::Affine::IDENTITY,
                    bounds,
                    clip: None,
                    opacity: 1.0,
                    blend: crate::BlendModeHint::Normal,
                    anchor,
                    content: crate::SurfaceContent::Display(Box::new(tree)),
                });
            }
        }

        plan
    }

    fn overlay_surface_bounds(&self, id: ElementId, fallback: Rect) -> Rect {
        if let Some(tooltip) = self.widget::<crate::widgets::Tooltip>(id)
            && let Some(position) = tooltip.position()
        {
            return Rect::from_origin_size(position, fallback.size());
        }
        if let Some(dropdown) = self.widget::<crate::widgets::Dropdown>(id)
            && let Some(position) = dropdown.position()
        {
            return Rect::from_origin_size(position, fallback.size());
        }
        fallback
    }

    fn overlay_surface_anchor(&self, id: ElementId) -> Option<crate::SurfaceAnchor> {
        if let Some(tooltip) = self.widget::<crate::widgets::Tooltip>(id) {
            return self.anchor_for(tooltip.trigger(), crate::AnchorKind::Tooltip);
        }
        if let Some(dropdown) = self.widget::<crate::widgets::Dropdown>(id) {
            return self.anchor_for(dropdown.anchor(), crate::AnchorKind::Dropdown);
        }
        None
    }

    fn anchor_for(
        &self,
        owner: ElementId,
        kind: crate::AnchorKind,
    ) -> Option<crate::SurfaceAnchor> {
        let rect_in_root = self.scene.as_ref()?.resolved_element(owner)?.rect;
        Some(crate::SurfaceAnchor {
            owner,
            kind,
            rect_in_root,
        })
    }

    /// Handles one pointer event from `ui-events`.
    pub fn handle_pointer_event(&mut self, event: &PointerEvent) -> InteractionBatch {
        self.with_text_engine(|ui, text| ui.handle_pointer_event_with(event, text))
    }

    fn handle_pointer_event_with(
        &mut self,
        event: &PointerEvent,
        text: &mut TextEngine,
    ) -> InteractionBatch {
        let mut batch = InteractionBatch::default();
        self.rebuild_with(text);

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
                if let Some(target) = self.pointer_dispatch_target() {
                    let _ = self.dispatch_widget_pointer_event(target, event, text, &mut batch);
                }
            }
            PointerEvent::Down(button) if is_primary_button(button.button) => {
                let point = point_from_state(&button.state);
                self.update_hover(point, &mut batch, text);
                self.rebuild_with(text);
                if let Some(target) = self.scene.as_ref().and_then(|scene| scene.top_hit(point)) {
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
                    let _ = self.dispatch_widget_pointer_event(target, event, text, &mut batch);
                }
            }
            PointerEvent::Up(button) if is_primary_button(button.button) => {
                let point = point_from_state(&button.state);
                self.update_hover(point, &mut batch, text);
                self.rebuild_with(text);
                let current_target = self.scene.as_ref().and_then(|scene| scene.top_hit(point));
                if let Some(target) = self.pointer_dispatch_target() {
                    let _ = self.dispatch_widget_pointer_event(target, event, text, &mut batch);
                }
                self.runtime.captured_target = None;
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
                            self.rebuild_with(text);
                            let is_focusable = self
                                .scene
                                .as_ref()
                                .expect("scene rebuilt")
                                .node_for(id)
                                .and_then(|node| {
                                    self.scene
                                        .as_ref()
                                        .expect("scene rebuilt")
                                        .box_tree()
                                        .flags(node)
                                })
                                .is_some_and(|f| f.contains(NodeFlags::FOCUSABLE));
                            if is_focusable {
                                self.set_focus_visible(id, false);
                                batch.push(Interaction::FocusChanged(id));
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
                if let Some(target) = self.pointer_dispatch_target() {
                    let _ = self.dispatch_widget_pointer_event(target, event, text, &mut batch);
                }
                self.runtime.captured_target = None;
                self.set_pressed_target(None, &mut batch);
                self.clear_hover(&mut batch);
            }
            PointerEvent::Scroll(scroll) => {
                let point = point_from_state(&scroll.state);
                let dy = scroll_delta_y(scroll.delta);
                if dy != 0.0 && {
                    self.rebuild_with(text);
                    self.scene.as_ref().and_then(|scene| scene.hit_path(point))
                }
                .is_some()
                {
                    let path = self
                        .scene
                        .as_ref()
                        .and_then(|scene| scene.hit_path(point))
                        .expect("path checked above");
                    for &ancestor in path.iter().rev() {
                        if self
                            .widget::<crate::widgets::ScrollView>(ancestor)
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
            self.rebuild_with(text);
        }
        batch
    }

    fn dispatch_widget_pointer_event(
        &mut self,
        id: ElementId,
        event: &PointerEvent,
        text: &mut TextEngine,
        batch: &mut InteractionBatch,
    ) -> bool {
        let Some(scene) = self.scene.as_ref() else {
            return false;
        };
        let Some(resolved) = scene.resolved_element(id).cloned() else {
            return false;
        };
        let Some(handle) = self
            .elements
            .get(id.index())
            .and_then(|element| element.widget)
        else {
            return false;
        };
        let resolved_slice = scene.resolved();
        let mut ctx = crate::PointerEventCtx::new(
            id,
            &mut self.elements,
            &self.registry,
            &self.props,
            &mut self.dirty,
            &mut self.runtime.captured_target,
            resolved_slice,
        );
        let Some(widget) = self.widget_arena.get_mut(handle) else {
            return false;
        };
        widget.handle_pointer_event(id, event, &resolved, &mut ctx, text, batch)
    }

    fn cursor_icon_for(&self, id: ElementId) -> Option<CursorIcon> {
        let mut current = Some(id);
        while let Some(id) = current {
            let element = self.elements.get(id.index())?;
            if let Some(handle) = element.widget
                && let Some(widget) = self.widget_arena.get(handle)
                && let Some(icon) = widget.cursor_icon(element)
            {
                return Some(icon);
            }
            current = element.parent();
        }
        None
    }

    fn mark_dirty(&mut self, channels: ChannelSet) {
        self.dirty |= channels;
    }

    fn pointer_dispatch_target(&self) -> Option<ElementId> {
        self.runtime.captured_target.or(self.runtime.pressed_target)
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
        self.rebuild_with(text);
        let path = self
            .scene
            .as_ref()
            .and_then(|scene| scene.hit_path(point))
            .unwrap_or_default();
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

    fn with_text_engine<R>(&mut self, f: impl FnOnce(&mut Self, &mut TextEngine) -> R) -> R {
        let mut text = self
            .text
            .take()
            .expect("Ui text engine should always be present");
        let result = f(self, &mut text);
        self.text = Some(text);
        result
    }
}

/// Default theme used by Overstory examples and tests.
///
/// This theme fills the semantic token vocabulary in [`ThemeKeys`]. Widget and
/// interaction-state-specific behavior is not encoded here; built-in cascades
/// decide when to use tokens such as `CONTROL_BACKGROUND_EMPHASIZED` or
/// `ACCENT_BACKGROUND_STRONG`.
#[must_use]
pub fn default_theme() -> Theme {
    ThemeBuilder::new()
        .set(
            ThemeKeys::APP_BACKGROUND,
            Color::from_rgba8(242, 239, 232, 255),
        )
        .set(
            ThemeKeys::SURFACE_BACKGROUND,
            Color::from_rgba8(255, 252, 246, 255),
        )
        .set(
            ThemeKeys::SURFACE_MUTED_BACKGROUND,
            Color::from_rgba8(226, 222, 213, 255),
        )
        .set(
            ThemeKeys::CONTROL_BACKGROUND,
            Color::from_rgba8(238, 233, 225, 255),
        )
        .set(
            ThemeKeys::CONTROL_BACKGROUND_EMPHASIZED,
            Color::from_rgba8(230, 225, 216, 255),
        )
        .set(
            ThemeKeys::CONTROL_BACKGROUND_STRONG,
            Color::from_rgba8(214, 208, 198, 255),
        )
        .set(
            ThemeKeys::ACCENT_BACKGROUND,
            Color::from_rgba8(24, 92, 72, 255),
        )
        .set(
            ThemeKeys::ACCENT_BACKGROUND_EMPHASIZED,
            Color::from_rgba8(31, 109, 86, 255),
        )
        .set(
            ThemeKeys::ACCENT_BACKGROUND_STRONG,
            Color::from_rgba8(18, 72, 57, 255),
        )
        .set(ThemeKeys::ACCENT_FOREGROUND, Color::WHITE)
        .set(ThemeKeys::FOREGROUND, Color::from_rgba8(33, 37, 41, 255))
        .set(
            ThemeKeys::BORDER_COLOR,
            Color::from_rgba8(143, 133, 122, 255),
        )
        .set(
            ThemeKeys::FOCUS_RING_COLOR,
            Color::from_rgba8(199, 122, 36, 255),
        )
        .set(ThemeKeys::CORNER_RADIUS, 10.0_f64)
        .set(ThemeKeys::PADDING, 16.0_f64)
        .set(ThemeKeys::GAP, 12.0_f64)
        .set(ThemeKeys::BUTTON_HEIGHT, 44.0_f64)
        .set(ThemeKeys::FONT_SIZE, 16.0_f64)
        .set(ThemeKeys::LABEL_PADDING, 12.0_f64)
        .set(ThemeKeys::FONT_FAMILY, Box::<str>::from("sans-serif"))
        .set(ThemeKeys::TEXT_ALIGN, TextAlign::Start)
        .set(
            ThemeKeys::DIVIDER_BACKGROUND_EMPHASIZED,
            Color::from_rgba8(24, 92, 72, 28),
        )
        .set(
            ThemeKeys::DIVIDER_BACKGROUND_STRONG,
            Color::from_rgba8(24, 92, 72, 56),
        )
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

fn navigation_for_key_event(event: &ui_events::keyboard::KeyboardEvent) -> Option<Navigation> {
    if !event.state.is_down() {
        return None;
    }
    match event.key {
        ui_events::keyboard::Key::Named(ui_events::keyboard::NamedKey::Tab) => {
            if event
                .modifiers
                .contains(ui_events::keyboard::Modifiers::SHIFT)
            {
                Some(Navigation::Prev)
            } else {
                Some(Navigation::Next)
            }
        }
        ui_events::keyboard::Key::Named(ui_events::keyboard::NamedKey::ArrowUp) => {
            Some(Navigation::Up)
        }
        ui_events::keyboard::Key::Named(ui_events::keyboard::NamedKey::ArrowDown) => {
            Some(Navigation::Down)
        }
        ui_events::keyboard::Key::Named(ui_events::keyboard::NamedKey::ArrowLeft) => {
            Some(Navigation::Left)
        }
        ui_events::keyboard::Key::Named(ui_events::keyboard::NamedKey::ArrowRight) => {
            Some(Navigation::Right)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PSEUDO_HOVER;
    use cursor_icon::CursorIcon;
    use ui_events::keyboard::{Code, Key, KeyState, KeyboardEvent, Location, Modifiers, NamedKey};
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

        let column = ui.append(ui.root(), crate::Column::new().padding(0.0).gap(8.0));

        let first = ui.append(column, crate::Button::new().height(20.0));
        let second = ui.append(column, crate::Button::new().height(30.0));

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

        let button = ui.append(ui.root(), crate::Button::new());
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
    fn append_builds_typed_row_and_button_specs() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(ui.root(), crate::Row::new().padding(0.0).gap(12.0));
        let button = ui.append(
            row,
            crate::Button::new()
                .with_text("Search")
                .primary()
                .height(42.0),
        );

        let scene = ui.rebuild();
        let row_rect = scene.resolved_element(row).expect("row resolved").rect;
        let button_rect = scene
            .resolved_element(button)
            .expect("button resolved")
            .rect;
        let resolved = scene
            .resolved_element(button)
            .expect("button resolved element");

        assert_eq!(row_rect.width(), 320.0);
        assert_eq!(button_rect.height(), 42.0);
        assert_eq!(resolved.text.as_deref(), Some("Search"));
    }

    #[test]
    fn pointer_click_emits_press_and_click_interactions() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let button = ui.append(ui.root(), crate::Button::new().with_text("Launch"));

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
    fn splitter_hover_exposes_resize_cursor() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 640.0, 240.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(
            ui.root(),
            crate::Row::new()
                .padding(0.0)
                .gap(0.0)
                .width(640.0)
                .height(240.0),
        );

        let left = ui.append(row, crate::Panel::new().width(180.0).height(240.0));

        let splitter = ui.append(
            row,
            crate::Splitter::vertical(left).width(14.0).height(240.0),
        );

        let _right = ui.append(row, crate::Panel::new().fill().height(240.0));

        let scene = ui.rebuild();
        let splitter_rect = scene.resolved_element(splitter).unwrap().rect;
        let center = splitter_rect.center();

        let _ = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(center.x, center.y, 1),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));

        assert_eq!(ui.cursor_icon(), Some(CursorIcon::ColResize));
    }

    #[test]
    fn pressed_splitter_keeps_resize_cursor() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 640.0, 240.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(
            ui.root(),
            crate::Row::new()
                .padding(0.0)
                .gap(0.0)
                .width(640.0)
                .height(240.0),
        );

        let left = ui.append(row, crate::Panel::new().width(180.0).height(240.0));

        let splitter = ui.append(
            row,
            crate::Splitter::vertical(left).width(14.0).height(240.0),
        );

        let _right = ui.append(row, crate::Panel::new().fill().height(240.0));

        let scene = ui.rebuild();
        let splitter_rect = scene.resolved_element(splitter).unwrap().rect;
        let center = splitter_rect.center();

        let _ = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(center.x, center.y, 1),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));
        let _ = ui.handle_pointer_event(&PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(center.x, center.y, 2),
        }));
        let _ = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(center.x + 20.0, center.y, 3),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));

        assert_eq!(ui.cursor_icon(), Some(CursorIcon::ColResize));
    }

    #[test]
    fn splitter_drag_captures_pointer_until_release() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 640.0, 240.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(
            ui.root(),
            crate::Row::new()
                .padding(0.0)
                .gap(0.0)
                .width(640.0)
                .height(240.0),
        );

        let left = ui.append(row, crate::Panel::new().width(180.0).height(240.0));

        let splitter = ui.append(
            row,
            crate::Splitter::vertical(left).width(14.0).height(240.0),
        );

        let _right = ui.append(row, crate::Panel::new().fill().height(240.0));

        let scene = ui.rebuild();
        let splitter_rect = scene.resolved_element(splitter).unwrap().rect;
        let center = splitter_rect.center();

        let _ = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(center.x, center.y, 1),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));
        let _ = ui.handle_pointer_event(&PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(center.x, center.y, 2),
        }));
        assert_eq!(ui.runtime.captured_target, Some(splitter));

        let _ = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(center.x + 32.0, center.y, 3),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));
        assert_eq!(ui.runtime.captured_target, Some(splitter));

        let _ = ui.handle_pointer_event(&PointerEvent::Up(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(center.x + 32.0, center.y, 4),
        }));
        assert_eq!(ui.runtime.captured_target, None);
    }

    #[test]
    fn clicking_splitter_sets_keyboard_focus() {
        let mut ui = Ui::new(default_theme());
        let (left, splitter, _right, center) = splitter_test_row(&mut ui);

        let _ = left;
        let _ = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(center.x, center.y, 1),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));
        let _ = ui.handle_pointer_event(&PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(center.x, center.y, 2),
        }));
        let up_batch = ui.handle_pointer_event(&PointerEvent::Up(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(center.x, center.y, 3),
        }));

        assert_eq!(ui.runtime.focused, Some(splitter));
        assert!(
            up_batch
                .events()
                .contains(&Interaction::FocusChanged(splitter))
        );
        let scene = ui.rebuild();
        assert!(
            !scene
                .resolved_element(splitter)
                .expect("splitter resolved")
                .focus_visible
        );
    }

    #[test]
    fn focus_visible_splitter_uses_focus_visible_style() {
        let mut ui = Ui::new(default_theme());
        let (_left, splitter, _right, _center) = splitter_test_row(&mut ui);
        ui.set_focus(splitter);

        let theme_focused = *ui
            .theme()
            .get(ThemeKeys::DIVIDER_BACKGROUND_EMPHASIZED)
            .expect("focused divider background in theme");
        let scene = ui.rebuild();
        let resolved = scene.resolved_element(splitter).expect("splitter resolved");
        assert_eq!(resolved.background, theme_focused);
    }

    #[test]
    fn focused_splitter_resizes_from_arrow_keys() {
        let mut ui = Ui::new(default_theme());
        let (left, _splitter, _right, center) = splitter_test_row(&mut ui);

        let _ = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(center.x, center.y, 1),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));
        let _ = ui.handle_pointer_event(&PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(center.x, center.y, 2),
        }));
        let _ = ui.handle_pointer_event(&PointerEvent::Up(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(center.x, center.y, 3),
        }));

        let right_arrow = KeyboardEvent {
            key: Key::Named(NamedKey::ArrowRight),
            code: Code::Unidentified,
            state: KeyState::Down,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            repeat: false,
            is_composing: false,
        };
        let batch = ui.handle_keyboard_event(&right_arrow);
        assert!(batch.is_empty());

        let scene = ui.rebuild();
        let left_rect = scene.resolved_element(left).expect("left resolved").rect;
        assert_eq!(left_rect.width(), 188.0);
    }

    #[test]
    fn splitter_drag_updates_leading_pane_width() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 640.0, 240.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(
            ui.root(),
            crate::Row::new()
                .padding(0.0)
                .gap(0.0)
                .width(640.0)
                .height(240.0),
        );

        let left = ui.append(row, crate::Panel::new().width(180.0).height(240.0));

        let splitter = ui.append(
            row,
            crate::Splitter::vertical(left)
                .with_min_primary(140.0)
                .with_min_secondary(220.0)
                .width(14.0)
                .height(240.0),
        );

        let right = ui.append(row, crate::Panel::new().fill().height(240.0));

        let scene = ui.rebuild();
        let splitter_rect = scene.resolved_element(splitter).unwrap().rect;
        let start = splitter_rect.center();

        let _ = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(start.x, start.y, 1),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));
        let _ = ui.handle_pointer_event(&PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(start.x, start.y, 2),
        }));
        let _ = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(start.x + 60.0, start.y, 3),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));
        let _ = ui.handle_pointer_event(&PointerEvent::Up(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(start.x + 60.0, start.y, 4),
        }));

        let scene = ui.rebuild();
        let left_rect = scene.resolved_element(left).unwrap().rect;
        let splitter_rect = scene.resolved_element(splitter).unwrap().rect;
        let right_rect = scene.resolved_element(right).unwrap().rect;

        assert_eq!(left_rect.width(), 240.0);
        assert_eq!(splitter_rect.x0, left_rect.x1);
        assert_eq!(right_rect.x0, splitter_rect.x1);
        assert!(right_rect.width() >= 220.0);
    }

    fn splitter_test_row(ui: &mut Ui) -> (ElementId, ElementId, ElementId, Point) {
        ui.set_view_rect(Rect::new(0.0, 0.0, 640.0, 240.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(
            ui.root(),
            crate::Row::new()
                .padding(0.0)
                .gap(0.0)
                .width(640.0)
                .height(240.0),
        );

        let left = ui.append(row, crate::Panel::new().width(180.0).height(240.0));

        let splitter = ui.append(
            row,
            crate::Splitter::vertical(left)
                .with_min_primary(140.0)
                .with_min_secondary(220.0)
                .width(14.0)
                .height(240.0),
        );

        let right = ui.append(row, crate::Panel::new().fill().height(240.0));

        let scene = ui.rebuild();
        let center = scene
            .resolved_element(splitter)
            .expect("splitter resolved")
            .rect
            .center();
        (left, splitter, right, center)
    }

    #[test]
    fn row_places_children_left_to_right() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(ui.root(), crate::Row::new().padding(0.0).gap(12.0));

        let left = ui.append(row, crate::Panel::new().width(100.0).height(80.0));

        let right = ui.append(row, crate::Panel::new().fill().height(80.0));

        let scene = ui.rebuild();
        let left_rect = scene.resolved_element(left).unwrap().rect;
        let right_rect = scene.resolved_element(right).unwrap().rect;

        assert_eq!(left_rect, Rect::new(0.0, 0.0, 100.0, 80.0));
        assert_eq!(right_rect, Rect::new(112.0, 0.0, 320.0, 80.0));
    }

    #[test]
    fn row_non_fill_buttons_use_intrinsic_widths() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 480.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(ui.root(), crate::Row::new().padding(0.0).gap(12.0));

        let short = ui.append(row, crate::Button::new().with_text("A"));

        let long = ui.append(
            row,
            crate::Button::new().with_text("A much longer button label"),
        );

        let scene = ui.rebuild();
        let short_rect = scene.resolved_element(short).expect("short resolved").rect;
        let long_rect = scene.resolved_element(long).expect("long resolved").rect;

        assert!(short_rect.width() < long_rect.width());
        assert!(short_rect.width() < 200.0);
        assert!(long_rect.x0 >= short_rect.x1 + 12.0);
        assert!(long_rect.x1 < 480.0);
    }

    #[test]
    fn row_non_fill_panel_uses_intrinsic_content_width() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 480.0, 160.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(ui.root(), crate::Row::new().padding(0.0).gap(12.0));

        let panel = ui.append(row, crate::Panel::new().padding(0.0));

        let button = ui.append(panel, crate::Button::new().with_text("Inspect"));

        let fill = ui.append(row, crate::Panel::new().fill());

        let scene = ui.rebuild();
        let panel_rect = scene.resolved_element(panel).expect("panel resolved").rect;
        let button_rect = scene
            .resolved_element(button)
            .expect("button resolved")
            .rect;
        let fill_rect = scene.resolved_element(fill).expect("fill resolved").rect;

        assert_eq!(panel_rect.width(), button_rect.width());
        assert!(panel_rect.width() < 200.0);
        assert_eq!(fill_rect.x0, panel_rect.x1 + 12.0);
        assert_eq!(fill_rect.x1, 480.0);
    }

    #[test]
    fn column_children_stretch_to_container_width() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 360.0, 180.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append(ui.root(), crate::Column::new().padding(0.0).gap(8.0));

        let panel = ui.append(column, crate::Panel::new().padding(0.0));

        let button = ui.append(panel, crate::Button::new().with_text("Inspect"));

        let scene = ui.rebuild();
        let column_rect = scene
            .resolved_element(column)
            .expect("column resolved")
            .rect;
        let panel_rect = scene.resolved_element(panel).expect("panel resolved").rect;
        let button_rect = scene
            .resolved_element(button)
            .expect("button resolved")
            .rect;

        assert_eq!(column_rect.width(), 360.0);
        assert_eq!(panel_rect.width(), 360.0);
        assert_eq!(button_rect.width(), 360.0);
    }

    #[test]
    fn row_children_stretch_to_container_height() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 360.0, 180.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(
            ui.root(),
            crate::Row::new().padding(0.0).gap(12.0).height(180.0),
        );

        let panel = ui.append(row, crate::Panel::new().padding(0.0));

        let button = ui.append(panel, crate::Button::new().with_text("Inspect"));

        let scene = ui.rebuild();
        let row_rect = scene.resolved_element(row).expect("row resolved").rect;
        let panel_rect = scene.resolved_element(panel).expect("panel resolved").rect;
        let button_rect = scene
            .resolved_element(button)
            .expect("button resolved")
            .rect;

        assert_eq!(row_rect.height(), 180.0);
        assert_eq!(panel_rect.height(), 180.0);
        assert!(button_rect.height() < panel_rect.height());
    }

    #[test]
    fn column_non_fill_children_use_intrinsic_height() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 360.0, 240.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append(
            ui.root(),
            crate::Column::new().padding(0.0).gap(8.0).height(240.0),
        );

        let panel = ui.append(column, crate::Panel::new().padding(0.0));

        let button = ui.append(panel, crate::Button::new().with_text("Inspect"));

        let fill = ui.append(column, crate::Panel::new().fill());

        let scene = ui.rebuild();
        let panel_rect = scene.resolved_element(panel).expect("panel resolved").rect;
        let button_rect = scene
            .resolved_element(button)
            .expect("button resolved")
            .rect;
        let fill_rect = scene.resolved_element(fill).expect("fill resolved").rect;

        assert_eq!(panel_rect.height(), button_rect.height());
        assert!(panel_rect.height() < 100.0);
        assert_eq!(fill_rect.y0, panel_rect.y1 + 8.0);
        assert_eq!(fill_rect.y1, 240.0);
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

    #[test]
    fn tab_moves_focus_between_buttons() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(ui.root(), crate::Row::new().padding(0.0).gap(12.0));
        let first = ui.append(row, crate::Button::new().with_text("First"));
        let second = ui.append(row, crate::Button::new().with_text("Second"));

        let tab = KeyboardEvent {
            key: Key::Named(NamedKey::Tab),
            code: Code::Unidentified,
            state: KeyState::Down,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            repeat: false,
            is_composing: false,
        };
        let shift_tab = KeyboardEvent {
            modifiers: Modifiers::SHIFT,
            ..tab.clone()
        };

        let batch = ui.handle_keyboard_event(&tab);
        assert_eq!(ui.runtime.focused, Some(first));
        assert!(batch.events().contains(&Interaction::FocusChanged(first)));

        let batch = ui.handle_keyboard_event(&tab);
        assert_eq!(ui.runtime.focused, Some(second));
        assert!(batch.events().contains(&Interaction::FocusChanged(second)));

        let batch = ui.handle_keyboard_event(&shift_tab);
        assert_eq!(ui.runtime.focused, Some(first));
        assert!(batch.events().contains(&Interaction::FocusChanged(first)));
    }

    #[test]
    fn tab_prefers_autofocus_candidate_when_unfocused() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(ui.root(), crate::Row::new().padding(0.0).gap(12.0));
        let first = ui.append(row, crate::Button::new().with_text("First"));
        let second = ui.append(row, crate::Button::new().with_text("Second"));
        ui.set_local(second, ui.properties().autofocus, true);

        let tab = KeyboardEvent {
            key: Key::Named(NamedKey::Tab),
            code: Code::Unidentified,
            state: KeyState::Down,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            repeat: false,
            is_composing: false,
        };

        let batch = ui.handle_keyboard_event(&tab);
        assert_eq!(ui.runtime.focused, Some(second));
        assert!(batch.events().contains(&Interaction::FocusChanged(second)));
        assert_ne!(ui.runtime.focused, Some(first));
    }

    #[test]
    fn focused_button_clicks_on_enter_keydown() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 120.0));

        let button = ui.append(ui.root(), crate::Button::new().with_text("Run"));
        ui.set_focus(button);

        let enter = KeyboardEvent {
            key: Key::Named(NamedKey::Enter),
            code: Code::Unidentified,
            state: KeyState::Down,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            repeat: false,
            is_composing: false,
        };

        let batch = ui.handle_keyboard_event(&enter);
        assert!(batch.events().contains(&Interaction::PressStarted(button)));
        assert!(batch.events().contains(&Interaction::PressEnded(button)));
        assert!(batch.events().contains(&Interaction::Clicked(button)));
    }

    #[test]
    fn focused_button_clicks_on_space_keyup() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 120.0));

        let button = ui.append(ui.root(), crate::Button::new().with_text("Run"));
        ui.set_focus(button);

        let space_down = KeyboardEvent {
            key: Key::Character(" ".into()),
            code: Code::Unidentified,
            state: KeyState::Down,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            repeat: false,
            is_composing: false,
        };
        let space_up = KeyboardEvent {
            state: KeyState::Up,
            ..space_down.clone()
        };

        let down_batch = ui.handle_keyboard_event(&space_down);
        assert!(
            down_batch
                .events()
                .contains(&Interaction::PressStarted(button))
        );
        assert!(!down_batch.events().contains(&Interaction::Clicked(button)));
        let pressed_scene = ui.rebuild();
        assert!(
            pressed_scene
                .resolved_element(button)
                .expect("button resolved")
                .pressed
        );

        let up_batch = ui.handle_keyboard_event(&space_up);
        assert!(up_batch.events().contains(&Interaction::PressEnded(button)));
        assert!(up_batch.events().contains(&Interaction::Clicked(button)));
        let released_scene = ui.rebuild();
        assert!(
            !released_scene
                .resolved_element(button)
                .expect("button resolved")
                .pressed
        );
    }

    #[test]
    fn focused_button_clears_keyboard_pressed_state_on_focus_change() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 120.0));

        let row = ui.append(ui.root(), crate::Row::new().padding(0.0).gap(12.0));
        let first = ui.append(row, crate::Button::new().with_text("First"));
        let second = ui.append(row, crate::Button::new().with_text("Second"));
        ui.set_focus(first);

        let space_down = KeyboardEvent {
            key: Key::Character(" ".into()),
            code: Code::Unidentified,
            state: KeyState::Down,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            repeat: false,
            is_composing: false,
        };

        let _ = ui.handle_keyboard_event(&space_down);
        assert!(
            ui.rebuild()
                .resolved_element(first)
                .expect("button resolved")
                .pressed
        );

        ui.set_focus(second);
        let scene = ui.rebuild();
        assert!(
            !scene
                .resolved_element(first)
                .expect("button resolved")
                .pressed
        );
    }

    #[test]
    fn arrow_navigation_moves_button_focus_when_widget_does_not_handle() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 120.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append(ui.root(), crate::Row::new().padding(0.0).gap(12.0));
        let first = ui.append(row, crate::Button::new().with_text("First"));
        let second = ui.append(row, crate::Button::new().with_text("Second"));
        ui.set_focus(first);

        let right = KeyboardEvent {
            key: Key::Named(NamedKey::ArrowRight),
            code: Code::Unidentified,
            state: KeyState::Down,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            repeat: false,
            is_composing: false,
        };

        let batch = ui.handle_keyboard_event(&right);
        assert_eq!(ui.runtime.focused, Some(second));
        assert!(batch.events().contains(&Interaction::FocusChanged(second)));
    }

    #[test]
    fn arrow_navigation_prefers_same_focus_group() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 360.0, 160.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append(ui.root(), crate::Column::new().padding(0.0).gap(8.0));
        let top = ui.append(column, crate::Button::new().with_text("Top"));
        let row = ui.append(column, crate::Row::new().padding(0.0).gap(12.0));
        let left = ui.append(row, crate::Button::new().with_text("Left"));
        let right = ui.append(row, crate::Button::new().with_text("Right"));

        let group = Some(understory_focus::FocusSymbol(7));
        ui.set_local(left, ui.properties().focus_group, group);
        ui.set_local(right, ui.properties().focus_group, group);
        ui.set_focus(left);

        let up = KeyboardEvent {
            key: Key::Named(NamedKey::ArrowUp),
            code: Code::Unidentified,
            state: KeyState::Down,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            repeat: false,
            is_composing: false,
        };

        let batch = ui.handle_keyboard_event(&up);
        assert_eq!(ui.runtime.focused, Some(right));
        assert!(batch.events().contains(&Interaction::FocusChanged(right)));
        assert_ne!(ui.runtime.focused, Some(top));
    }

    #[test]
    fn focused_button_uses_focus_ring_border() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 120.0));

        let button = ui.append(ui.root(), crate::Button::new().with_text("Save"));
        ui.set_focus(button);

        let theme_focus_ring = *ui
            .theme()
            .get(ThemeKeys::FOCUS_RING_COLOR)
            .expect("focus ring color in theme");
        let scene = ui.rebuild();
        let resolved = scene.resolved_element(button).expect("button resolved");

        assert_eq!(resolved.border.color, theme_focus_ring);
        assert_eq!(resolved.border.width, 2.0);
        assert!(resolved.focus_visible);
    }

    #[test]
    fn focused_text_input_uses_focus_ring_border() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 120.0));

        let input = ui.append(ui.root(), crate::TextInput::new(16.0));
        ui.set_focus(input);

        let theme_focus_ring = *ui
            .theme()
            .get(ThemeKeys::FOCUS_RING_COLOR)
            .expect("focus ring color in theme");
        let scene = ui.rebuild();
        let resolved = scene.resolved_element(input).expect("text input resolved");

        assert_eq!(resolved.border.color, theme_focus_ring);
        assert_eq!(resolved.border.width, 2.0);
        assert!(resolved.focus_visible);
    }

    #[test]
    fn disabled_button_is_not_clickable_or_focusable() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 240.0, 120.0));

        let button = ui.append(ui.root(), crate::Button::new().with_text("Disabled"));
        ui.set_local(button, ui.properties().enabled, false);

        let center = ui
            .rebuild()
            .resolved_element(button)
            .expect("button resolved")
            .rect
            .center();

        let _ = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
            pointer: primary_pointer(),
            current: pointer_state(center.x, center.y, 1),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        }));
        let _ = ui.handle_pointer_event(&PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(center.x, center.y, 2),
        }));
        let up_batch = ui.handle_pointer_event(&PointerEvent::Up(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            pointer: primary_pointer(),
            state: pointer_state(center.x, center.y, 3),
        }));

        assert!(!up_batch.events().contains(&Interaction::Clicked(button)));
        assert_ne!(ui.runtime.focused, Some(button));
        let resolved = ui
            .rebuild()
            .resolved_element(button)
            .expect("button resolved");
        assert!(resolved.disabled);
        assert_eq!(resolved.semantics.role, crate::SemanticRole::Button);
        assert!(resolved.semantics.state.disabled);
    }

    #[test]
    fn resolved_semantics_expose_role_name_and_value() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 160.0));

        let button = ui.append(ui.root(), crate::Button::new().with_text("Save"));
        let input = ui.append(ui.root(), crate::TextInput::new(16.0).with_text("hello"));

        let scene = ui.rebuild();
        let button_resolved = scene.resolved_element(button).expect("button resolved");
        let input_resolved = scene.resolved_element(input).expect("input resolved");

        assert_eq!(button_resolved.semantics.role, crate::SemanticRole::Button);
        assert_eq!(button_resolved.semantics.name.as_deref(), Some("Save"));
        assert_eq!(button_resolved.semantics.value.as_deref(), None);

        assert_eq!(
            input_resolved.semantics.role,
            crate::SemanticRole::TextInput
        );
        assert_eq!(input_resolved.semantics.name.as_deref(), None);
        assert_eq!(input_resolved.semantics.value.as_deref(), Some("hello"));
    }

    #[test]
    fn single_line_text_input_submits_on_enter() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 120.0));

        let input = ui.append(ui.root(), crate::TextInput::new(16.0).single_line());
        ui.set_focus(input);
        let enter = KeyboardEvent::key_down(Key::Named(NamedKey::Enter), Code::Enter);

        let batch = ui.handle_keyboard_event(&enter);

        assert!(batch.events().contains(&Interaction::Submitted(input)));
        assert_eq!(ui.text_buffer(input), "");
    }

    #[test]
    fn tooltip_surface_is_out_of_flow() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 240.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let column = ui.append(ui.root(), crate::Column::new().padding(0.0).gap(8.0));
        let first = ui.append(column, crate::Button::new().with_text("One"));
        let second = ui.append(column, crate::Button::new().with_text("Two"));
        let tooltip = ui.append(
            ui.root(),
            crate::Tooltip::new(first)
                .with_text("Ships")
                .width(120.0)
                .height(28.0),
        );

        let expected_gap = {
            let scene = ui.rebuild();
            let first_rect = scene.resolved_element(first).expect("first resolved").rect;
            let second_rect = scene
                .resolved_element(second)
                .expect("second resolved")
                .rect;
            second_rect.y0 - first_rect.y1
        };

        if let Some(widget) = ui.widget_mut::<crate::Tooltip>(tooltip) {
            widget.set_visible(true);
            widget.set_position(Point::new(40.0, 140.0));
        }
        ui.set_local(tooltip, ui.properties().visible, true);
        ui.mark_dirty(DirtyChannels::LAYOUT.into_set() | DirtyChannels::PAINT.into_set());

        let scene = ui.rebuild();
        let first_rect = scene.resolved_element(first).expect("first resolved").rect;
        let second_rect = scene
            .resolved_element(second)
            .expect("second resolved")
            .rect;
        let tooltip_rect = scene
            .resolved_element(tooltip)
            .expect("tooltip resolved")
            .rect;

        assert_eq!(second_rect.y0 - first_rect.y1, expected_gap);
        assert_eq!(tooltip_rect.origin(), Point::new(40.0, 140.0));
    }

    #[test]
    fn dropdown_surface_plan_uses_anchor_and_position() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(Rect::new(0.0, 0.0, 320.0, 240.0));

        let input = ui.append(
            ui.root(),
            crate::TextInput::new(16.0).width(180.0).padding(8.0),
        );
        let dropdown = ui.append(
            ui.root(),
            crate::Dropdown::new(input)
                .width(180.0)
                .height(96.0)
                .padding(4.0)
                .background(Color::WHITE)
                .display_name("Autocomplete dropdown"),
        );

        ui.set_dropdown_position(dropdown, 24.0, 64.0);
        ui.set_dropdown_open(dropdown, true);

        let plan = ui.surface_plan();
        let surface = plan
            .overlay_surfaces()
            .find(|surface| surface.element_id == dropdown)
            .expect("dropdown surface");

        assert_eq!(surface.role, crate::SurfaceRole::Dropdown);
        assert_eq!(surface.bounds.origin(), Point::new(24.0, 64.0));
        let anchor = surface.anchor.as_ref().expect("dropdown anchor");
        assert_eq!(anchor.owner, input);
        assert_eq!(anchor.kind, crate::AnchorKind::Dropdown);
    }
}
