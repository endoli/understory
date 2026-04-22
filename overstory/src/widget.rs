// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Widget trait and arena for attachable element behavior.
//!
//! Widgets are optional behavioral fragments attached to elements in the
//! retained tree. The element tree owns structure, identity, and properties;
//! widgets own kind-specific state and behavior (event handling, custom
//! layout, rendering).

use alloc::{boxed::Box, vec::Vec};
use core::any::Any;

use kurbo::{Point, Size};
use understory_display::{DisplayNode, TextEngine};
use understory_style::ResourceKey;

/// Context provided to widgets during measurement.
///
/// Exposes text measurement without leaking `TextEngine` or Parley
/// directly into the widget interface.
pub struct MeasureCtx<'a> {
    text: &'a mut TextEngine,
}

impl core::fmt::Debug for MeasureCtx<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MeasureCtx").finish_non_exhaustive()
    }
}

impl<'a> MeasureCtx<'a> {
    /// Creates a measurement context wrapping a text engine.
    pub fn new(text: &'a mut TextEngine) -> Self {
        Self { text }
    }

    /// Measures a text string and returns its rendered size.
    ///
    /// `font_size` is in logical display units. `font_family` is a CSS-like
    /// family string. `max_width` constrains line breaking.
    #[must_use]
    pub fn measure_text(
        &mut self,
        text: &str,
        font_size: f32,
        font_family: &str,
        max_width: Option<f32>,
    ) -> Size {
        self.text
            .measure_text(text, font_size, font_family, max_width)
    }
}

use crate::{ElementId, InteractionBatch, ResolvedElement};

/// Builds a text display node from a resolved element's label and style.
///
/// This is the shared helper for all widgets that render text labels.
/// Uses the resolved `font_size`, `font_family`, and `text_align`
/// which are guaranteed to have non-zero/non-empty values.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Font size is a small positive value; f32 is sufficient."
)]
pub fn text_label_node(label: &str, brush: peniko::Brush, resolved: &ResolvedElement) -> DisplayNode {
    DisplayNode::text(
        label,
        brush,
        resolved.font_size as f32,
        &*resolved.font_family,
        resolved.text_align,
    )
}

/// Builds a text display node with an explicit max width for line breaking.
///
/// Use this when measurement and rendering must agree on wrapping width
/// (e.g., text inputs, constrained paragraphs).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Font size and max_width are small display values."
)]
pub fn text_label_node_constrained(
    label: &str,
    brush: peniko::Brush,
    resolved: &ResolvedElement,
    max_width: f64,
) -> DisplayNode {
    DisplayNode::text_constrained(
        label,
        brush,
        resolved.font_size as f32,
        &*resolved.font_family,
        resolved.text_align,
        max_width as f32,
    )
}

/// Thin behavioral interface for element-attached widgets.
///
/// Widgets provide kind-specific state, layout, rendering, and event handling
/// without requiring changes to the core element tree or scene resolution.
///
/// All methods have default no-op implementations so widgets only need to
/// override what they care about.
pub trait Widget {
    /// Measure the widget's desired size given available space.
    ///
    /// `ctx` provides text measurement and other layout capabilities.
    /// Return `Some(size)` to provide a measured size. Return `None` to
    /// fall through to the standard container layout.
    fn measure(&self, _available: Size, _ctx: &mut MeasureCtx<'_>) -> Option<Size> {
        None
    }

    /// Produce display nodes for this widget's visual content.
    ///
    /// Called during display tree projection. Nodes are added to `children`
    /// alongside the element's background, border, and label nodes.
    fn display(
        &self,
        _id: ElementId,
        _resolved: &ResolvedElement,
        _children: &mut Vec<DisplayNode>,
    ) {
    }

    /// Handle a keyboard event when this widget is focused.
    ///
    /// `text` provides font/layout contexts for text editing operations.
    /// Push interactions to `batch`. Return `true` if the event was handled.
    fn keyboard_event(
        &mut self,
        _id: ElementId,
        _event: &ui_events::keyboard::KeyboardEvent,
        _text: &mut TextEngine,
        _batch: &mut InteractionBatch,
    ) -> bool {
        false
    }

    /// Handle a click on this widget.
    ///
    /// `point` is in view-space coordinates. `resolved` provides the element's
    /// layout rect for computing local positions.
    fn click(
        &mut self,
        _id: ElementId,
        _point: Point,
        _resolved: &ResolvedElement,
        _text: &mut TextEngine,
    ) {
    }

    /// Called when a timer fires for this widget's element.
    fn on_timer(&mut self, _id: crate::TimerId, _now: u64) {}

