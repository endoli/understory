// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reusable dock-pane collapse/expand controller.

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};

use crate::{ElementId, Ui};

/// Fixed element ids that make up one dock pane.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DockPaneIds {
    /// Pane root element.
    pub pane: ElementId,
    /// Splitter element adjacent to the pane.
    pub splitter: ElementId,
    /// Toggle button/rail element.
    pub toggle: ElementId,
}

/// Visual policy for one dock pane.
#[derive(Clone, Debug, PartialEq)]
pub struct DockPaneStyle {
    /// Label shown while expanded.
    pub expanded_label: Box<str>,
    /// Label shown while collapsed.
    pub collapsed_label: Box<str>,
    /// Toggle width while expanded.
    pub expanded_toggle_width: f64,
    /// Toggle width while collapsed.
    pub collapsed_toggle_width: f64,
    /// Pane padding while expanded.
    pub expanded_padding: f64,
    /// Pane gap while expanded.
    pub expanded_gap: f64,
    /// Pane padding while collapsed.
    pub collapsed_padding: f64,
    /// Pane gap while collapsed.
    pub collapsed_gap: f64,
}

impl Default for DockPaneStyle {
    fn default() -> Self {
        Self {
            expanded_label: Box::from("Pane ⟩"),
            collapsed_label: Box::from("⟨"),
            expanded_toggle_width: 112.0,
            collapsed_toggle_width: 32.0,
            expanded_padding: 16.0,
            expanded_gap: 12.0,
            collapsed_padding: 6.0,
            collapsed_gap: 0.0,
        }
    }
}

/// Small reusable controller for one collapsible dock pane.
#[derive(Clone, Debug)]
pub struct DockPaneController {
    ids: DockPaneIds,
    hidden: Vec<ElementId>,
    collapsed: bool,
    expanded_extent: f64,
    collapsed_extent: f64,
    style: DockPaneStyle,
}

impl DockPaneController {
    /// Creates a new dock pane controller.
    #[must_use]
    pub fn new(
        ids: DockPaneIds,
        hidden: Vec<ElementId>,
        expanded_extent: f64,
        collapsed_extent: f64,
    ) -> Self {
        Self {
            ids,
            hidden,
            collapsed: false,
            expanded_extent: expanded_extent.max(0.0),
            collapsed_extent: collapsed_extent.max(0.0),
            style: DockPaneStyle::default(),
        }
    }

    /// Returns the fixed ids for this dock pane.
    #[must_use]
    pub const fn ids(&self) -> DockPaneIds {
        self.ids
    }

    /// Returns whether the pane is currently collapsed.
    #[must_use]
    pub const fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    /// Returns the remembered expanded extent.
    #[must_use]
    pub const fn expanded_extent(&self) -> f64 {
        self.expanded_extent
    }

    /// Returns the currently active pane extent.
    #[must_use]
    pub const fn current_extent(&self) -> f64 {
        if self.collapsed {
            self.collapsed_extent
        } else {
            self.expanded_extent
        }
    }

    /// Returns the current style.
    #[must_use]
    pub fn style(&self) -> &DockPaneStyle {
        &self.style
    }

    /// Replaces the current style.
    pub fn set_style(&mut self, style: DockPaneStyle) {
        self.style = style;
    }

    /// Sets the remembered expanded extent.
    pub fn set_expanded_extent(&mut self, extent: f64) {
        self.expanded_extent = extent.max(0.0);
    }

    /// Sets whether the pane is collapsed.
    pub fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }

    /// Toggles the collapsed state.
    pub fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
    }

    /// Applies the current dock-pane state into the retained UI.
    pub fn sync(&self, ui: &mut Ui) {
        ui.set_local(self.ids.pane, ui.properties().width, self.current_extent());
        ui.set_local(self.ids.splitter, ui.properties().visible, !self.collapsed);
        for id in &self.hidden {
            ui.set_local(*id, ui.properties().visible, !self.collapsed);
        }
        ui.set_label(
            self.ids.toggle,
            if self.collapsed {
                self.style.collapsed_label.clone()
            } else {
                self.style.expanded_label.clone()
            },
        );
        ui.set_local(
            self.ids.toggle,
            ui.properties().width,
            if self.collapsed {
                self.style.collapsed_toggle_width
            } else {
                self.style.expanded_toggle_width
            },
        );
        ui.set_local(
            self.ids.pane,
            ui.properties().padding,
            if self.collapsed {
                self.style.collapsed_padding
            } else {
                self.style.expanded_padding
            },
        );
        ui.set_local(
            self.ids.pane,
            ui.properties().gap,
            if self.collapsed {
                self.style.collapsed_gap
            } else {
                self.style.expanded_gap
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::{TYPE_BUTTON, TYPE_PANEL, TYPE_SPLITTER, default_theme};
    use kurbo::Rect;
    use understory_display::TextEngine;

    #[test]
    fn collapsing_hides_splitter_and_content() {
        let mut ui = Ui::new(default_theme());
        let mut text = TextEngine::new();
        ui.set_view_rect(Rect::new(0.0, 0.0, 640.0, 320.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 0.0);

        let row = ui.append_child(ui.root(), crate::TYPE_ROW);
        ui.set_local(row, ui.properties().padding, 0.0);
        ui.set_local(row, ui.properties().gap, 0.0);

        let content = ui.append_child(row, TYPE_PANEL);
        ui.set_local(content, ui.properties().fill, true);

        let splitter = ui.append_child(row, TYPE_SPLITTER);
        ui.set_local(splitter, ui.properties().width, 12.0);

        let pane = ui.append_child(row, TYPE_PANEL);
        let toggle = ui.append_child(pane, TYPE_BUTTON);
        let hidden = ui.append_child(pane, TYPE_PANEL);

        let ids = DockPaneIds {
            pane,
            splitter,
            toggle,
        };
        let mut dock = DockPaneController::new(ids, vec![hidden], 280.0, 44.0);
        dock.set_style(DockPaneStyle {
            expanded_label: Box::from("Inspector ⟩"),
            collapsed_label: Box::from("⟨"),
            expanded_toggle_width: 112.0,
            collapsed_toggle_width: 32.0,
            expanded_padding: 18.0,
            expanded_gap: 12.0,
            collapsed_padding: 6.0,
            collapsed_gap: 0.0,
        });

        dock.sync(&mut ui);
        let scene = ui.scene(&mut text);
        assert_eq!(scene.resolved_element(pane).unwrap().rect.width(), 280.0);
        assert!(scene.resolved_element(splitter).is_some());
        assert!(scene.resolved_element(hidden).is_some());

        dock.set_collapsed(true);
        dock.sync(&mut ui);
        let scene = ui.scene(&mut text);
        assert_eq!(scene.resolved_element(pane).unwrap().rect.width(), 44.0);
        assert!(scene.resolved_element(splitter).is_none());
        assert!(scene.resolved_element(hidden).is_none());
        assert_eq!(ui.element(toggle).and_then(|e| e.label()), Some("⟨"));
    }
}
