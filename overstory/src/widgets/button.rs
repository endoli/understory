// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Button widget with centered label text.

use alloc::vec::Vec;

use cursor_icon::CursorIcon;
use peniko::Brush;
use understory_display::{DisplayAlign, DisplayNode, Insets};

use crate::{Element, ElementId, ResolvedElement, Widget, content_box, text_label_node};

/// Interactive push button widget with horizontally padded, vertically
/// centered label text.
#[derive(Clone, Debug, Default)]
pub struct Button;

impl Button {
    /// Creates a new button widget.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Widget for Button {
    fn display(&self, _id: ElementId, resolved: &ResolvedElement, children: &mut Vec<DisplayNode>) {
        let Some(label) = resolved.label.as_deref() else {
            return;
        };
        if label.is_empty() {
            return;
        }
        let text_node = text_label_node(label, Brush::Solid(resolved.foreground), resolved);
        children.push(content_box(
            text_node,
            DisplayAlign::Start,
            DisplayAlign::Center,
            Insets::symmetric(resolved.label_padding, 0.0),
        ));
    }

    fn default_pickable(&self) -> bool {
        true
    }

    fn default_focusable(&self) -> bool {
        true
    }

    fn cursor_icon(&self, _element: &Element) -> Option<CursorIcon> {
        Some(CursorIcon::Pointer)
    }

    crate::impl_widget_any!();
}
