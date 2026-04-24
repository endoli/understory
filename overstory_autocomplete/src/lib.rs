// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Type-ahead autocomplete surface integration for Overstory.

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};

use overstory::ui_events::keyboard::{Key, KeyboardEvent, NamedKey};
use overstory::{Color, Dropdown, ElementId, ScrollView, TextInput, ThemeKeys, Ui};
use overstory_list::{
    ListKeyboardAction, ListRowAction, ListRowPresentation, ListViewController, ListViewStyle,
};

/// Stable element ids for one mounted autocomplete field.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AutocompleteIds {
    /// Text input element.
    pub input: ElementId,
    /// Promoted dropdown surface root.
    pub dropdown: ElementId,
    /// Scroll view inside the dropdown.
    pub scroll_view: ElementId,
}

/// One visible autocomplete option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutocompleteOption<K> {
    /// Stable option key.
    pub key: K,
    /// Visible option label.
    pub label: Box<str>,
    /// Text committed into the input on acceptance.
    pub commit_text: Box<str>,
}

impl<K> AutocompleteOption<K> {
    /// Creates one autocomplete option whose committed text matches its label.
    #[must_use]
    pub fn new(key: K, label: impl Into<Box<str>>) -> Self {
        let label = label.into();
        Self {
            key,
            commit_text: label.clone(),
            label,
        }
    }

    /// Replaces the committed input text.
    #[must_use]
    pub fn commit_text(mut self, commit_text: impl Into<Box<str>>) -> Self {
        self.commit_text = commit_text.into();
        self
    }
}

/// Style knobs for the autocomplete popup.
#[derive(Clone, Debug, PartialEq)]
pub struct AutocompleteStyle {
    /// Debounce interval in nanoseconds.
    pub debounce_nanos: u64,
    /// Vertical gap between the input rect and dropdown origin.
    pub dropdown_gap: f64,
    /// Maximum popup height before scroll takes over.
    pub max_popup_height: f64,
    /// Popup corner radius.
    pub popup_corner_radius: f64,
    /// Popup border width.
    pub popup_border_width: f64,
    /// Row styling delegated to `overstory_list`.
    pub row_style: ListViewStyle,
}

impl Default for AutocompleteStyle {
    fn default() -> Self {
        Self {
            debounce_nanos: 150_000_000,
            dropdown_gap: 4.0,
            max_popup_height: 180.0,
            popup_corner_radius: 8.0,
            popup_border_width: 1.0,
            row_style: ListViewStyle {
                row_padding: 1.0,
                row_corner_radius: 6.0,
                font_size: 14.0,
                label_padding: 6.0,
                background: Color::TRANSPARENT,
                selected_background: Color::TRANSPARENT,
                focused_background: Color::TRANSPARENT,
            },
        }
    }
}

/// High-level action emitted by the autocomplete controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutocompleteAction<K> {
    /// The debounced query is ready for refresh.
    RefreshQuery(Box<str>),
    /// One option was accepted.
    Accepted(K),
    /// The popup was dismissed without acceptance.
    Dismissed,
}

/// Result of routing one keyboard event through autocomplete first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutocompleteOutcome<K> {
    /// Whether the controller consumed the keyboard event.
    pub consumed: bool,
    /// Optional higher-level action produced by that handling.
    pub action: Option<AutocompleteAction<K>>,
}

/// Reusable autocomplete field controller.
#[derive(Clone, Debug)]
pub struct AutocompleteController<K> {
    ids: AutocompleteIds,
    style: AutocompleteStyle,
    list: ListViewController<K>,
    options: Vec<AutocompleteOption<K>>,
    last_query: Box<str>,
    pending_due_at: Option<u64>,
}

