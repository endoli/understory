// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{boxed::Box, format, string::String, vec::Vec};

use hashbrown::HashMap;
use overstory::{Color, ElementId, MessageClass, Ui};
use understory_transcript::{
    AnnotationLevel, EntryBody, EntryId, EntryKind, EntryStatus, MessageRole, Transcript,
    TranscriptEntry,
};

/// Styling knobs for transcript rows.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptViewStyle {
    /// Gap between row children.
    pub row_gap: f64,
    /// Inner padding of transcript text rows.
    pub row_padding: f64,
    /// Corner radius applied to row text blocks.
    pub row_corner_radius: f64,
    /// Spinner size for in-progress rows.
    pub spinner_size: f64,
}

impl Default for TranscriptViewStyle {
    fn default() -> Self {
        Self {
            row_gap: 8.0,
            row_padding: 8.0,
            row_corner_radius: 8.0,
            spinner_size: 18.0,
        }
    }
}

/// Semantic role for one transcript row.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TranscriptEntryRole {
    /// End-user authored message.
    User,
    /// Assistant/model message.
    Assistant,
    /// Tool or process/protocol message.
    Auxiliary,
    /// System or annotation-like message.
    System,
}

/// Presentation for one transcript entry row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptRowPresentation {
    /// Semantic role of the row.
    pub role: TranscriptEntryRole,
    /// Visible row text.
    pub text: String,
    /// Whether the row should be visible at all.
    pub visible: bool,
    /// Whether the row should show an in-progress spinner.
    pub show_spinner: bool,
}

/// Element ids for one realized transcript row.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TranscriptRowIds {
    /// Row container.
    pub row: ElementId,
    /// Text block child.
    pub text: ElementId,
    /// Spinner child.
    pub spinner: ElementId,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct TranscriptRowState {
    entry_id: EntryId,
    ids: TranscriptRowIds,
}

/// Append-oriented transcript view controller for one Overstory `ScrollView`.
#[derive(Clone, Debug)]
pub struct TranscriptViewController {
    scroll_view: ElementId,
    style: TranscriptViewStyle,
    rows: Vec<TranscriptRowState>,
    row_indices: HashMap<EntryId, usize>,
}

impl TranscriptViewController {
    /// Creates a controller bound to one transcript `ScrollView`.
    #[must_use]
    pub fn new(scroll_view: ElementId) -> Self {
        Self {
            scroll_view,
            style: TranscriptViewStyle::default(),
            rows: Vec::new(),
            row_indices: HashMap::new(),
        }
    }

    /// Returns the bound transcript scroll view.
    #[must_use]
    pub const fn scroll_view(&self) -> ElementId {
        self.scroll_view
    }

    /// Returns the current row styling.
    #[must_use]
    pub const fn style(&self) -> &TranscriptViewStyle {
        &self.style
    }

    /// Replaces the row styling.
    pub fn set_style(&mut self, style: TranscriptViewStyle) {
        self.style = style;
    }

    /// Returns element ids for one transcript entry row, if realized.
    #[must_use]
    pub fn row_ids(&self, entry_id: EntryId) -> Option<TranscriptRowIds> {
        let index = self.row_indices.get(&entry_id).copied()?;
        Some(self.rows.get(index)?.ids)
    }

    /// Returns `true` if the transcript view is currently at the tail.
    #[must_use]
    pub fn is_at_tail(&self, ui: &Ui) -> bool {
        let offset = ui.scroll_offset(self.scroll_view);
        let content_h = ui.content_height(self.scroll_view);
        let viewport_h = ui.viewport_height(self.scroll_view);
        content_h <= viewport_h || offset + viewport_h >= content_h - 1.0
    }

    /// Scrolls the transcript view to the tail.
    pub fn scroll_to_tail(&self, ui: &mut Ui) {
        let _ = ui.scene();
        let content_h = ui.content_height(self.scroll_view);
        let viewport_h = ui.viewport_height(self.scroll_view);
        ui.set_scroll_offset(self.scroll_view, (content_h - viewport_h).max(0.0));
    }

