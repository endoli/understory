// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{format, string::String, vec::Vec};
use core::fmt::Display;

use overstory::{Color, Ui};
use overstory_tree::{
    TreeRowAction, TreeRowIds, TreeRowPresentation, TreeViewController, TreeViewRealizedRow,
    TreeViewStyle,
};
use understory_inspector::{Inspector, InspectorModel};

/// Styling knobs for inspector trees rendered through Overstory.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InspectorTreeStyle {
    /// Generic tree row style.
    pub tree: TreeViewStyle,
}

/// One realized inspector tree row in Overstory.
pub type InspectorTreeRealizedRow<K> = TreeViewRealizedRow<K>;

/// Realized element ids for one inspector tree row.
pub type InspectorTreeRowIds = TreeRowIds;

/// Outcome of clicking one realized inspector tree row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorTreeClick<K> {
    /// Clicked model key.
    pub key: K,
    /// Whether the click toggled expansion instead of selecting the row.
    pub toggled: bool,
}

/// Overstory-facing adapter that renders an [`understory_inspector::Inspector`]
/// through the generic [`overstory_tree`] surface.
#[derive(Clone, Debug)]
pub struct InspectorTreeController<M>
where
    M: InspectorModel,
{
    style: InspectorTreeStyle,
    inspector: Inspector<M>,
    tree: TreeViewController<M::Key>,
}

impl<M> InspectorTreeController<M>
where
    M: InspectorModel,
{
    /// Creates a controller bound to one Overstory `ScrollView`.
    #[must_use]
    pub fn new(scroll_view: overstory::ElementId, inspector: Inspector<M>) -> Self {
        Self {
            style: InspectorTreeStyle::default(),
            inspector,
            tree: TreeViewController::new(scroll_view),
        }
    }

    /// Returns the bound tree scroll view.
    #[must_use]
    pub const fn scroll_view(&self) -> overstory::ElementId {
        self.tree.scroll_view()
    }

    /// Returns the current row style.
    #[must_use]
    pub const fn style(&self) -> &InspectorTreeStyle {
        &self.style
    }

    /// Replaces the row style.
    pub fn set_style(&mut self, style: InspectorTreeStyle) {
        self.tree.set_style(style.tree.clone());
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
        self.tree.realized_rows()
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
        let visible_rows = self.inspector.visible_rows().to_vec();
        let rows = visible_rows
            .iter()
            .map(|row| {
                let item = self
                    .inspector
                    .item(&row.key)
                    .map(&mut format_item)
                    .unwrap_or_default();
                TreeRowPresentation::new(
                    row.key.clone(),
                    item.into_boxed_str(),
                    row.depth,
                    row.has_children,
                    row.is_expanded,
                    selected_key == Some(&row.key),
                )
            })
            .collect::<Vec<_>>();
        self.tree.sync(ui, &rows);
    }

    /// Maps a clicked Overstory element back into an inspector tree action.
    pub fn handle_row_click(
        &mut self,
        target: overstory::ElementId,
    ) -> Option<InspectorTreeClick<M::Key>> {
        match self.tree.handle_click(target)? {
            TreeRowAction::Select(key) => Some(InspectorTreeClick {
                key,
                toggled: false,
            }),
            TreeRowAction::Toggle(key) => {
                let toggled = self.inspector.toggle(key.clone());
                Some(InspectorTreeClick { key, toggled })
            }
        }
    }
}

/// Returns a theme-tinted inspector tree style.
#[must_use]
pub fn themed_tree_style(background: Color, selected_background: Color) -> InspectorTreeStyle {
    let mut style = InspectorTreeStyle::default();
    style.tree.background = background;
    style.tree.selected_background = selected_background;
    style
}
