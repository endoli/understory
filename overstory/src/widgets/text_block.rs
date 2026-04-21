// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text block widget for multiline wrapped text.

use alloc::vec::Vec;
use core::any::Any;

use peniko::Brush;
use understory_display::{DisplayNode, Insets};

use crate::{ElementId, ResolvedElement, Widget};

/// Default font size fallback.
const DEFAULT_FONT_SIZE: f64 = 16.0;
/// Default font family fallback.
const DEFAULT_FONT_FAMILY: &str = "sans-serif";

/// Multiline wrapped text block widget.
///
/// Renders its label as top-left aligned, uniformly padded text that wraps
/// at the container width. Height is estimated from the label length and
/// font size during scene layout.
#[derive(Clone, Debug, Default)]
pub struct TextBlockWidget;

impl TextBlockWidget {
    /// Creates a new text block widget.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Widget for TextBlockWidget {
    fn measure_height(
        &self,
        available_width: f64,
        _style_height: f64,
        style_padding: f64,
        label: Option<&str>,
    ) -> Option<f64> {
        let font_size = DEFAULT_FONT_SIZE;
        let line_height = font_size * 1.4;
        let content_width = (available_width - style_padding * 2.0).max(1.0);
        let char_count = label.map_or(0, |l| l.len());
        let avg_char_width = font_size * 0.55;
        let estimated_text_width = char_count as f64 * avg_char_width;
        let lines = (estimated_text_width / content_width).ceil().max(1.0);
        Some((lines * line_height + style_padding * 2.0).max(0.0))
    }

    fn display(
        &self,
        _id: ElementId,
        resolved: &ResolvedElement,
        children: &mut Vec<DisplayNode>,
    ) {
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
        children.push(DisplayNode::padding(
            Insets::uniform(resolved.label_padding.max(0.0)),
            text_node,
        ));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