    /// Syncs transcript rows using the built-in projection for `EntryBody`.
    pub fn sync_default(&mut self, ui: &mut Ui, transcript: &Transcript<EntryBody>) {
        self.sync_with(ui, transcript, project_default_entry);
    }

    /// Syncs transcript rows using a caller-provided row projector.
    pub fn sync_with<P>(
        &mut self,
        ui: &mut Ui,
        transcript: &Transcript<P>,
        mut project: impl FnMut(&TranscriptEntry<P>) -> TranscriptRowPresentation,
    ) {
        let entries = transcript.entries();
        while self.rows.len() < entries.len() {
            let entry = &entries[self.rows.len()];
            let ids = self.append_row(ui);
            self.row_indices.insert(entry.id, self.rows.len());
            self.rows.push(TranscriptRowState {
                entry_id: entry.id,
                ids,
            });
        }

        for entry in entries {
            let Some(ids) = self.row_ids(entry.id) else {
                continue;
            };
            let presentation = project(entry);
            self.apply_row(ui, ids, presentation);
        }
    }

    fn append_row(&self, ui: &mut Ui) -> TranscriptRowIds {
        let row = ui.append_child(self.scroll_view, overstory::TYPE_ROW);
        ui.set_local(row, ui.properties().padding, 0.0);
        ui.set_local(row, ui.properties().gap, self.style.row_gap);
        ui.set_local(row, ui.properties().background, Color::TRANSPARENT);

        let spinner = ui.append_child_with(
            row,
            overstory::TYPE_SPINNER,
            Some(Box::new(overstory::widgets::Spinner::new(
                self.style.spinner_size,
            ))),
        );
        ui.set_local(spinner, ui.properties().visible, false);

        let text = ui.append_child(row, overstory::TYPE_TEXT_BLOCK);
        ui.set_local(text, ui.properties().label_padding, self.style.row_padding);
        ui.set_local(text, ui.properties().padding, self.style.row_padding);
        ui.set_local(
            text,
            ui.properties().corner_radius,
            self.style.row_corner_radius,
        );

        TranscriptRowIds { row, text, spinner }
    }

    fn apply_row(
        &self,
        ui: &mut Ui,
        ids: TranscriptRowIds,
        presentation: TranscriptRowPresentation,
    ) {
        let show_text = presentation.visible && !presentation.text.is_empty();
        ui.set_local(ids.row, ui.properties().visible, presentation.visible);
        ui.widget_mut::<overstory::widgets::TextBlock>(ids.text)
            .expect("transcript rows use text block children")
            .set_text(presentation.text);
        ui.set_local(ids.text, ui.properties().visible, show_text);
        ui.set_local(
            ids.spinner,
            ui.properties().visible,
            presentation.visible && presentation.show_spinner,
        );

        if presentation.visible && presentation.show_spinner {
            ui.start_spinner(ids.spinner);
        } else {
            ui.stop_spinner(ids.spinner);
        }

        if matches!(presentation.role, TranscriptEntryRole::User) {
            ui.add_class(ids.text, MessageClass::User.class_id());
        }
    }
}

