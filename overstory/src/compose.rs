// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Typed element/widget authoring specs for Overstory.

use alloc::boxed::Box;

use peniko::Color;
use understory_style::{ClassId, StyleCascade, TypeTag};

use crate::{
    BuiltInProperties, ElementId, LayoutClass, TYPE_COLUMN, TYPE_PANEL, TYPE_ROW, TYPE_SPACER, Ui,
    Widget,
};

/// Typed authoring surface for one element or widget append operation.
pub trait AppendSpec {
    /// Appends the spec under `parent`, returning the realized element id.
    fn append_to(self, ui: &mut Ui, parent: ElementId) -> ElementId;
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ElementOptions {
    pub(crate) width: Option<f64>,
    pub(crate) height: Option<f64>,
    pub(crate) padding: Option<f64>,
    pub(crate) gap: Option<f64>,
    pub(crate) fill: Option<bool>,
    pub(crate) visible: Option<bool>,
    pub(crate) enabled: Option<bool>,
    pub(crate) background: Option<Color>,
    pub(crate) foreground: Option<Color>,
    pub(crate) border_width: Option<f64>,
    pub(crate) corner_radius: Option<f64>,
    pub(crate) font_size: Option<f64>,
    pub(crate) label_padding: Option<f64>,
    pub(crate) display_name: Option<Box<str>>,
    pub(crate) style: Option<StyleCascade>,
    pub(crate) classes: alloc::vec::Vec<ClassId>,
}

impl ElementOptions {
    pub(crate) fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }

    pub(crate) fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }

    pub(crate) fn padding(mut self, padding: f64) -> Self {
        self.padding = Some(padding);
        self
    }

    pub(crate) fn gap(mut self, gap: f64) -> Self {
        self.gap = Some(gap);
        self
    }

    pub(crate) fn fill(mut self, fill: bool) -> Self {
        self.fill = Some(fill);
        self
    }

    pub(crate) fn visible(mut self, visible: bool) -> Self {
        self.visible = Some(visible);
        self
    }

    pub(crate) fn background(mut self, background: Color) -> Self {
        self.background = Some(background);
        self
    }

    pub(crate) fn foreground(mut self, foreground: Color) -> Self {
        self.foreground = Some(foreground);
        self
    }

    pub(crate) fn border_width(mut self, border_width: f64) -> Self {
        self.border_width = Some(border_width);
        self
    }

    pub(crate) fn corner_radius(mut self, corner_radius: f64) -> Self {
        self.corner_radius = Some(corner_radius);
        self
    }

    pub(crate) fn font_size(mut self, font_size: f64) -> Self {
        self.font_size = Some(font_size);
        self
    }

    pub(crate) fn label_padding(mut self, label_padding: f64) -> Self {
        self.label_padding = Some(label_padding);
        self
    }

    pub(crate) fn display_name(mut self, display_name: impl Into<Box<str>>) -> Self {
        self.display_name = Some(display_name.into());
        self
    }

    pub(crate) fn class(mut self, class: ClassId) -> Self {
        self.classes.push(class);
        self
    }

    pub(crate) fn style(mut self, style: StyleCascade) -> Self {
        self.style = Some(style);
        self
    }
}

pub(crate) fn apply_element_options(
    ui: &mut Ui,
    id: ElementId,
    props: &BuiltInProperties,
    options: ElementOptions,
) {
    if let Some(width) = options.width {
        ui.set_local(id, props.width, width);
    }
    if let Some(height) = options.height {
        ui.set_local(id, props.height, height);
    }
    if let Some(padding) = options.padding {
        ui.set_local(id, props.padding, padding);
    }
    if let Some(gap) = options.gap {
        ui.set_local(id, props.gap, gap);
    }
    if let Some(fill) = options.fill {
        ui.set_local(id, props.fill, fill);
    }
    if let Some(visible) = options.visible {
        ui.set_local(id, props.visible, visible);
    }
    if let Some(enabled) = options.enabled {
        ui.set_local(id, props.enabled, enabled);
    }
    if let Some(background) = options.background {
        ui.set_local(id, props.background, background);
    }
    if let Some(foreground) = options.foreground {
        ui.set_local(id, props.foreground, foreground);
    }
    if let Some(border_width) = options.border_width {
        ui.set_local(id, props.border_width, border_width);
    }
    if let Some(corner_radius) = options.corner_radius {
        ui.set_local(id, props.corner_radius, corner_radius);
    }
    if let Some(font_size) = options.font_size {
        ui.set_local(id, props.font_size, font_size);
    }
    if let Some(label_padding) = options.label_padding {
        ui.set_local(id, props.label_padding, label_padding);
    }
    if let Some(display_name) = options.display_name {
        ui.set_label(id, display_name);
    }
    if let Some(style) = options.style {
        ui.set_style(id, style);
    }
    for class in options.classes {
        ui.add_class(id, class);
    }
}

pub(crate) fn append_widget_spec<W: Widget + 'static>(
    ui: &mut Ui,
    parent: ElementId,
    type_tag: TypeTag,
    widget: W,
    options: ElementOptions,
) -> ElementId {
    let props = ui.properties().clone();
    let id = ui.append_child_with(parent, type_tag, Some(Box::new(widget)));
    apply_element_options(ui, id, &props, options);
    id
}

pub(crate) fn append_container_widget_spec<W: Widget + 'static>(
    ui: &mut Ui,
    parent: ElementId,
    type_tag: TypeTag,
    horizontal: bool,
    widget: W,
    options: ElementOptions,
) -> ElementId {
    let props = ui.properties().clone();
    let id = ui.append_container_with_widget(parent, type_tag, horizontal, Box::new(widget));
    apply_element_options(ui, id, &props, options);
    id
}

