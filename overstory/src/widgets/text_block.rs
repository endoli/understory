// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text block widget for multiline wrapped text.

use alloc::vec::Vec;

use peniko::Brush;
use understory_display::{DisplayNode, Insets};

use crate::{ElementId, ResolvedElement, Widget, text_label_node};

/// Multiline wrapped text block widget.
///
/// Renders its label as top-left aligned, uniformly padded text that wraps
/// at the container width. Height is estimated from the label length and
/// font size during scene layout.
#[derive(Clone, Debug, Default)]
pub struct TextBlock;

impl TextBlock {
    /// Creates a new text block widget.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Widget for TextBlock {
    fn display(&self, _id: ElementId, resolved: &ResolvedElement, children: &mut Vec<DisplayNode>) {
        let Some(label) = resolved.label.as_deref() else {
            return;
        };
        if label.is_empty() {
            return;
        }
        let text_node = text_label_node(label, Brush::Solid(resolved.foreground), resolved);
        children.push(DisplayNode::padding(
            Insets::uniform(resolved.label_padding),
            text_node,
        ));
    }

    crate::impl_widget_any!();
}
