// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dropdown surface widget — promoted overlay surface anchored to an owner.

use kurbo::Point;

use crate::{AppendSpec, ElementId, SemanticRole, SurfaceRole, Ui, Widget, compose};

/// Dropdown surface widget promoted out of normal inline painting.
///
/// The dropdown owns overlay placement metadata, while its visual content
/// comes from the normal retained element subtree beneath it.
#[derive(Clone, Debug)]
pub struct Dropdown {
    anchor: ElementId,
    open: bool,
    position: Option<Point>,
    mount: compose::ElementOptions,
}

impl Dropdown {
    /// Creates a dropdown surface anchored to one owner element.
    #[must_use]
    pub fn new(anchor: ElementId) -> Self {
        Self {
            anchor,
            open: false,
            position: None,
            mount: compose::ElementOptions::default(),
        }
    }

    /// Returns the owner element this dropdown is anchored to.
    #[must_use]
    pub const fn anchor(&self) -> ElementId {
        self.anchor
    }

    /// Returns whether the dropdown is currently open.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Sets the open state.
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// Returns the desired dropdown origin in root coordinates.
    #[must_use]
    pub const fn position(&self) -> Option<Point> {
        self.position
    }

    /// Sets the desired dropdown origin in root coordinates.
    pub fn set_position(&mut self, position: Point) {
        self.position = Some(position);
    }

    /// Sets an explicit width.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.mount = self.mount.width(width);
        self
    }

    /// Sets an explicit height.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.mount = self.mount.height(height);
        self
    }

    /// Sets uniform inner padding.
    #[must_use]
    pub fn padding(mut self, padding: f64) -> Self {
        self.mount = self.mount.padding(padding);
        self
    }

    /// Sets the inter-child gap.
    #[must_use]
    pub fn gap(mut self, gap: f64) -> Self {
        self.mount = self.mount.gap(gap);
        self
    }

    /// Sets the background color.
    #[must_use]
    pub fn background(mut self, background: crate::Color) -> Self {
        self.mount = self.mount.background(background);
        self
    }

    /// Sets border width.
    #[must_use]
    pub fn border_width(mut self, border_width: f64) -> Self {
        self.mount = self.mount.border_width(border_width);
        self
    }

    /// Sets corner radius.
    #[must_use]
    pub fn corner_radius(mut self, corner_radius: f64) -> Self {
        self.mount = self.mount.corner_radius(corner_radius);
        self
    }

    /// Sets a display name for inspectors/debug views.
    #[must_use]
    pub fn display_name(mut self, display_name: impl Into<alloc::boxed::Box<str>>) -> Self {
        self.mount = self.mount.display_name(display_name);
        self
    }
}

impl AppendSpec for Dropdown {
    fn append_to(mut self, ui: &mut Ui, parent: ElementId) -> ElementId {
        let mount = core::mem::take(&mut self.mount);
        compose::append_container_widget_spec(ui, parent, crate::TYPE_DROPDOWN, false, self, mount)
    }
}

impl Widget for Dropdown {
    fn surface_role(&self) -> Option<SurfaceRole> {
        self.open.then_some(SurfaceRole::Dropdown)
    }

    fn semantic_role(&self) -> SemanticRole {
        SemanticRole::Generic
    }

    crate::impl_widget_any!();
}
