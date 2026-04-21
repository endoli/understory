// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Tooltip widget — promoted overlay surface with text content.

use alloc::vec::Vec;
use core::any::Any;

use peniko::Brush;
use understory_display::{DisplayAlign, DisplayNode, Insets};

use crate::{ElementId, ResolvedElement, SurfaceRole, Widget};

const DEFAULT_FONT_SIZE: f64 = 13.0;
const DEFAULT_PADDING: f64 = 6.0;
const DEFAULT_FONT_FAMILY: &str = "sans-serif";

/// Tooltip widget that renders as a promoted overlay surface.
///
/// Associates with a trigger element. The tooltip is visible only when
/// the trigger is hovered, and positions itself below the trigger's rect.
/// Use [`crate::Ui::update_tooltips`] to drive visibility and positioning.
#[derive(Clone, Debug)]
pub struct TooltipWidget {
    trigger: ElementId,
    visible: bool,
    /// Desired position in root coordinates (set by `update_tooltips`).
    position: Option<kurbo::Point>,
}

impl TooltipWidget {
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

impl Widget for TooltipWidget {
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
        let font_size = if resolved.font_size > 0.0 {
            resolved.font_size
        } else {
            DEFAULT_FONT_SIZE
        };
        let font_family = if resolved.font_family.is_empty() {
            DEFAULT_FONT_FAMILY
        } else {
            &resolved.font_family
        };
        #[allow(
            clippy::cast_possible_truncation,
            reason = "Font size is a small positive value; f32 is sufficient."
        )]
        let text_node = DisplayNode::text(
            label,
            Brush::Solid(resolved.foreground),
            font_size as f32,
            font_family,
            resolved.text_align,
        );
        children.push(DisplayNode::align(
            DisplayAlign::Start,
            DisplayAlign::Center,
            DisplayNode::padding(Insets::uniform(DEFAULT_PADDING), text_node),
        ));
    }

    fn background_key(&self, _element: &crate::Element) -> Option<understory_style::ResourceKey> {
        Some(crate::ThemeKeys::BUTTON_BACKGROUND)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