pub(crate) fn append_container_spec(
    ui: &mut Ui,
    parent: ElementId,
    type_tag: TypeTag,
    horizontal: bool,
    options: ElementOptions,
) -> ElementId {
    let props = ui.properties().clone();
    let id = ui.append_container(parent, type_tag, horizontal);
    apply_element_options(ui, id, &props, options);
    id
}

/// Decorative/padded container spec.
#[derive(Clone, Debug, Default)]
pub struct Panel {
    options: ElementOptions,
}

impl Panel {
    /// Creates a new panel.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies the built-in sidebar class.
    #[must_use]
    pub fn sidebar(mut self) -> Self {
        self.options = self.options.class(LayoutClass::Sidebar.class_id());
        self
    }

    /// Sets the explicit width.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.options = self.options.width(width);
        self
    }

    /// Sets the explicit height.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.options = self.options.height(height);
        self
    }

    /// Sets uniform padding.
    #[must_use]
    pub fn padding(mut self, padding: f64) -> Self {
        self.options = self.options.padding(padding);
        self
    }

    /// Sets the inter-child gap.
    #[must_use]
    pub fn gap(mut self, gap: f64) -> Self {
        self.options = self.options.gap(gap);
        self
    }

    /// Fills the remaining parent-axis space when supported by the parent.
    #[must_use]
    pub fn fill(mut self) -> Self {
        self.options = self.options.fill(true);
        self
    }

    /// Sets a background brush color.
    #[must_use]
    pub fn background(mut self, background: Color) -> Self {
        self.options = self.options.background(background);
        self
    }

    /// Sets the corner radius.
    #[must_use]
    pub fn corner_radius(mut self, corner_radius: f64) -> Self {
        self.options = self.options.corner_radius(corner_radius);
        self
    }

    /// Sets a human-readable display name for inspectors/debug views.
    #[must_use]
    pub fn display_name(mut self, display_name: impl Into<Box<str>>) -> Self {
        self.options = self.options.display_name(display_name);
        self
    }
}

impl AppendSpec for Panel {
    fn append_to(self, ui: &mut Ui, parent: ElementId) -> ElementId {
        append_container_spec(ui, parent, TYPE_PANEL, false, self.options)
    }
}

/// Horizontal stack container spec.
#[derive(Clone, Debug, Default)]
pub struct Row {
    options: ElementOptions,
}

impl Row {
    /// Creates a new row container.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets uniform padding.
    #[must_use]
    pub fn padding(mut self, padding: f64) -> Self {
        self.options = self.options.padding(padding);
        self
    }

    /// Sets the inter-child gap.
    #[must_use]
    pub fn gap(mut self, gap: f64) -> Self {
        self.options = self.options.gap(gap);
        self
    }

    /// Sets the explicit width.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.options = self.options.width(width);
        self
    }

    /// Sets the explicit height.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.options = self.options.height(height);
        self
    }

    /// Fills the remaining parent-axis space when supported by the parent.
    #[must_use]
    pub fn fill(mut self) -> Self {
        self.options = self.options.fill(true);
        self
    }

    /// Sets the background color.
    #[must_use]
    pub fn background(mut self, background: Color) -> Self {
        self.options = self.options.background(background);
        self
    }

    /// Sets a human-readable display name for inspectors/debug views.
    #[must_use]
    pub fn display_name(mut self, display_name: impl Into<Box<str>>) -> Self {
        self.options = self.options.display_name(display_name);
        self
    }
}

impl AppendSpec for Row {
    fn append_to(self, ui: &mut Ui, parent: ElementId) -> ElementId {
        append_container_spec(ui, parent, TYPE_ROW, true, self.options)
    }
}

/// Vertical stack container spec.
#[derive(Clone, Debug, Default)]
pub struct Column {
    options: ElementOptions,
}

impl Column {
    /// Creates a new column container.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets uniform padding.
    #[must_use]
    pub fn padding(mut self, padding: f64) -> Self {
        self.options = self.options.padding(padding);
        self
    }

    /// Sets the inter-child gap.
    #[must_use]
    pub fn gap(mut self, gap: f64) -> Self {
        self.options = self.options.gap(gap);
        self
    }

    /// Sets the explicit width.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.options = self.options.width(width);
        self
    }

    /// Sets the explicit height.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.options = self.options.height(height);
        self
    }

    /// Fills the remaining parent-axis space when supported by the parent.
    #[must_use]
    pub fn fill(mut self) -> Self {
        self.options = self.options.fill(true);
        self
    }

    /// Sets a human-readable display name for inspectors/debug views.
    #[must_use]
    pub fn display_name(mut self, display_name: impl Into<Box<str>>) -> Self {
        self.options = self.options.display_name(display_name);
        self
    }
}

impl AppendSpec for Column {
    fn append_to(self, ui: &mut Ui, parent: ElementId) -> ElementId {
        append_container_spec(ui, parent, TYPE_COLUMN, false, self.options)
    }
}

/// Non-interactive spacer element spec.
#[derive(Clone, Debug, Default)]
pub struct Spacer {
    options: ElementOptions,
}

impl Spacer {
    /// Creates a new spacer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the explicit width.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.options = self.options.width(width);
        self
    }

    /// Sets the explicit height.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.options = self.options.height(height);
        self
    }

    /// Fills the remaining parent-axis space when supported by the parent.
    #[must_use]
    pub fn fill(mut self) -> Self {
        self.options = self.options.fill(true);
        self
    }
}

impl AppendSpec for Spacer {
    fn append_to(self, ui: &mut Ui, parent: ElementId) -> ElementId {
        let props = ui.properties().clone();
        let id = ui.append_child_with(parent, TYPE_SPACER, None);
        apply_element_options(ui, id, &props, self.options);
        id
    }
}
