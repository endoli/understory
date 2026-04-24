// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{boxed::Box, vec::Vec};

use overstory::ui_events::keyboard::{Key, KeyboardEvent, NamedKey};
use overstory::{Color, ElementId, Panel, TextBlock, Ui};

/// One projected list row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListRowPresentation<K> {
    /// Stable row key.
    pub key: K,
    /// Visible row label.
    pub label: Box<str>,
}

impl<K> ListRowPresentation<K> {
    /// Creates a new list row presentation.
    #[must_use]
    pub fn new(key: K, label: impl Into<Box<str>>) -> Self {
        Self {
            key,
            label: label.into(),
        }
    }
}

/// Element ids for one realized list row.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ListRowIds {
    /// Row background/container.
    pub row: ElementId,
    /// Label text.
    pub label: ElementId,
}

/// One realized list row in Overstory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListViewRealizedRow<K> {
    /// Row key in the underlying model.
    pub key: K,
    /// Whether this row is currently focused.
    pub focused: bool,
    /// Realized element ids.
    pub ids: ListRowIds,
}

/// Action produced by clicking one realized list row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListRowAction<K> {
    /// Activate/select the row.
    Select(K),
}

/// Keyboard/navigation action derived from the current realized list rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListKeyboardAction<K> {
    /// Move focus to another row.
    Focus(K),
    /// Activate/select the focused row.
    Activate(K),
}

/// Styling knobs for Overstory list rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ListViewStyle {
    /// Outer row padding on the background container.
    pub row_padding: f64,
    /// Corner radius for selected rows.
    pub row_corner_radius: f64,
    /// Font size for label text.
    pub font_size: f64,
    /// Label padding for the text block.
    pub label_padding: f64,
    /// Background for unselected rows.
    pub background: Color,
    /// Background for selected rows.
    pub selected_background: Color,
    /// Background for focused rows.
    pub focused_background: Color,
}

impl Default for ListViewStyle {
    fn default() -> Self {
        Self {
            row_padding: 1.0,
            row_corner_radius: 6.0,
            font_size: 11.0,
            label_padding: 4.0,
            background: Color::TRANSPARENT,
            selected_background: Color::TRANSPARENT,
            focused_background: Color::TRANSPARENT,
        }
    }
}

/// Reusable Overstory controller for linear list rows.
#[derive(Clone, Debug)]
pub struct ListViewController<K> {
    scroll_view: ElementId,
    style: ListViewStyle,
    rows: Vec<ListViewRealizedRow<K>>,
    selected_key: Option<K>,
    focused_key: Option<K>,
}

impl<K> ListViewController<K> {
    /// Creates a controller bound to one Overstory `ScrollView`.
    #[must_use]
    pub fn new(scroll_view: ElementId) -> Self {
        Self {
            scroll_view,
            style: ListViewStyle::default(),
            rows: Vec::new(),
            selected_key: None,
            focused_key: None,
        }
    }

    /// Returns the bound list scroll view.
    #[must_use]
    pub const fn scroll_view(&self) -> ElementId {
        self.scroll_view
    }

    /// Returns the current list style.
    #[must_use]
    pub const fn style(&self) -> &ListViewStyle {
        &self.style
    }

    /// Replaces the current list style.
    pub fn set_style(&mut self, style: ListViewStyle) {
        self.style = style;
    }

    /// Returns the currently realized rows.
    #[must_use]
    pub fn realized_rows(&self) -> &[ListViewRealizedRow<K>] {
        &self.rows
    }

    /// Returns the currently selected row key, if any.
    #[must_use]
    pub fn selected_key(&self) -> Option<&K> {
        self.selected_key.as_ref()
    }

    /// Returns the currently focused row key, if any.
    #[must_use]
    pub fn focused_key(&self) -> Option<&K> {
        self.focused_key.as_ref()
    }

    /// Replaces the currently selected row key.
    pub fn set_selected_key(&mut self, key: Option<K>) {
        self.selected_key = key;
    }

    /// Replaces the currently focused row key.
    pub fn set_focused_key(&mut self, key: Option<K>) {
        self.focused_key = key;
    }

    /// Focuses the first visible row and returns its key.
    pub fn focus_first_visible(&mut self) -> Option<K>
    where
        K: Clone,
    {
        let key = self.rows.first()?.key.clone();
        self.focused_key = Some(key.clone());
        Some(key)
    }