impl<K> AutocompleteController<K> {
    /// Appends a single-line text input plus promoted dropdown surface.
    #[must_use]
    pub fn append(ui: &mut Ui, parent: ElementId, input: TextInput) -> Self
    where
        K: Clone + PartialEq,
    {
        let style = themed_style(ui);
        let input = ui.append(parent, input.single_line());
        let dropdown = ui.append(
            ui.root(),
            Dropdown::new(input)
                .padding(0.0)
                .gap(0.0)
                .background(
                    *ui.theme()
                        .get(ThemeKeys::SURFACE_BACKGROUND)
                        .unwrap_or(&Color::WHITE),
                )
                .border_width(style.popup_border_width)
                .corner_radius(style.popup_corner_radius)
                .display_name("Autocomplete dropdown"),
        );
        let scroll_view = ui.append(
            dropdown,
            ScrollView::new()
                .height(style.max_popup_height)
                .padding(0.0)
                .gap(0.0)
                .background(Color::TRANSPARENT),
        );
        ui.set_dropdown_open(dropdown, false);
        let mut list = ListViewController::new(scroll_view);
        list.set_style(style.row_style.clone());
        Self {
            ids: AutocompleteIds {
                input,
                dropdown,
                scroll_view,
            },
            style,
            list,
            options: Vec::new(),
            last_query: Box::from(""),
            pending_due_at: None,
        }
    }

    /// Returns the realized element ids for this field.
    #[must_use]
    pub const fn ids(&self) -> AutocompleteIds {
        self.ids
    }

    /// Returns the current style.
    #[must_use]
    pub const fn style(&self) -> &AutocompleteStyle {
        &self.style
    }

    /// Replaces the autocomplete style.
    pub fn set_style(&mut self, ui: &mut Ui, style: AutocompleteStyle)
    where
        K: Clone + PartialEq,
    {
        self.style = style;
        self.list.set_style(self.style.row_style.clone());
        self.sync_view(ui);
    }

    /// Returns the next debounce deadline in host monotonic nanoseconds.
    #[must_use]
    pub const fn next_deadline(&self) -> Option<u64> {
        self.pending_due_at
    }

