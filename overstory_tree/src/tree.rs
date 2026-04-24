// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{boxed::Box, vec::Vec};

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
    ) -> Self {
        Self {
            key,
            label: label.into(),
            depth,
            has_children,
            is_expanded,
            selected,
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
    /// Whether this row currently has a disclosure affordance.
    pub can_toggle: bool,
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
                realized.can_toggle = row.has_children;
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

    fn append_row(&self, ui: &mut Ui, key: K) -> TreeViewRealizedRow<K> {
        let row = ui.append(
            self.scroll_view,
            Panel::new()
                .fill()
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
            can_toggle: false,
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
    use overstory::{ScrollView, default_theme};

    #[test]
    fn disclosure_and_label_clicks_map_to_distinct_actions() {
        let mut ui = Ui::new(default_theme());
        let scroll = ui.append(ui.root(), ScrollView::new().fill());
        let mut tree = TreeViewController::<u32>::new(scroll);
        tree.sync(
            &mut ui,
            &[TreeRowPresentation::new(7, "Node", 0, true, false, false)],
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
                1, "Selected", 1, false, false, true,
            )],
        );

        let resolved = ui
            .scene()
            .resolved_element(tree.realized_rows()[0].ids.row)
            .expect("tree row should be visible");
        assert_eq!(resolved.background, Color::from_rgba8(1, 2, 3, 255));
    }
}
