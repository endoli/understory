// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{boxed::Box, vec::Vec};

use overstory::ui_events::keyboard::{Key, KeyboardEvent, NamedKey};
use overstory::{Color, ElementId, Panel, Row, Spacer, TextBlock, Ui};

/// One projected tree row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRowPresentation<K> {
    /// Stable row key.
    pub key: K,
    /// Visible row label.
    pub label: Box<str>,
    /// Tree depth for indentation.
    pub depth: usize,
    /// Whether the row has children.
    pub has_children: bool,
    /// Whether the row is currently expanded.
    pub is_expanded: bool,
}

impl<K> TreeRowPresentation<K> {
    /// Creates a new tree row presentation.
    #[must_use]
    pub fn new(
        key: K,
        label: impl Into<Box<str>>,
        depth: usize,
        has_children: bool,
        is_expanded: bool,
    ) -> Self {
        Self {
            key,
            label: label.into(),
            depth,
            has_children,
            is_expanded,
        }
    }
}

/// Element ids for one realized tree row.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TreeRowIds {
    /// Row background/container.
    pub row: ElementId,
    /// Pickable inner row content.
    pub content: ElementId,
    /// Indent spacer.
    pub indent: ElementId,
    /// Disclosure glyph.
    pub disclosure: ElementId,
    /// Label text.
    pub label: ElementId,
}

/// One realized tree row in Overstory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeViewRealizedRow<K> {
    /// Row key in the underlying model.
    pub key: K,
    /// Tree depth for parent/child keyboard navigation.
    pub depth: usize,
    /// Whether this row currently has a disclosure affordance.
    pub can_toggle: bool,
    /// Whether this row is currently expanded.
    pub is_expanded: bool,
    /// Whether this row is currently focused.
    pub focused: bool,
    /// Realized element ids.
    pub ids: TreeRowIds,
}

/// Action produced by clicking one realized tree row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeRowAction<K> {
    /// Activate/select the row.
    Select(K),
    /// Toggle the row's disclosure state.
    Toggle(K),
}

/// Keyboard/navigation action derived from the current realized tree rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeKeyboardAction<K> {
    /// Move focus to another row.
    Focus(K),
    /// Activate/select the focused row.
    Activate(K),
    /// Expand the focused row.
    Expand(K),
    /// Collapse the focused row.
    Collapse(K),
}

/// Styling knobs for Overstory tree rows.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeViewStyle {
    /// Outer row padding on the background container.
    pub row_padding: f64,
    /// Inter-child gap inside the content row.
    pub row_gap: f64,
    /// Corner radius for selected rows.
    pub row_corner_radius: f64,
    /// Font size for disclosure and label text.
    pub font_size: f64,
    /// Label padding for disclosure and label blocks.
    pub label_padding: f64,
    /// Horizontal indentation per depth level.
    pub indent_width: f64,
    /// Reserved width for the disclosure slot.
    pub disclosure_width: f64,
    /// Background for unselected rows.
    pub background: Color,
    /// Background for selected rows.
    pub selected_background: Color,
    /// Background for focused rows.
    pub focused_background: Color,
}

impl Default for TreeViewStyle {
    fn default() -> Self {
        Self {
            row_padding: 1.0,
            row_gap: 6.0,
            row_corner_radius: 6.0,
            font_size: 11.0,
            label_padding: 2.0,
            indent_width: 14.0,
            disclosure_width: 16.0,
            background: Color::TRANSPARENT,
            selected_background: Color::TRANSPARENT,
            focused_background: Color::TRANSPARENT,
        }
    }
}

/// Reusable Overstory controller for hierarchical tree rows.
#[derive(Clone, Debug)]
pub struct TreeViewController<K> {
    scroll_view: ElementId,
    style: TreeViewStyle,
    rows: Vec<TreeViewRealizedRow<K>>,
    selected_key: Option<K>,
    focused_key: Option<K>,
}

impl<K> TreeViewController<K> {
    /// Creates a controller bound to one Overstory `ScrollView`.
    #[must_use]
    pub fn new(scroll_view: ElementId) -> Self {
        Self {
            scroll_view,
            style: TreeViewStyle::default(),
            rows: Vec::new(),
            selected_key: None,
            focused_key: None,
        }
    }

    /// Returns the bound tree scroll view.
    #[must_use]
    pub const fn scroll_view(&self) -> ElementId {
        self.scroll_view
    }