    /// Focuses the last visible row and returns its key.
    pub fn focus_last_visible(&mut self) -> Option<K>
    where
        K: Clone,
    {
        let key = self.rows.last()?.key.clone();
        self.focused_key = Some(key.clone());
        Some(key)
    }

    /// Focuses the previous visible row and returns its key.
    pub fn focus_prev_visible(&mut self) -> Option<K>
    where
        K: Clone + PartialEq,
    {
        let index = self.focused_index()?.checked_sub(1)?;
        let key = self.rows.get(index)?.key.clone();
        self.focused_key = Some(key.clone());
        Some(key)
    }

    /// Focuses the next visible row and returns its key.
    pub fn focus_next_visible(&mut self) -> Option<K>
    where
        K: Clone + PartialEq,
    {
        let index = self.focused_index()? + 1;
        let key = self.rows.get(index)?.key.clone();
        self.focused_key = Some(key.clone());
        Some(key)
    }

    /// Selects the focused row and returns its key.
    pub fn select_focused(&mut self) -> Option<K>
    where
        K: Clone + PartialEq,
    {
        let key = self.focused_key()?.clone();
        self.selected_key = Some(key.clone());
        Some(key)
    }

    /// Returns the focused row key and also marks it selected.
    pub fn activate_focused(&mut self) -> Option<K>
    where
        K: Clone + PartialEq,
    {
        self.select_focused()
    }

    /// Syncs projected list rows into Overstory.
    pub fn sync(&mut self, ui: &mut Ui, rows: &[ListRowPresentation<K>])
    where
        K: Clone + PartialEq,
    {
        while self.rows.len() < rows.len() {
            let key = rows[self.rows.len()].key.clone();
            self.rows.push(self.append_row(ui, key));
        }

        if !rows.is_empty() {
            let visible = |key: &K| rows.iter().any(|row| &row.key == key);
            if self.focused_key.as_ref().is_none_or(|key| !visible(key)) {
                self.focused_key = rows.first().map(|row| row.key.clone());
            }
            if self.selected_key.as_ref().is_some_and(|key| !visible(key)) {
                self.selected_key = None;
            }
        } else {
            self.focused_key = None;
            self.selected_key = None;
        }

        for (index, realized) in self.rows.iter_mut().enumerate() {
            if let Some(row) = rows.get(index) {
                realized.key = row.key.clone();
                realized.focused = self.focused_key.as_ref() == Some(&row.key);
                apply_row(
                    &self.style,
                    ui,
                    realized,
                    row,
                    self.selected_key.as_ref() == Some(&row.key),
                    realized.focused,
                );
            } else {
                hide_row(ui, realized);
            }
        }
    }

    /// Maps a clicked Overstory element back into a list row action.
    pub fn handle_click(&mut self, target: ElementId) -> Option<ListRowAction<K>>
    where
        K: Clone + PartialEq,
    {
        let realized = self
            .rows
            .iter()
            .find(|row| target == row.ids.row || target == row.ids.label)?;
        self.focused_key = Some(realized.key.clone());
        Some(ListRowAction::Select(self.select_focused()?))
    }

    /// Maps a keyboard event into a list navigation action using the current
    /// focused row and visible row order.
    pub fn handle_keyboard_event(&mut self, event: &KeyboardEvent) -> Option<ListKeyboardAction<K>>
    where
        K: Clone + PartialEq,
    {
        if !event.state.is_down() {
            return None;
        }
        self.focused_index()?;
        match &event.key {
            Key::Named(NamedKey::ArrowUp) => {
                self.focus_prev_visible().map(ListKeyboardAction::Focus)
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.focus_next_visible().map(ListKeyboardAction::Focus)
            }
            Key::Named(NamedKey::Home) => self.focus_first_visible().map(ListKeyboardAction::Focus),
            Key::Named(NamedKey::End) => self.focus_last_visible().map(ListKeyboardAction::Focus),
            Key::Named(NamedKey::Enter) => {
                self.activate_focused().map(ListKeyboardAction::Activate)
            }
            Key::Character(space) if &**space == " " => {
                self.activate_focused().map(ListKeyboardAction::Activate)
            }
            _ => None,
        }
    }

    fn focused_index(&self) -> Option<usize>
    where
        K: PartialEq,
    {
        self.focused_key
            .as_ref()
            .and_then(|key| self.rows.iter().position(|row| &row.key == key))
            .or_else(|| (!self.rows.is_empty()).then_some(0))
    }

