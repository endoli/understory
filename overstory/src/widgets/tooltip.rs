// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Tooltip widget — promoted overlay surface with text content.

use alloc::vec::Vec;

use peniko::Brush;
use understory_display::{DisplayAlign, DisplayNode, Insets};

use crate::{ElementId, ResolvedElement, SurfaceRole, Widget};

/// Tooltip-specific padding (tighter than the theme default).
const TOOLTIP_PADDING: f64 = 6.0;
/// Tooltip-specific font size (smaller than the theme default).
const TOOLTIP_FONT_SIZE: f64 = 13.0;

/// Tooltip widget that renders as a promoted overlay surface.
///
/// Associates with a trigger element. The tooltip is visible only when
/// the trigger is hovered, and positions itself below the trigger's rect.
/// Use [`crate::Ui::update_tooltips`] to drive visibility and positioning.
#[derive(Clone, Debug)]
pub struct Tooltip {
    trigger: ElementId,
    visible: bool,
    /// Desired position in root coordinates (set by `update_tooltips`).
    position: Option<kurbo::Point>,
}

impl Tooltip {
    /// Creates a tooltip widget associated with a trigger element.
    #[must_use]
    pub fn new(trigger: ElementId) -> Self {
        Self {
            trigger,
            visible: false,
            position: None,
        }
    }

    /// Returns the trigger element this tooltip is associated with.
    #[must_use]
    pub fn trigger(&self) -> ElementId {
        self.trigger
    }

    /// Returns whether the tooltip is currently visible.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Sets the tooltip visibility.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Returns the desired position in root coordinates.
    #[must_use]
    pub fn position(&self) -> Option<kurbo::Point> {
        self.position
    }

    /// Sets the desired position in root coordinates.
    pub fn set_position(&mut self, position: kurbo::Point) {
        self.position = Some(position);
    }
}

impl Widget for Tooltip {
    fn surface_role(&self) -> Option<SurfaceRole> {
        if self.visible {
            Some(SurfaceRole::Tooltip)
        } else {
            None
        }
    }

    fn display(&self, _id: ElementId, resolved: &ResolvedElement, children: &mut Vec<DisplayNode>) {
        let Some(label) = resolved.label.as_deref() else {
            return;
        };
        if label.is_empty() {
            return;
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "Font size is a small positive value; f32 is sufficient."
        )]
        let text_node = DisplayNode::text(
            label,
            Brush::Solid(resolved.foreground),
            TOOLTIP_FONT_SIZE as f32,
            &*resolved.font_family,
            resolved.text_align,
        );
        children.push(crate::content_box(
            text_node,
            DisplayAlign::Start,
            DisplayAlign::Center,
            Insets::uniform(TOOLTIP_PADDING),
        ));
    }

    crate::impl_widget_any!();
}