    /// Returns the current tree style.
    #[must_use]
    pub const fn style(&self) -> &TreeViewStyle {
        &self.style
    }

    /// Replaces the current tree style.
    pub fn set_style(&mut self, style: TreeViewStyle) {
        self.style = style;
    }

    /// Returns the currently realized rows.
    #[must_use]
    pub fn realized_rows(&self) -> &[TreeViewRealizedRow<K>] {
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

    /// Focuses the parent of the focused row and returns its key.
    pub fn focus_parent(&mut self) -> Option<K>
    where
        K: Clone + PartialEq,
    {
        let focused_index = self.focused_index()?;
        let key = self.parent_row(focused_index)?.key.clone();
        self.focused_key = Some(key.clone());
        Some(key)
    }

    /// Focuses the first visible child of the focused row and returns its key.
    pub fn focus_first_child(&mut self) -> Option<K>
    where
        K: Clone + PartialEq,
    {
        let focused_index = self.focused_index()?;
        let focused = self.rows.get(focused_index)?;
        let row = self.rows.get(focused_index + 1)?;
        if row.depth != focused.depth + 1 {
            return None;
        }
        let key = row.key.clone();
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

    /// Returns the focused row key if it should be expanded.
    pub fn expand_focused(&self) -> Option<K>
    where
        K: Clone + PartialEq,
    {
        let focused = self.focused_row()?;
        (focused.can_toggle && !focused.is_expanded).then(|| focused.key.clone())
    }

    /// Returns the focused row key if it should be collapsed.
    pub fn collapse_focused(&self) -> Option<K>
    where
        K: Clone + PartialEq,
    {
        let focused = self.focused_row()?;
        (focused.can_toggle && focused.is_expanded).then(|| focused.key.clone())
    }

    /// Returns the focused row key if its disclosure should toggle.
    pub fn toggle_focused_disclosure(&self) -> Option<K>
    where
        K: Clone + PartialEq,
    {
        let focused = self.focused_row()?;
        focused.can_toggle.then(|| focused.key.clone())
    }

    /// Syncs projected tree rows into Overstory.
    pub fn sync(&mut self, ui: &mut Ui, rows: &[TreeRowPresentation<K>])
    where
        K: Clone + PartialEq,
    {
        while self.rows.len() < rows.len() {
            let key = rows[self.rows.len()].key.clone();
            self.rows.push(self.append_row(ui, key));
        }

        for (index, realized) in self.rows.iter_mut().enumerate() {
            if let Some(row) = rows.get(index) {
                realized.key = row.key.clone();
                realized.depth = row.depth;
                realized.can_toggle = row.has_children;
                realized.is_expanded = row.is_expanded;
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
                realized.focused = self.focused_key.as_ref() == Some(&row.key);
                apply_row(
                    &self.style,
                    ui,
                    realized,
                    row,
                    self.selected_key.as_ref() == Some(&row.key),
                    realized.focused,
                );
            }
        }
    }

    /// Maps a clicked Overstory element back into a tree row action.
    pub fn handle_click(&mut self, target: ElementId) -> Option<TreeRowAction<K>>
    where
        K: Clone + PartialEq,
    {
        let realized = self.rows.iter().find(|row| {
            let ids = row.ids;
            target == ids.content || target == ids.label || target == ids.disclosure
        })?;
        self.focused_key = Some(realized.key.clone());
        if target == realized.ids.disclosure && realized.can_toggle {
            Some(TreeRowAction::Toggle(realized.key.clone()))
        } else {
            Some(TreeRowAction::Select(self.select_focused()?))
        }
    }

    /// Maps a keyboard event into a tree navigation action using the current
    /// focused row and visible row order.
    pub fn handle_keyboard_event(&mut self, event: &KeyboardEvent) -> Option<TreeKeyboardAction<K>>
    where
        K: Clone + PartialEq,
    {
        if !event.state.is_down() {
            return None;
        }
        self.focused_index()?;
        match &event.key {
            Key::Named(NamedKey::ArrowUp) => {
                self.focus_prev_visible().map(TreeKeyboardAction::Focus)
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.focus_next_visible().map(TreeKeyboardAction::Focus)
            }
            Key::Named(NamedKey::Home) => self.focus_first_visible().map(TreeKeyboardAction::Focus),
            Key::Named(NamedKey::End) => self.focus_last_visible().map(TreeKeyboardAction::Focus),
            Key::Named(NamedKey::ArrowRight) => {
                if let Some(key) = self.expand_focused() {
                    Some(TreeKeyboardAction::Expand(key))
                } else {
                    self.focus_first_child().map(TreeKeyboardAction::Focus)
                }
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if let Some(key) = self.collapse_focused() {
                    Some(TreeKeyboardAction::Collapse(key))
                } else {
                    self.focus_parent().map(TreeKeyboardAction::Focus)
                }
            }
            Key::Named(NamedKey::Enter) => {
                self.activate_focused().map(TreeKeyboardAction::Activate)
            }
            Key::Character(space) if &**space == " " => {
                self.activate_focused().map(TreeKeyboardAction::Activate)
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

    fn focused_row(&self) -> Option<&TreeViewRealizedRow<K>>
    where
        K: PartialEq,
    {
        self.rows.get(self.focused_index()?)
    }

    fn parent_row(&self, focused_index: usize) -> Option<&TreeViewRealizedRow<K>> {
        let focused = self.rows.get(focused_index)?;
        let parent_depth = focused.depth.checked_sub(1)?;
        self.rows[..focused_index]
            .iter()
            .rev()
            .find(|row| row.depth == parent_depth)
    }

    fn append_row(&self, ui: &mut Ui, key: K) -> TreeViewRealizedRow<K> {
        let row = ui.append(
            self.scroll_view,
            Panel::new()
                .padding(0.0)
                .corner_radius(self.style.row_corner_radius)
                .background(self.style.background),
        );

        let content = ui.append(
            row,
            Row::new()
                .fill()
                .padding(self.style.row_padding)
                .gap(self.style.row_gap)
                .background(Color::TRANSPARENT),
        );
        ui.set_local(content, ui.properties().pickable, true);

        let indent = ui.append(content, Spacer::new().width(0.0));
        let disclosure = ui.append(
            content,
            TextBlock::new()
                .label_padding(self.style.label_padding)
                .padding(0.0)
                .font_size(self.style.font_size)
                .background(Color::TRANSPARENT),
        );
        ui.set_local(
            disclosure,
            ui.properties().width,
            self.style.disclosure_width,
        );
        ui.set_local(disclosure, ui.properties().pickable, true);

        let label = ui.append(
            content,
            TextBlock::new()
                .fill()
                .label_padding(self.style.label_padding)
                .padding(0.0)
                .font_size(self.style.font_size)
                .background(Color::TRANSPARENT),
        );
        ui.set_local(label, ui.properties().pickable, true);

        TreeViewRealizedRow {
            key,
            depth: 0,
            can_toggle: false,
            is_expanded: false,
            focused: false,
            ids: TreeRowIds {
                row,
                content,
                indent,
                disclosure,
                label,
            },
        }
    }
}

fn apply_row<K>(
    style: &TreeViewStyle,
    ui: &mut Ui,
    realized: &TreeViewRealizedRow<K>,
    row: &TreeRowPresentation<K>,
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
    ui.set_local(
        realized.ids.indent,
        ui.properties().width,
        row.depth as f64 * style.indent_width,
    );
    set_text_block_text(
        ui,
        realized.ids.disclosure,
        if row.has_children {
            if row.is_expanded { "▾" } else { "▸" }
        } else {
            ""
        },
    );
    set_text_block_text(ui, realized.ids.label, row.label.as_ref());
}

fn hide_row<K>(ui: &mut Ui, realized: &TreeViewRealizedRow<K>) {
    ui.set_local(realized.ids.row, ui.properties().visible, false);
    set_text_block_text(ui, realized.ids.disclosure, "");
    set_text_block_text(ui, realized.ids.label, "");
}

fn set_text_block_text(ui: &mut Ui, id: ElementId, text: impl Into<Box<str>>) {
    ui.widget_mut::<TextBlock>(id)
        .expect("tree rows use text block children")
        .set_text(text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use overstory::ui_events::keyboard::Code;
    use overstory::{ScrollView, default_theme};

    #[test]
    fn disclosure_and_label_clicks_map_to_distinct_actions() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut tree = TreeViewController::<u32>::new(scroll);
        tree.sync(
            &mut ui,
            &[TreeRowPresentation::new(7, "Node", 0, true, false)],
        );

        let ids = tree.realized_rows()[0].ids;
        assert_eq!(
            tree.handle_click(ids.disclosure),
            Some(TreeRowAction::Toggle(7))
        );
        assert_eq!(tree.handle_click(ids.label), Some(TreeRowAction::Select(7)));
        assert_eq!(
            tree.handle_click(ids.content),
            Some(TreeRowAction::Select(7))
        );
    }

    #[test]
    fn selected_rows_apply_selected_background() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut tree = TreeViewController::<u32>::new(scroll);
        let mut style = tree.style().clone();
        style.selected_background = Color::from_rgba8(1, 2, 3, 255);
        tree.set_style(style);
        tree.set_selected_key(Some(1));
        tree.sync(
            &mut ui,
            &[TreeRowPresentation::new(1, "Selected", 1, false, false)],
        );

        let resolved = ui
            .scene()
            .resolved_element(tree.realized_rows()[0].ids.row)
            .expect("tree row should be visible");
        assert_eq!(resolved.background, Color::from_rgba8(1, 2, 3, 255));
    }

    #[test]
    fn keyboard_navigation_maps_from_focused_row() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut tree = TreeViewController::<u32>::new(scroll);
        tree.set_focused_key(Some(2));
        tree.sync(
            &mut ui,
            &[
                TreeRowPresentation::new(1, "One", 0, true, false),
                TreeRowPresentation::new(2, "Two", 1, false, false),
                TreeRowPresentation::new(3, "Three", 1, false, false),
            ],
        );

        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::ArrowUp),
                Code::ArrowUp
            )),
            Some(TreeKeyboardAction::Focus(1))
        );
        tree.set_focused_key(Some(2));
        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::ArrowDown),
                Code::ArrowDown
            )),
            Some(TreeKeyboardAction::Focus(3))
        );
        tree.set_focused_key(Some(2));
        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::Home),
                Code::Home
            )),
            Some(TreeKeyboardAction::Focus(1))
        );
        tree.set_focused_key(Some(2));
        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::End),
                Code::End
            )),
            Some(TreeKeyboardAction::Focus(3))
        );
        tree.set_focused_key(Some(2));
        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::Enter),
                Code::Enter
            )),
            Some(TreeKeyboardAction::Activate(2))
        );
        assert_eq!(tree.selected_key(), Some(&2));
    }

    #[test]
    fn horizontal_arrows_use_tree_structure() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut tree = TreeViewController::<u32>::new(scroll);
        tree.set_focused_key(Some(1));
        tree.sync(
            &mut ui,
            &[
                TreeRowPresentation::new(1, "Root", 0, true, true),
                TreeRowPresentation::new(2, "Child", 1, false, false),
                TreeRowPresentation::new(3, "Sibling", 0, true, false),
            ],
        );

        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::ArrowRight),
                Code::ArrowRight
            )),
            Some(TreeKeyboardAction::Focus(2))
        );

        tree.set_focused_key(Some(2));
        tree.sync(
            &mut ui,
            &[
                TreeRowPresentation::new(1, "Root", 0, true, true),
                TreeRowPresentation::new(2, "Child", 1, false, false),
                TreeRowPresentation::new(3, "Sibling", 0, true, false),
            ],
        );
        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::ArrowLeft),
                Code::ArrowLeft
            )),
            Some(TreeKeyboardAction::Focus(1))
        );
    }

    #[test]
    fn composite_navigation_methods_update_state() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut tree = TreeViewController::<u32>::new(scroll);
        tree.sync(
            &mut ui,
            &[
                TreeRowPresentation::new(1, "Root", 0, true, true),
                TreeRowPresentation::new(2, "Child", 1, false, false),
                TreeRowPresentation::new(3, "Sibling", 0, false, false),
            ],
        );

        assert_eq!(tree.focused_key(), Some(&1));
        assert_eq!(tree.focus_first_child(), Some(2));
        assert_eq!(tree.focused_key(), Some(&2));
        assert_eq!(tree.focus_parent(), Some(1));
        assert_eq!(tree.focused_key(), Some(&1));
        assert_eq!(tree.focus_next_visible(), Some(2));
        assert_eq!(tree.focus_last_visible(), Some(3));
        assert_eq!(tree.focus_prev_visible(), Some(2));
        assert_eq!(tree.focus_first_visible(), Some(1));
        assert_eq!(tree.activate_focused(), Some(1));
        assert_eq!(tree.selected_key(), Some(&1));
        assert_eq!(tree.expand_focused(), None);
        assert_eq!(tree.toggle_focused_disclosure(), Some(1));
    }
}
