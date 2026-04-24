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
    /// Whether the row is currently selected.
    pub selected: bool,
    /// Whether the row is currently focused for keyboard navigation.
    pub focused: bool,
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
        selected: bool,
        focused: bool,
    ) -> Self {
        Self {
            key,
            label: label.into(),
            depth,
            has_children,
            is_expanded,
            selected,
            focused,
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
}

impl<K> TreeViewController<K> {
    /// Creates a controller bound to one Overstory `ScrollView`.
    #[must_use]
    pub fn new(scroll_view: ElementId) -> Self {
        Self {
            scroll_view,
            style: TreeViewStyle::default(),
            rows: Vec::new(),
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

    /// Syncs projected tree rows into Overstory.
    pub fn sync(&mut self, ui: &mut Ui, rows: &[TreeRowPresentation<K>])
    where
        K: Clone,
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
                realized.focused = row.focused;
                apply_row(&self.style, ui, realized, row);
            } else {
                hide_row(ui, realized);
            }
        }
    }

    /// Maps a clicked Overstory element back into a tree row action.
    pub fn handle_click(&self, target: ElementId) -> Option<TreeRowAction<K>>
    where
        K: Clone,
    {
        let realized = self.rows.iter().find(|row| {
            let ids = row.ids;
            target == ids.content || target == ids.label || target == ids.disclosure
        })?;
        if target == realized.ids.disclosure && realized.can_toggle {
            Some(TreeRowAction::Toggle(realized.key.clone()))
        } else {
            Some(TreeRowAction::Select(realized.key.clone()))
        }
    }

    /// Maps a keyboard event into a tree navigation action using the current
    /// focused row and visible row order.
    pub fn handle_keyboard_event(&self, event: &KeyboardEvent) -> Option<TreeKeyboardAction<K>>
    where
        K: Clone,
    {
        if !event.state.is_down() {
            return None;
        }
        let focused_index = self.rows.iter().position(|row| row.focused)?;
        let focused = self.rows.get(focused_index)?;
        let parent = self.parent_row(focused_index);
        match &event.key {
            Key::Named(NamedKey::ArrowUp) => focused_index
                .checked_sub(1)
                .and_then(|index| self.rows.get(index))
                .map(|row| TreeKeyboardAction::Focus(row.key.clone())),
            Key::Named(NamedKey::ArrowDown) => self
                .rows
                .get(focused_index + 1)
                .map(|row| TreeKeyboardAction::Focus(row.key.clone())),
            Key::Named(NamedKey::Home) => self
                .rows
                .first()
                .map(|row| TreeKeyboardAction::Focus(row.key.clone())),
            Key::Named(NamedKey::End) => self
                .rows
                .last()
                .map(|row| TreeKeyboardAction::Focus(row.key.clone())),
            Key::Named(NamedKey::ArrowRight) => {
                if focused.can_toggle && !focused.is_expanded {
                    Some(TreeKeyboardAction::Expand(focused.key.clone()))
                } else {
                    self.rows.get(focused_index + 1).and_then(|row| {
                        (row.depth == focused.depth + 1)
                            .then(|| TreeKeyboardAction::Focus(row.key.clone()))
                    })
                }
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if focused.can_toggle && focused.is_expanded {
                    Some(TreeKeyboardAction::Collapse(focused.key.clone()))
                } else {
                    parent.map(|row| TreeKeyboardAction::Focus(row.key.clone()))
                }
            }
            Key::Named(NamedKey::Enter) => Some(TreeKeyboardAction::Activate(focused.key.clone())),
            Key::Character(space) if &**space == " " => {
                Some(TreeKeyboardAction::Activate(focused.key.clone()))
            }
            _ => None,
        }
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
) {
    ui.set_local(realized.ids.row, ui.properties().visible, true);
    ui.set_local(
        realized.ids.row,
        ui.properties().background,
        if row.selected {
            style.selected_background
        } else if row.focused {
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
            &[TreeRowPresentation::new(
                7, "Node", 0, true, false, false, false,
            )],
        );

        let row = &tree.realized_rows()[0];
        assert_eq!(
            tree.handle_click(row.ids.disclosure),
            Some(TreeRowAction::Toggle(7))
        );
        assert_eq!(
            tree.handle_click(row.ids.label),
            Some(TreeRowAction::Select(7))
        );
        assert_eq!(
            tree.handle_click(row.ids.content),
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
        tree.sync(
            &mut ui,
            &[TreeRowPresentation::new(
                1, "Selected", 1, false, false, true, false,
            )],
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
        tree.sync(
            &mut ui,
            &[
                TreeRowPresentation::new(1, "One", 0, true, false, false, false),
                TreeRowPresentation::new(2, "Two", 1, false, false, false, true),
                TreeRowPresentation::new(3, "Three", 1, false, false, false, false),
            ],
        );

        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::ArrowUp),
                Code::ArrowUp
            )),
            Some(TreeKeyboardAction::Focus(1))
        );
        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::ArrowDown),
                Code::ArrowDown
            )),
            Some(TreeKeyboardAction::Focus(3))
        );
        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::Home),
                Code::Home
            )),
            Some(TreeKeyboardAction::Focus(1))
        );
        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::End),
                Code::End
            )),
            Some(TreeKeyboardAction::Focus(3))
        );
        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::Enter),
                Code::Enter
            )),
            Some(TreeKeyboardAction::Activate(2))
        );
    }

    #[test]
    fn horizontal_arrows_use_tree_structure() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut tree = TreeViewController::<u32>::new(scroll);
        tree.sync(
            &mut ui,
            &[
                TreeRowPresentation::new(1, "Root", 0, true, true, false, true),
                TreeRowPresentation::new(2, "Child", 1, false, false, false, false),
                TreeRowPresentation::new(3, "Sibling", 0, true, false, false, false),
            ],
        );

        assert_eq!(
            tree.handle_keyboard_event(&KeyboardEvent::key_down(
                Key::Named(NamedKey::ArrowRight),
                Code::ArrowRight
            )),
            Some(TreeKeyboardAction::Focus(2))
        );

        tree.sync(
            &mut ui,
            &[
                TreeRowPresentation::new(1, "Root", 0, true, true, false, false),
                TreeRowPresentation::new(2, "Child", 1, false, false, false, true),
                TreeRowPresentation::new(3, "Sibling", 0, true, false, false, false),
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
}
