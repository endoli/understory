// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Button widget with centered label text.

use alloc::vec::Vec;

use peniko::Brush;
use understory_display::{DisplayAlign, DisplayNode, Insets};


use understory_style::ResourceKey;

use crate::{
    ButtonClass, Element, ElementId, ResolvedElement, ThemeKeys, Widget, content_box,
    text_label_node,
};


/// Interactive push button widget with horizontally padded, vertically
/// centered label text.
#[derive(Clone, Debug, Default)]
pub struct ButtonWidget;

impl ButtonWidget {
    /// Creates a new button widget.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Widget for ButtonWidget {
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

    fn background_key(&self, element: &Element) -> Option<ResourceKey> {
        let primary = element.classes.contains(ButtonClass::Primary.class_id());
        Some(
            match (primary, element.pseudos.pressed, element.pseudos.hovered) {
                (true, true, _) => ThemeKeys::PRIMARY_PRESSED_BACKGROUND,
                (true, false, true) => ThemeKeys::PRIMARY_HOVER_BACKGROUND,
                (true, false, false) => ThemeKeys::PRIMARY_BACKGROUND,
                (false, true, _) => ThemeKeys::BUTTON_PRESSED_BACKGROUND,
                (false, false, true) => ThemeKeys::BUTTON_HOVER_BACKGROUND,
                (false, false, false) => ThemeKeys::BUTTON_BACKGROUND,
            },
        )
    }

    fn height_key(&self) -> Option<ResourceKey> {
        Some(ThemeKeys::BUTTON_HEIGHT)
    }

    fn default_pickable(&self) -> bool {
        true
    }

    fn default_focusable(&self) -> bool {
        true
    }

    crate::impl_widget_any!();
}
