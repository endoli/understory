// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text input widget backed by Parley's `PlainEditor`.

use alloc::vec::Vec;
use core::cell::Cell;

use kurbo::{Point, Rect, Vec2};
use parley::PlainEditor;
use peniko::{Brush, Color};
use ui_events::keyboard::{Key, KeyboardEvent, Modifiers, NamedKey};
use understory_display::{DisplayAlign, DisplayNode, Insets, TextEngine};

use understory_style::ResourceKey;

use crate::{
    Element, ElementId, Interaction, InteractionBatch, ResolvedElement, ThemeKeys, Widget,
    content_box, text_label_node, text_label_node_constrained,
};

/// Label padding used for content box calculation in `measure`.
/// Must match the resolved `label_padding` for consistent geometry.
const CONTENT_PADDING: f64 = 12.0;

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
    placeholder: Option<alloc::string::String>,
    /// Last measured content width, used to set editor wrap width in `refresh_layout`.
    last_content_width: Cell<Option<f32>>,
    /// Whether the cursor is currently visible (toggles for blink).
    cursor_visible: bool,
    /// Timer ID for the blink timer, if active.
    blink_timer: Option<crate::TimerId>,
}

impl TextInputWidget {
    /// Creates a new text input widget with the given default font size.
    #[must_use]
    pub fn new(font_size: f32) -> Self {
        Self {
            editor: PlainEditor::new(font_size),
            cached_cursor_rect: None,
            cached_selection_rects: Vec::new(),
            placeholder: None,
            last_content_width: Cell::new(None),
            cursor_visible: true,
            blink_timer: None,
        }
    }

    /// Returns the current text buffer content.
    #[must_use]
    pub fn text(&self) -> &str {
        self.editor.raw_text()
    }

    /// Sets the placeholder text shown when the input is empty and unfocused.
    pub fn set_placeholder(&mut self, placeholder: impl Into<alloc::string::String>) {
        self.placeholder = Some(placeholder.into());
    }

    /// Starts cursor blink. Called when the input gains focus.
    pub fn start_blink(
        &mut self,
        ui_timers: &mut crate::TimerQueue,
        element_id: ElementId,
        now: u64,
    ) {
        self.cursor_visible = true;
        if self.blink_timer.is_none() {
            const BLINK_INTERVAL: u64 = 500_000_000; // 500ms in nanos
            let id = ui_timers.request(element_id, now, BLINK_INTERVAL, Some(BLINK_INTERVAL));
            self.blink_timer = Some(id);
        }
    }

    /// Stops cursor blink. Called when the input loses focus.
    pub fn stop_blink(&mut self, ui_timers: &mut crate::TimerQueue) {
        if let Some(id) = self.blink_timer.take() {
            ui_timers.cancel(id);
        }
        self.cursor_visible = true;
    }

    /// Resets the blink cycle (cursor becomes visible). Called on typing.
    pub fn reset_blink(&mut self) {
        self.cursor_visible = true;
    }

    /// Clears the text buffer and resets the cursor to the start.
    pub fn clear(&mut self, text: &mut TextEngine) {
        self.editor.set_text("");
        let (font_cx, layout_cx) = text.contexts();
        self.editor.driver(font_cx, layout_cx).move_to_text_start();
    }
}

/// Maximum content height before the input stops growing (padding added by scene).
const MAX_HEIGHT: f64 = 100.0;