    /// Returns whether the dropdown is currently open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        !self.options.is_empty()
    }

    /// Returns the currently focused option key, if any.
    #[must_use]
    pub fn focused_key(&self) -> Option<&K> {
        self.list.focused_key()
    }

    /// Returns the number of visible options.
    #[must_use]
    pub fn option_count(&self) -> usize {
        self.options.len()
    }

    /// Synchronizes the current input text and schedules debounce if it changed.
    pub fn sync_query_from_input(&mut self, ui: &mut Ui, now: u64)
    where
        K: Clone + PartialEq,
    {
        if ui.focused_element() != Some(self.ids.input) {
            self.pending_due_at = None;
            self.dismiss(ui);
            return;
        }
        let query = Box::<str>::from(ui.text_buffer(self.ids.input));
        if query == self.last_query {
            return;
        }
        self.last_query = query.clone();
        if query.is_empty() {
            self.pending_due_at = None;
            self.dismiss(ui);
            return;
        }
        self.pending_due_at = Some(now.saturating_add(self.style.debounce_nanos));
    }

    /// Returns a debounced query refresh request when due.
    pub fn take_due_query(&mut self, now: u64) -> Option<AutocompleteAction<K>> {
        match self.pending_due_at {
            Some(deadline) if deadline <= now => {
                self.pending_due_at = None;
                Some(AutocompleteAction::RefreshQuery(self.last_query.clone()))
            }
            _ => None,
        }
    }

    /// Replaces the currently visible options and synchronizes the popup.
    pub fn set_options(&mut self, ui: &mut Ui, options: &[AutocompleteOption<K>])
    where
        K: Clone + PartialEq,
    {
        self.options.clear();
        self.options.extend_from_slice(options);
        self.sync_view(ui);
    }

    /// Dismisses the popup and clears any visible options.
    pub fn dismiss(&mut self, ui: &mut Ui)
    where
        K: Clone + PartialEq,
    {
        self.options.clear();
        self.list.set_focused_key(None);
        self.list.set_selected_key(None);
        self.sync_view(ui);
    }

    /// Handles autocomplete keyboard routing while the input retains focus.
    #[must_use]
    pub fn handle_keyboard_event(
        &mut self,
        ui: &mut Ui,
        event: &KeyboardEvent,
    ) -> AutocompleteOutcome<K>
    where
        K: Clone + PartialEq,
    {
        if ui.focused_element() != Some(self.ids.input) || self.options.is_empty() {
            return AutocompleteOutcome {
                consumed: false,
                action: None,
            };
        }
        if !event.state.is_down() {
            return AutocompleteOutcome {
                consumed: false,
                action: None,
            };
        }
        if matches!(&event.key, Key::Named(NamedKey::Escape)) {
            self.dismiss(ui);
            return AutocompleteOutcome {
                consumed: true,
                action: Some(AutocompleteAction::Dismissed),
            };
        }
        match self.list.handle_keyboard_event(event) {
            Some(ListKeyboardAction::Focus(_)) => {
                self.sync_view(ui);
                self.scroll_focused_row_into_view(ui);
                AutocompleteOutcome {
                    consumed: true,
                    action: None,
                }
            }
            Some(ListKeyboardAction::Activate(key)) => AutocompleteOutcome {
                consumed: true,
                action: self.accept_key(ui, &key),
            },
            None => AutocompleteOutcome {
                consumed: false,
                action: None,
            },
        }
    }

    /// Handles click/focus interactions after `Ui` processed pointer input.
    pub fn handle_interactions(
        &mut self,
        ui: &mut Ui,
        interactions: &[overstory::Interaction],
    ) -> Option<AutocompleteAction<K>>
    where
        K: Clone + PartialEq,
    {
        for interaction in interactions {
            match *interaction {
                overstory::Interaction::Clicked(target) => {
                    if let Some(ListRowAction::Select(key)) = self.list.handle_click(target) {
                        return self.accept_key(ui, &key);
                    }
                    if target != self.ids.input {
                        self.dismiss(ui);
                        return Some(AutocompleteAction::Dismissed);
                    }
                }
                overstory::Interaction::FocusChanged(target) if target != self.ids.input => {
                    self.dismiss(ui);
                    return Some(AutocompleteAction::Dismissed);
                }
                _ => {}
            }
        }
        None
    }

    fn accept_key(&mut self, ui: &mut Ui, key: &K) -> Option<AutocompleteAction<K>>
    where
        K: Clone + PartialEq,
    {
        let option = self
            .options
            .iter()
            .find(|option| &option.key == key)?
            .clone();
        ui.set_text_buffer(self.ids.input, &option.commit_text);
        ui.set_focus(self.ids.input);
        self.last_query = option.commit_text;
        self.pending_due_at = None;
        self.dismiss(ui);
        Some(AutocompleteAction::Accepted(option.key))
    }

    fn sync_view(&mut self, ui: &mut Ui)
    where
        K: Clone + PartialEq,
    {
        let rows: Vec<_> = self
            .options
            .iter()
            .map(|option| ListRowPresentation::new(option.key.clone(), option.label.clone()))
            .collect();
        self.list.sync(ui, &rows);

        if rows.is_empty() {
            ui.set_dropdown_open(self.ids.dropdown, false);
            return;
        }

        let input_rect = ui
            .scene()
            .resolved_element(self.ids.input)
            .expect("autocomplete input should resolve")
            .rect;
        ui.set_local(
            self.ids.dropdown,
            ui.properties().width,
            input_rect.width().max(0.0),
        );
        ui.set_dropdown_position(
            self.ids.dropdown,
            input_rect.x0,
            input_rect.y1 + self.style.dropdown_gap,
        );
        ui.set_local(
            self.ids.scroll_view,
            ui.properties().height,
            self.popup_height(rows.len()),
        );
        ui.set_dropdown_open(self.ids.dropdown, true);
    }

    fn popup_height(&self, row_count: usize) -> f64 {
        let row_height =
            self.style.row_style.font_size * 1.4 + self.style.row_style.label_padding * 2.0;
        (row_count as f64 * row_height).min(self.style.max_popup_height)
    }

    fn scroll_focused_row_into_view(&mut self, ui: &mut Ui)
    where
        K: Clone + PartialEq,
    {
        let Some(key) = self.list.focused_key() else {
            return;
        };
        let Some(realized) = self.list.realized_rows().iter().find(|row| &row.key == key) else {
            return;
        };
        let scene = ui.scene();
        let Some(row_rect) = scene
            .resolved_element(realized.ids.row)
            .map(|resolved| resolved.rect)
        else {
            return;
        };
        let Some(viewport_rect) = scene
            .resolved_element(self.ids.scroll_view)
            .map(|resolved| resolved.rect)
        else {
            return;
        };
        let row_top = row_rect.y0;
        let row_bottom = row_rect.y1;
        let viewport_top = viewport_rect.y0;
        let viewport_bottom = viewport_rect.y1;
        let current = ui.scroll_offset(self.ids.scroll_view);
        if row_top < viewport_top {
            ui.set_scroll_offset(self.ids.scroll_view, current - (viewport_top - row_top));
        } else if row_bottom > viewport_bottom {
            ui.set_scroll_offset(
                self.ids.scroll_view,
                current + (row_bottom - viewport_bottom),
            );
        }
    }
}