    /// Refresh any internal layout state before scene resolution.
    ///
    /// Called before each scene rebuild for widgets that cache layout data
    /// (e.g., text editor glyph positions).
    fn refresh_layout(&mut self, _text: &mut TextEngine) {}

    /// Return the widget's effective label text, if any.
    ///
    /// Used by scene resolution to populate `ResolvedElement::label` for
    /// widgets that generate their own text content (e.g., text input buffers).
    fn label(&self) -> Option<&str> {
        None
    }

    /// Return the theme resource key for this widget's background color.
    ///
    /// Called during style resolution. Return `None` for no theme background.
    fn background_key(&self, _element: &crate::Element) -> Option<ResourceKey> {
        None
    }

    /// Return the theme resource key for this widget's height.
    ///
    /// Called during style resolution. Return `None` for no theme height.
    fn height_key(&self) -> Option<ResourceKey> {
        None
    }

    /// Whether this widget makes its element pickable by default.
    fn default_pickable(&self) -> bool {
        false
    }

    /// Whether this widget makes its element focusable by default.
    fn default_focusable(&self) -> bool {
        false
    }

    /// Request that this element be promoted to a separate compositor surface.
    ///
    /// Return `Some(role)` to promote the element out of the root surface
    /// into its own overlay/popup/tooltip surface. Return `None` for normal
    /// inline rendering.
    fn surface_role(&self) -> Option<crate::SurfaceRole> {
        None
    }

    /// Downcast to a concrete type for typed accessors.
    fn as_any(&self) -> &dyn Any;

    /// Downcast to a concrete type for typed mutation.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Wraps content in a standard widget content box layout.
///
/// This helper centralizes the align + padding pattern so widgets use
/// the same content box for display, measurement, and hit-testing.
#[must_use]
pub fn content_box(
    content: DisplayNode,
    h_align: understory_display::DisplayAlign,
    v_align: understory_display::DisplayAlign,
    padding: understory_display::Insets,
) -> DisplayNode {
    DisplayNode::align(h_align, v_align, DisplayNode::padding(padding, content))
}

/// Implements the `as_any`/`as_any_mut` boilerplate for a Widget type.
///
/// Every widget implementation needs these two methods to support typed
/// downcasting via the arena. This macro eliminates the repetition.
#[macro_export]
macro_rules! impl_widget_any {
    () => {
        fn as_any(&self) -> &dyn core::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
            self
        }
    };
}

/// Opaque handle to a widget in the [`WidgetArena`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WidgetHandle(u32);

/// Arena storage for widget instances.
///
/// Widgets are stored by handle and accessed through the arena. This keeps
/// `Element` small and cache-friendly while allowing polymorphic dispatch.
pub struct WidgetArena {
    widgets: Vec<Option<Box<dyn Widget>>>,
}

impl WidgetArena {
    /// Creates an empty widget arena.
    #[must_use]
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
        }
    }

    /// Inserts a widget and returns its handle.
    pub fn insert(&mut self, widget: Box<dyn Widget>) -> WidgetHandle {
        let index = self.widgets.len();
        self.widgets.push(Some(widget));
        WidgetHandle(u32::try_from(index).expect("widget arena index exceeds u32"))
    }

    /// Returns a reference to the widget at the given handle.
    /// Returns a reference to the widget at the given handle.
    #[must_use]
    pub fn get(&self, handle: WidgetHandle) -> Option<&dyn Widget> {
        self.widgets.get(handle.0 as usize)?.as_ref().map(|w| &**w)
    }

    /// Returns a mutable reference to the widget at the given handle.
    #[must_use]
    pub fn get_mut(&mut self, handle: WidgetHandle) -> Option<&mut (dyn Widget + 'static)> {
        self.widgets
            .get_mut(handle.0 as usize)?
            .as_mut()
            .map(|w| &mut **w)
    }

    /// Iterates over all live widgets mutably.
    pub fn iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (WidgetHandle, &mut (dyn Widget + 'static))> + '_ {
        self.widgets.iter_mut().enumerate().filter_map(|(i, slot)| {
            let w = slot.as_mut()?;
            #[allow(
                clippy::cast_possible_truncation,
                reason = "Arena indices are bounded by insert checks."
            )]
            Some((WidgetHandle(i as u32), &mut **w))
        })
    }
}

impl Default for WidgetArena {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for WidgetArena {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WidgetArena")
            .field(
                "count",
                &self.widgets.iter().filter(|s| s.is_some()).count(),
            )
            .finish()
    }
}