    fn append_row(&self, ui: &mut Ui, key: K) -> ListViewRealizedRow<K> {
        let row = ui.append(
            self.scroll_view,
            Panel::new()
                .padding(self.style.row_padding)
                .corner_radius(self.style.row_corner_radius)
                .background(self.style.background),
        );
        ui.set_local(row, ui.properties().pickable, true);

        let label = ui.append(
            row,
            TextBlock::new()
                .fill()
                .label_padding(self.style.label_padding)
                .padding(0.0)
                .font_size(self.style.font_size)
                .background(Color::TRANSPARENT),
        );
        ui.set_local(label, ui.properties().pickable, true);

        ListViewRealizedRow {
            key,
            focused: false,
            ids: ListRowIds { row, label },
        }
    }
}

fn apply_row<K>(
    style: &ListViewStyle,
    ui: &mut Ui,
    realized: &ListViewRealizedRow<K>,
    row: &ListRowPresentation<K>,
    selected: bool,
    focused: bool,
) {
    ui.set_local(realized.ids.row, ui.properties().visible, true);
    ui.set_local(
        realized.ids.row,
        ui.properties().background,
        if selected {
            style.selected_background
        } else if focused {
            style.focused_background
        } else {
            style.background
        },
    );
    set_text_block_text(ui, realized.ids.label, row.label.as_ref());
}

fn hide_row<K>(ui: &mut Ui, realized: &ListViewRealizedRow<K>) {
    ui.set_local(realized.ids.row, ui.properties().visible, false);
    set_text_block_text(ui, realized.ids.label, "");
}

fn set_text_block_text(ui: &mut Ui, id: ElementId, text: impl Into<Box<str>>) {
    ui.widget_mut::<TextBlock>(id)
        .expect("list rows use text block children")
        .set_text(text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use overstory::ui_events::keyboard::Code;
    use overstory::{ScrollView, default_theme};

    #[test]
    fn clicking_row_selects_it() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut list = ListViewController::<u32>::new(scroll);
        list.sync(
            &mut ui,
            &[
                ListRowPresentation::new(1, "One"),
                ListRowPresentation::new(2, "Two"),
            ],
        );

        let ids = list.realized_rows()[1].ids;
        assert_eq!(list.handle_click(ids.label), Some(ListRowAction::Select(2)));
        assert_eq!(list.focused_key(), Some(&2));
        assert_eq!(list.selected_key(), Some(&2));
    }

    #[test]
    fn keyboard_navigation_updates_focus_and_selection() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut list = ListViewController::<u32>::new(scroll);
        list.sync(
            &mut ui,
            &[
                ListRowPresentation::new(1, "One"),
                ListRowPresentation::new(2, "Two"),
                ListRowPresentation::new(3, "Three"),
            ],
        );

        list.set_focused_key(Some(2));
        assert_eq!(
            list.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::ArrowUp),
                Code::ArrowUp
            )),
            Some(ListKeyboardAction::Focus(1))
        );
        list.set_focused_key(Some(2));
        assert_eq!(
            list.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::ArrowDown),
                Code::ArrowDown
            )),
            Some(ListKeyboardAction::Focus(3))
        );
        list.set_focused_key(Some(2));
        assert_eq!(
            list.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::Home),
                Code::Home
            )),
            Some(ListKeyboardAction::Focus(1))
        );
        list.set_focused_key(Some(2));
        assert_eq!(
            list.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::End),
                Code::End
            )),
            Some(ListKeyboardAction::Focus(3))
        );
        list.set_focused_key(Some(2));
        assert_eq!(
            list.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::Enter),
                Code::Enter
            )),
            Some(ListKeyboardAction::Activate(2))
        );
        assert_eq!(list.selected_key(), Some(&2));
    }

    #[test]
    fn composite_navigation_methods_update_state() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut list = ListViewController::<u32>::new(scroll);
        list.sync(
            &mut ui,
            &[
                ListRowPresentation::new(1, "One"),
                ListRowPresentation::new(2, "Two"),
                ListRowPresentation::new(3, "Three"),
            ],
        );

        assert_eq!(list.focused_key(), Some(&1));
        assert_eq!(list.focus_next_visible(), Some(2));
        assert_eq!(list.focus_prev_visible(), Some(1));
        assert_eq!(list.focus_last_visible(), Some(3));
        assert_eq!(list.focus_first_visible(), Some(1));
        assert_eq!(list.activate_focused(), Some(1));
        assert_eq!(list.selected_key(), Some(&1));
    }
}