fn themed_style(ui: &Ui) -> AutocompleteStyle {
    let mut style = AutocompleteStyle::default();
    style.row_style.selected_background = *ui
        .theme()
        .get(ThemeKeys::CONTROL_BACKGROUND_STRONG)
        .unwrap_or(&Color::TRANSPARENT);
    style.row_style.focused_background = *ui
        .theme()
        .get(ThemeKeys::CONTROL_BACKGROUND_EMPHASIZED)
        .unwrap_or(&Color::TRANSPARENT);
    style
}

#[cfg(test)]
mod tests {
    use super::*;
    use overstory::ui_events::keyboard::{Code, KeyboardEvent};
    use overstory::{Column, default_theme};

    fn mount_controller() -> (Ui, AutocompleteController<u32>) {
        let mut ui = Ui::new(default_theme());
        let column = ui.append(ui.root(), Column::new().padding(0.0).gap(8.0));
        let controller = AutocompleteController::append(&mut ui, column, TextInput::new(16.0));
        ui.set_focus(controller.ids().input);
        (ui, controller)
    }

    #[test]
    fn debounced_query_refreshes_from_input_text() {
        let (mut ui, mut controller) = mount_controller();
        ui.set_text_buffer(controller.ids().input, "de");
        controller.sync_query_from_input(&mut ui, 10);
        assert_eq!(controller.take_due_query(100), None);
        assert_eq!(
            controller.take_due_query(10 + controller.style().debounce_nanos),
            Some(AutocompleteAction::RefreshQuery(Box::from("de")))
        );
    }

    #[test]
    fn keyboard_accepts_focused_option_into_input() {
        let (mut ui, mut controller) = mount_controller();
        ui.set_text_buffer(controller.ids().input, "de");
        controller.set_options(
            &mut ui,
            &[
                AutocompleteOption::new(1, "Deploy"),
                AutocompleteOption::new(2, "Debug"),
            ],
        );

        let down = KeyboardEvent::key_down(Key::Named(NamedKey::ArrowDown), Code::ArrowDown);
        let enter = KeyboardEvent::key_down(Key::Named(NamedKey::Enter), Code::Enter);
        let outcome = controller.handle_keyboard_event(&mut ui, &down);
        assert!(outcome.consumed);
        let outcome = controller.handle_keyboard_event(&mut ui, &enter);
        assert_eq!(outcome.action, Some(AutocompleteAction::Accepted(2)));
        assert_eq!(ui.text_buffer(controller.ids().input), "Debug");
        assert!(!controller.is_open());
    }

    #[test]
    fn clicking_row_accepts_option() {
        let (mut ui, mut controller) = mount_controller();
        controller.set_options(
            &mut ui,
            &[AutocompleteOption::new(7, "Search").commit_text("search")],
        );
        let target = controller.list.realized_rows()[0].ids.label;
        let action =
            controller.handle_interactions(&mut ui, &[overstory::Interaction::Clicked(target)]);
        assert_eq!(action, Some(AutocompleteAction::Accepted(7)));
        assert_eq!(ui.text_buffer(controller.ids().input), "search");
    }
}