impl Widget for TextInputWidget {
    fn measure(
        &self,
        available: kurbo::Size,
        ctx: &mut crate::MeasureCtx<'_>,
    ) -> Option<kurbo::Size> {
        // Subtract internal padding to match the text content box used by
        // display() and click(). Text input uses one top-left aligned content
        // box for measurement, painting, and hit-testing.
        let padding = CONTENT_PADDING;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "Display coordinates are small positive values."
        )]
        let text_width = (available.width - padding * 2.0).max(1.0) as f32;
        // Store for refresh_layout to set editor wrap width.
        self.last_content_width.set(Some(text_width));

        let text = self.editor.raw_text();
        let font_size = self.editor.get_font_size();
        let line_height = f64::from(font_size) * 1.4;
        if text.is_empty() {
            return Some(kurbo::Size::new(
                available.width,
                line_height + padding * 2.0,
            ));
        }
        let text_size = ctx.measure_text(text, font_size, "sans-serif", Some(text_width));
        let content_h = text_size.height.max(line_height);
        let height = (content_h + padding * 2.0).min(MAX_HEIGHT);
        Some(kurbo::Size::new(available.width, height))
    }

    fn display(&self, _id: ElementId, resolved: &ResolvedElement, children: &mut Vec<DisplayNode>) {
        // Render the text content.
        let is_empty = resolved.label.as_deref().is_none_or(|l| l.is_empty());
        let show_placeholder = is_empty && self.placeholder.is_some();

        let display_text = if show_placeholder {
            self.placeholder.as_deref()
        } else {
            resolved.label.as_deref()
        };

        if let Some(label) = display_text
            && !label.is_empty()
        {
            let text_brush = if show_placeholder {
                let fg = resolved.foreground.to_rgba8();
                Brush::Solid(Color::from_rgba8(fg.r, fg.g, fg.b, 100))
            } else {
                Brush::Solid(resolved.foreground)
            };
            let text_node = if let Some(w) = self.last_content_width.get() {
                text_label_node_constrained(label, text_brush, resolved, f64::from(w))
            } else {
                text_label_node(label, text_brush, resolved)
            };
            children.push(content_box(
                text_node,
                DisplayAlign::Start,
                DisplayAlign::Start,
                Insets::uniform(resolved.label_padding),
            ));
        }

        // Render selection and cursor overlays.
        let label_padding = resolved.label_padding;
        let selection_brush = Brush::Solid(Color::from_rgba8(80, 140, 220, 100));
        let cursor_brush = Brush::Solid(resolved.foreground);

        let mut overlay_nodes = Vec::new();
        for sel_rect in &self.cached_selection_rects {
            overlay_nodes.push(DisplayNode::offset(
                Vec2::new(sel_rect.x0, sel_rect.y0),
                DisplayNode::fixed_frame(
                    sel_rect.size(),
                    DisplayNode::fill_rect(selection_brush.clone()),
                ),
            ));
        }
        if let Some(cursor) = &self.cached_cursor_rect
            && self.cursor_visible
        {
            overlay_nodes.push(DisplayNode::offset(
                Vec2::new(cursor.x0, cursor.y0),
                DisplayNode::fixed_frame(cursor.size(), DisplayNode::fill_rect(cursor_brush)),
            ));
        }
        if !overlay_nodes.is_empty() {
            children.push(content_box(
                DisplayNode::stack(overlay_nodes),
                DisplayAlign::Start,
                DisplayAlign::Start,
                Insets::uniform(label_padding),
            ));
        }
    }

    fn on_timer(&mut self, id: crate::TimerId, _now: u64) {
        if self.blink_timer == Some(id) {
            self.cursor_visible = !self.cursor_visible;
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

        // Reset blink cycle — cursor becomes visible on any keypress.
        self.reset_blink();

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
                NamedKey::Enter if action_mod || event.modifiers.contains(Modifiers::SHIFT) => {
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
        let local_y = (point.y - resolved.rect.y0 - label_padding) as f32;
        let (font_cx, layout_cx) = text.contexts();
        self.editor
            .driver(font_cx, layout_cx)
            .move_to_point(local_x, local_y);
    }

    fn refresh_layout(&mut self, text: &mut TextEngine) {
        // Apply the wrap width from the last measure pass.
        if let Some(w) = self.last_content_width.get() {
            self.editor.set_width(Some(w));
        }
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
        if text.is_empty() { None } else { Some(text) }
    }

    fn background_key(&self, _element: &Element) -> Option<ResourceKey> {
        Some(ThemeKeys::PANEL_BACKGROUND)
    }

    fn height_key(&self) -> Option<ResourceKey> {
        // No theme height — TextInput uses Widget::measure() for dynamic sizing.
        None
    }

    fn default_pickable(&self) -> bool {
        true
    }

    fn default_focusable(&self) -> bool {
        true
    }

    crate::impl_widget_any!();
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, vec::Vec};

    use super::*;
    use crate::{BorderStyle, ElementId, MeasureCtx, ResolvedElement, TYPE_TEXT_INPUT};
    use kurbo::{Point, Rect};
    use peniko::Color;
    use understory_display::{DisplayNodeKind, TextAlign, TextEngine};

    fn resolved_text_input(rect: Rect, label: &str) -> ResolvedElement {
        ResolvedElement {
            id: ElementId::new(1),
            type_tag: TYPE_TEXT_INPUT,
            depth: 0,
            rect,
            background: Color::WHITE,
            foreground: Color::BLACK,
            border: BorderStyle::default(),
            corner_radius: 0.0,
            label: Some(Box::<str>::from(label)),
            hovered: false,
            pressed: false,
            focused: true,
            font_size: 16.0,
            label_padding: CONTENT_PADDING,
            font_family: Box::<str>::from("sans-serif"),
            text_align: TextAlign::Start,
            clips_content: false,
            scroll_offset: 0.0,
            widget: None,
        }
    }

    #[test]
    fn multiline_display_uses_uniform_padding_and_top_alignment() {
        let widget = TextInputWidget::new(16.0_f32);
        let resolved = resolved_text_input(Rect::new(0.0, 0.0, 240.0, 96.0), "alpha\nbeta");
        let mut children = Vec::new();

        widget.display(resolved.id, &resolved, &mut children);

        let Some(text_node) = children.first() else {
            panic!("expected text node");
        };
        let DisplayNodeKind::Align {
            horizontal,
            vertical,
            child,
        } = text_node.kind()
        else {
            panic!("expected align node");
        };
        assert_eq!(*horizontal, DisplayAlign::Start);
        assert_eq!(*vertical, DisplayAlign::Start);

        let DisplayNodeKind::Padding { insets, .. } = child.kind() else {
            panic!("expected padded text");
        };
        assert_eq!(*insets, Insets::uniform(CONTENT_PADDING));
    }

    #[test]
    fn multiline_click_targets_second_line_with_padded_origin() {
        let mut widget = TextInputWidget::new(16.0_f32);
        widget.editor.set_text("alpha\nbeta");

        let mut text = TextEngine::new();
        let available = kurbo::Size::new(240.0, f64::INFINITY);
        let mut measure = MeasureCtx::new(&mut text);
        let measured = widget
            .measure(available, &mut measure)
            .expect("text input should measure");
        let resolved = resolved_text_input(
            Rect::from_origin_size(Point::ORIGIN, measured),
            widget.editor.raw_text(),
        );

        widget.refresh_layout(&mut text);
        let (font_cx, layout_cx) = text.contexts();
        widget
            .editor
            .driver(font_cx, layout_cx)
            .move_to_text_start();
        widget.refresh_layout(&mut text);

        let initial_cursor = widget
            .cached_cursor_rect
            .expect("cursor geometry should exist");
        assert!(initial_cursor.y0 <= 0.1);

        let line_height = f64::from(widget.editor.get_font_size()) * 1.4;
        let click_point = Point::new(
            resolved.rect.x0 + CONTENT_PADDING + 4.0,
            resolved.rect.y0 + CONTENT_PADDING + line_height + 1.0,
        );
        widget.click(resolved.id, click_point, &resolved, &mut text);
        widget.refresh_layout(&mut text);

        let moved_cursor = widget
            .cached_cursor_rect
            .expect("cursor geometry should exist after click");
        assert!(
            moved_cursor.y0 >= line_height * 0.5,
            "cursor should move onto the second line; got {:?}",
            moved_cursor
        );
    }

    #[test]
    fn trailing_newline_does_not_add_extra_height_beyond_second_line() {
        let mut text = TextEngine::new();
        let available = kurbo::Size::new(240.0, f64::INFINITY);

        let mut newline_widget = TextInputWidget::new(16.0_f32);
        newline_widget.editor.set_text("alpha\n");
        let mut measure = MeasureCtx::new(&mut text);
        let newline_size = newline_widget
            .measure(available, &mut measure)
            .expect("text input should measure");

        let mut text_widget = TextInputWidget::new(16.0_f32);
        text_widget.editor.set_text("alpha\nb");
        let mut measure = MeasureCtx::new(&mut text);
        let text_size = text_widget
            .measure(available, &mut measure)
            .expect("text input should measure");

        assert_eq!(
            newline_size.height, text_size.height,
            "entering the first character on a new line should not shrink the widget"
        );
    }
}
