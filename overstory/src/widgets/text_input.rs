// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text input widget backed by Parley's `PlainEditor`.

use alloc::vec::Vec;
use core::any::Any;

use kurbo::{Point, Rect, Vec2};
use parley::PlainEditor;
use peniko::Brush;
use understory_display::{DisplayNode, Insets, TextEngine};
use ui_events::keyboard::{Key, KeyboardEvent, Modifiers, NamedKey};

use crate::{ElementId, Interaction, InteractionBatch, ResolvedElement, Widget};

impl core::fmt::Debug for TextInputWidget {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextInputWidget")
            .field("text_len", &self.editor.raw_text().len())
            .finish_non_exhaustive()
    }
}

/// Text input widget with cursor movement, selection, and word-aware editing.
///
/// Wraps Parley's [`PlainEditor`] and provides keyboard event handling,
/// click-to-position cursor placement, and cursor/selection rendering.
// PlainEditor doesn't implement Debug, so we can't derive it.
pub struct TextInputWidget {
    editor: PlainEditor<Brush>,
    cached_cursor_rect: Option<Rect>,
    cached_selection_rects: Vec<Rect>,
}

impl TextInputWidget {
    /// Creates a new text input widget with the given default font size.
    #[must_use]
    pub fn new(font_size: f32) -> Self {
        Self {
            editor: PlainEditor::new(font_size),
            cached_cursor_rect: None,
            cached_selection_rects: Vec::new(),
        }
    }

    /// Returns the current text buffer content.
    #[must_use]
    pub fn text(&self) -> &str {
        self.editor.raw_text()
    }

    /// Clears the text buffer and resets the cursor to the start.
    pub fn clear(&mut self, text: &mut TextEngine) {
        self.editor.set_text("");
        let (font_cx, layout_cx) = text.contexts();
        self.editor.driver(font_cx, layout_cx).move_to_text_start();
    }
}

impl Widget for TextInputWidget {
    fn measure_height(
        &self,
        _available_width: f64,
        style_height: f64,
        _style_padding: f64,
        _label: Option<&str>,
    ) -> Option<f64> {
        Some(style_height.max(0.0))
    }

    fn display(
        &self,
        _id: ElementId,
        resolved: &ResolvedElement,
        children: &mut Vec<DisplayNode>,
    ) {
        let label_padding = resolved.label_padding;
        let selection_brush = Brush::Solid(peniko::Color::from_rgba8(80, 140, 220, 100));
        let cursor_brush = Brush::Solid(resolved.foreground);

        let mut overlay_nodes = Vec::new();
        for sel_rect in &self.cached_selection_rects {
            overlay_nodes.push(DisplayNode::offset(
                Vec2::new(sel_rect.x0, sel_rect.y0),
                DisplayNode::fixed_frame(sel_rect.size(), DisplayNode::fill_rect(selection_brush.clone())),
            ));
        }
        if let Some(cursor) = &self.cached_cursor_rect {
            overlay_nodes.push(DisplayNode::offset(
                Vec2::new(cursor.x0, cursor.y0),
                DisplayNode::fixed_frame(cursor.size(), DisplayNode::fill_rect(cursor_brush)),
            ));
        }
        if !overlay_nodes.is_empty() {
            children.push(DisplayNode::align(
                understory_display::DisplayAlign::Start,
                understory_display::DisplayAlign::Center,
                DisplayNode::padding(
                    Insets::symmetric(label_padding, 0.0),
                    DisplayNode::stack(overlay_nodes),
                ),
            ));
        }
    }

    fn keyboard_event(
        &mut self,
        id: ElementId,
        event: &KeyboardEvent,
        text: &mut TextEngine,
        batch: &mut InteractionBatch,
    ) -> bool {
        if !event.state.is_down() {
            return false;
        }

        let (font_cx, layout_cx) = text.contexts();
        let mut driver = self.editor.driver(font_cx, layout_cx);
        let action_mod = event.modifiers.contains(Modifiers::META)
            || event.modifiers.contains(Modifiers::CONTROL);

        match &event.key {
            Key::Character(ch) if action_mod && ch.as_str() == "a" => {
                driver.select_all();
                true
            }
            Key::Character(ch) if action_mod => {
                let _ = ch;
                false
            }
            Key::Character(ch) => {
                driver.insert_or_replace_selection(ch);
                true
            }
            Key::Named(named) => match named {
                NamedKey::Backspace if action_mod => {
                    driver.backdelete_word();
                    true
                }
                NamedKey::Backspace => {
                    driver.backdelete();
                    true
                }
                NamedKey::Delete => {
                    driver.delete();
                    true
                }
                NamedKey::ArrowLeft if action_mod => {
                    driver.move_to_line_start();
                    true
                }
                NamedKey::ArrowRight if action_mod => {
                    driver.move_to_line_end();
                    true
                }
                NamedKey::ArrowLeft if event.modifiers.contains(Modifiers::SHIFT) => {
                    driver.select_left();
                    true
                }
                NamedKey::ArrowRight if event.modifiers.contains(Modifiers::SHIFT) => {
                    driver.select_right();
                    true
                }
                NamedKey::ArrowLeft => {
                    driver.move_left();
                    true
                }
                NamedKey::ArrowRight => {
                    driver.move_right();
                    true
                }
                NamedKey::Home => {
                    driver.move_to_line_start();
                    true
                }
                NamedKey::End => {
                    driver.move_to_line_end();
                    true
                }
                NamedKey::Enter
                    if action_mod || event.modifiers.contains(Modifiers::SHIFT) =>
                {
                    batch.push(Interaction::Submitted(id));
                    false // don't mark layout dirty
                }
                NamedKey::Enter => {
                    driver.insert_or_replace_selection("\n");
                    true
                }
                _ => false,
            },
        }
    }

    fn click(
        &mut self,
        _id: ElementId,
        point: Point,
        resolved: &ResolvedElement,
        text: &mut TextEngine,
    ) {
        let label_padding = resolved.label_padding;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "Parley move_to_point takes f32; display coordinates are small."
        )]
        let local_x = (point.x - resolved.rect.x0 - label_padding) as f32;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "Parley move_to_point takes f32; display coordinates are small."
        )]
        let local_y = (point.y - resolved.rect.y0) as f32;
        let (font_cx, layout_cx) = text.contexts();
        self.editor.driver(font_cx, layout_cx).move_to_point(local_x, local_y);
    }

    fn refresh_layout(&mut self, text: &mut TextEngine) {
        let (font_cx, layout_cx) = text.contexts();
        self.editor.refresh_layout(font_cx, layout_cx);
        self.cached_cursor_rect = self
            .editor
            .cursor_geometry(2.0)
            .map(|bb| Rect::new(bb.x0, bb.y0, bb.x1, bb.y1));
        let mut rects = Vec::new();
        self.editor.selection_geometry_with(|bb, _line| {
            rects.push(Rect::new(bb.x0, bb.y0, bb.x1, bb.y1));
        });
        self.cached_selection_rects = rects;
    }

    fn label(&self) -> Option<&str> {
        let text = self.editor.raw_text();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
