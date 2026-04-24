// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{format, string::String, vec::Vec};
use core::{fmt::Display, marker::PhantomData};

use overstory::{ElementId, TextBlock, Ui};
use understory_inspector::{Inspector, InspectorModel};

/// Styling knobs for tree rows projected through Overstory.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorTreeStyle {
    /// Inner label padding for one row.
    pub label_padding: f64,
    /// Outer row padding.
    pub padding: f64,
    /// Font size for row labels.
    pub font_size: f64,
    /// String used per tree depth for indentation.
    pub indent_unit: &'static str,
    /// Prefix for expanded parents.
    pub expanded_prefix: &'static str,
    /// Prefix for collapsed parents.
    pub collapsed_prefix: &'static str,
    /// Prefix for leaf rows.
    pub leaf_prefix: &'static str,
    /// Prefix applied to selected rows.
    pub selected_prefix: &'static str,
    /// Prefix applied to unselected rows.
    pub unselected_prefix: &'static str,
}

impl Default for InspectorTreeStyle {
    fn default() -> Self {
        Self {
            label_padding: 2.0,
            padding: 1.0,
            font_size: 11.0,
            indent_unit: "  ",
            expanded_prefix: "▾ ",
            collapsed_prefix: "▸ ",
            leaf_prefix: "  ",
            selected_prefix: "● ",
            unselected_prefix: "",
        }
    }
}

/// One realized tree row in Overstory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorTreeRealizedRow<K> {
    /// Row key in the underlying inspector model.
    pub key: K,
    /// Realized text block element.
    pub element: ElementId,
}

/// Outcome of clicking one realized tree row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorTreeClick<K> {
    /// Clicked model key.
    pub key: K,
    /// Whether the click also toggled expansion.
    pub toggled: bool,
}

/// Overstory-facing controller for an [`understory_inspector::Inspector`] tree view.
#[derive(Clone, Debug)]
pub struct InspectorTreeController<M>
where
    M: InspectorModel,
{
    scroll_view: ElementId,
    style: InspectorTreeStyle,
    inspector: Inspector<M>,
    rows: Vec<InspectorTreeRealizedRow<M::Key>>,
    _marker: PhantomData<M>,
}

impl<M> InspectorTreeController<M>
where
    M: InspectorModel,
{
    /// Creates a controller bound to one Overstory `ScrollView`.
    #[must_use]
    pub fn new(scroll_view: ElementId, inspector: Inspector<M>) -> Self {
        Self {
            scroll_view,
            style: InspectorTreeStyle::default(),
            inspector,
            rows: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Returns the bound tree scroll view.
    #[must_use]
    pub const fn scroll_view(&self) -> ElementId {
        self.scroll_view
    }

    /// Returns the current row style.
    #[must_use]
    pub const fn style(&self) -> &InspectorTreeStyle {
        &self.style
    }

    /// Replaces the row style.
    pub fn set_style(&mut self, style: InspectorTreeStyle) {
        self.style = style;
    }

    /// Returns the underlying inspector controller.
    #[must_use]
    pub const fn inspector(&self) -> &Inspector<M> {
        &self.inspector
    }

    /// Returns mutable access to the underlying inspector controller.
    pub fn inspector_mut(&mut self) -> &mut Inspector<M> {
        &mut self.inspector
    }

    /// Returns the currently realized rows.
    #[must_use]
    pub fn realized_rows(&self) -> &[InspectorTreeRealizedRow<M::Key>] {
        &self.rows
    }

    /// Syncs the current inspector projection into Overstory using the
    /// inspector item's `Display` output.
    pub fn sync_default(&mut self, ui: &mut Ui, selected_key: Option<&M::Key>)
    where
        M::Item: Display,
    {
        self.sync_with(ui, selected_key, |item| format!("{item}"));
    }

    /// Syncs the current inspector projection into Overstory using a custom
    /// item formatter.
    pub fn sync_with(
        &mut self,
        ui: &mut Ui,
        selected_key: Option<&M::Key>,
        mut format_item: impl FnMut(M::Item) -> String,
    ) {
        self.inspector.sync();
        let rows = self.inspector.visible_rows().to_vec();

        while self.rows.len() < rows.len() {
            let block = ui.append(
                self.scroll_view,
                TextBlock::new()
                    .label_padding(self.style.label_padding)
                    .padding(self.style.padding)
                    .font_size(self.style.font_size),
            );
            ui.set_local(block, ui.properties().pickable, true);
            self.rows.push(InspectorTreeRealizedRow {
                key: rows[self.rows.len()].key.clone(),
                element: block,
            });
        }

        for (index, realized) in self.rows.iter_mut().enumerate() {
            if let Some(row) = rows.get(index) {
                realized.key = row.key.clone();
                let item = self
                    .inspector
                    .item(&row.key)
                    .map(&mut format_item)
                    .unwrap_or_default();
                let indent = self.style.indent_unit.repeat(row.depth);
                let disclosure = match (row.has_children, row.is_expanded) {
                    (true, true) => self.style.expanded_prefix,
                    (true, false) => self.style.collapsed_prefix,
                    (false, _) => self.style.leaf_prefix,
                };
                let marker = if selected_key == Some(&row.key) {
                    self.style.selected_prefix
                } else {
                    self.style.unselected_prefix
                };
                let label = format!("{indent}{disclosure}{marker}{item}");
                set_text_block_text(ui, realized.element, label);
                ui.set_local(realized.element, ui.properties().visible, true);
            } else {
                set_text_block_text(ui, realized.element, "");
                ui.set_local(realized.element, ui.properties().visible, false);
            }
        }
    }

    /// Handles a click on a realized row element.
    pub fn handle_row_click(&mut self, target: ElementId) -> Option<InspectorTreeClick<M::Key>> {
        let index = self.rows.iter().position(|row| row.element == target)?;
        let visible = self.inspector.visible_rows().to_vec();
        let row = visible.get(index)?;
        let key = row.key.clone();
        let mut toggled = false;
        if row.has_children {
            let _ = self.inspector.toggle(key.clone());
            toggled = true;
        }
        Some(InspectorTreeClick { key, toggled })
    }
}

fn set_text_block_text(ui: &mut Ui, id: ElementId, text: impl Into<alloc::boxed::Box<str>>) {
    ui.widget_mut::<TextBlock>(id)
        .expect("inspector tree rows use text block children")
        .set_text(text);
}
