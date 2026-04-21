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

use kurbo::{Point, Rect};
use understory_display::{DisplayNode, TextEngine};

use crate::{ElementId, InteractionBatch, ResolvedElement};

/// Thin behavioral interface for element-attached widgets.
///
/// Widgets provide kind-specific state, layout, rendering, and event handling
/// without requiring changes to the core element tree or scene resolution.
///
/// All methods have default no-op implementations so widgets only need to
/// override what they care about.
pub trait Widget {
    /// Produce display nodes for this widget's visual content.
    ///
    /// Called during display tree projection. Nodes are added to `children`
    /// alongside the element's background, border, and label nodes.
    fn display(&self, _id: ElementId, _resolved: &ResolvedElement, _children: &mut Vec<DisplayNode>) {}

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

    /// Return cursor geometry from the widget's last layout, if any.
    fn cursor_rect(&self) -> Option<Rect> {
        None
    }

    /// Return selection highlight geometry from the widget's last layout.
    fn selection_rects(&self) -> &[Rect] {
        &[]
    }

    /// Downcast to a concrete type for typed accessors.
    fn as_any(&self) -> &dyn Any;

    /// Downcast to a concrete type for typed mutation.
    fn as_any_mut(&mut self) -> &mut dyn Any;
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
        self.widgets
            .get(handle.0 as usize)?
            .as_ref()
            .map(|w| &**w)
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
        self.widgets
            .iter_mut()
            .enumerate()
            .filter_map(|(i, slot)| {
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
            .field("count", &self.widgets.iter().filter(|s| s.is_some()).count())
            .finish()
    }
}