fn project_default_entry(entry: &TranscriptEntry<EntryBody>) -> TranscriptRowPresentation {
    match &entry.kind {
        EntryKind::Message(message) => {
            let body = message.body.as_text().unwrap_or("");
            TranscriptRowPresentation {
                role: match message.role {
                    MessageRole::User => TranscriptEntryRole::User,
                    MessageRole::Assistant => TranscriptEntryRole::Assistant,
                    MessageRole::System => TranscriptEntryRole::System,
                    MessageRole::Tool | MessageRole::Other => TranscriptEntryRole::Auxiliary,
                },
                text: body.into(),
                visible: !(entry.status == EntryStatus::Complete && body.is_empty()),
                show_spinner: entry.status == EntryStatus::InProgress,
            }
        }
        EntryKind::ToolCall(call) => TranscriptRowPresentation {
            role: TranscriptEntryRole::Auxiliary,
            text: format!("[tool call: {}]", call.tool_name),
            visible: true,
            show_spinner: entry.status == EntryStatus::InProgress,
        },
        EntryKind::ToolResult(result) => {
            let output = result.output.as_text().unwrap_or("");
            let text = if output.is_empty() {
                format!("[tool result: {}]", result.tool_name)
            } else {
                format!("[tool result: {}] {}", result.tool_name, output)
            };
            TranscriptRowPresentation {
                role: TranscriptEntryRole::Auxiliary,
                text,
                visible: true,
                show_spinner: false,
            }
        }
        EntryKind::ProcessOutput(output) => TranscriptRowPresentation {
            role: TranscriptEntryRole::Auxiliary,
            text: output.body.as_text().unwrap_or("").into(),
            visible: true,
            show_spinner: entry.status == EntryStatus::InProgress,
        },
        EntryKind::Annotation(annotation) => {
            let prefix = match annotation.level {
                AnnotationLevel::Info => "[info]",
                AnnotationLevel::Warning => "[warning]",
                AnnotationLevel::Error => "[error]",
            };
            let body = annotation.body.as_text().unwrap_or("");
            TranscriptRowPresentation {
                role: TranscriptEntryRole::System,
                text: if body.is_empty() {
                    prefix.into()
                } else {
                    format!("{prefix} {body}")
                },
                visible: true,
                show_spinner: false,
            }
        }
        EntryKind::State(state) => {
            let body = state.body.as_text().unwrap_or("");
            TranscriptRowPresentation {
                role: TranscriptEntryRole::System,
                text: if body.is_empty() {
                    format!("[state: {}]", state.label)
                } else {
                    format!("[state: {}] {}", state.label, body)
                },
                visible: true,
                show_spinner: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use overstory::{ThemeKeys, default_theme};
    use understory_transcript::{MessageRole, NewEntry};

    #[test]
    fn sync_default_creates_user_row_with_message_style() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(overstory::peniko::kurbo::Rect::new(0.0, 0.0, 400.0, 200.0));
        let scroll = ui.append_child(ui.root(), overstory::TYPE_SCROLL_VIEW);
        let mut controller = TranscriptViewController::new(scroll);
        let mut transcript = Transcript::new();
        let entry = transcript.append(NewEntry::message(MessageRole::User, "hello"));

        controller.sync_default(&mut ui, &transcript);

        let row = controller.row_ids(entry).expect("row ids");
        assert_eq!(
            ui.display_name(row.text),
            Some("hello"),
            "text-bearing rows should expose their widget text through display_name"
        );
        let expected = *ui
            .theme()
            .get(ThemeKeys::CONTROL_BACKGROUND)
            .expect("user message background");
        let scene = ui.scene();
        let resolved = scene.resolved_element(row.text).expect("resolved row");
        assert_eq!(resolved.background, expected);
    }

    #[test]
    fn sync_default_shows_spinner_for_in_progress_message() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(overstory::peniko::kurbo::Rect::new(0.0, 0.0, 400.0, 200.0));
        let scroll = ui.append_child(ui.root(), overstory::TYPE_SCROLL_VIEW);
        let mut controller = TranscriptViewController::new(scroll);
        let mut transcript = Transcript::new();
        let entry = transcript.append(
            NewEntry::message(MessageRole::Assistant, "").with_status(EntryStatus::InProgress),
        );

        controller.sync_default(&mut ui, &transcript);

        let row = controller.row_ids(entry).expect("row ids");
        let scene = ui.scene();
        assert!(scene.resolved_element(row.spinner).is_some());
        assert!(scene.resolved_element(row.text).is_none());
    }
}
