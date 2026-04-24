// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Button widget with centered text content.

use alloc::{boxed::Box, vec::Vec};

use cursor_icon::CursorIcon;
use kurbo::Size;
use peniko::Brush;
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, NamedKey};
use understory_display::{DisplayAlign, DisplayNode, Insets};

use crate::{
    AppendSpec, ButtonClass, Element, ElementId, Interaction, InteractionBatch, KeyboardEventCtx,
    MeasureCtx, MeasureStyle, ResolvedElement, Ui, Widget, compose, content_box, text_label_node,
};

/// Interactive push button widget with horizontally padded, vertically
/// centered label text.
#[derive(Clone, Debug, Default)]
pub struct Button {
    text: Box<str>,
    space_armed: bool,
    mount: compose::ElementOptions,
}

impl Button {
    /// Creates a new button widget.
    #[must_use]
    pub fn new() -> Self {
        Self {
            text: Box::from(""),
            space_armed: false,
            mount: compose::ElementOptions::default(),
        }
    }

    /// Creates a button with initial text.
    #[must_use]
    pub fn with_text(mut self, text: impl Into<Box<str>>) -> Self {
        self.text = text.into();
        self
    }

    /// Returns the button text.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        (!self.text.is_empty()).then_some(self.text.as_ref())
    }

    /// Replaces the button text.
    pub fn set_text(&mut self, text: impl Into<Box<str>>) {
        self.text = text.into();
    }

    /// Applies the built-in primary button class.
    #[must_use]
    pub fn primary(mut self) -> Self {
        self.mount = self.mount.class(ButtonClass::Primary.class_id());
        self
    }

    /// Fills the remaining parent-axis space when supported by the parent.
    #[must_use]
    pub fn fill(mut self) -> Self {
        self.mount = self.mount.fill(true);
        self
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

    /// Sets the foreground color.
    #[must_use]
    pub fn foreground(mut self, foreground: crate::Color) -> Self {
        self.mount = self.mount.foreground(foreground);
        self
    }

    /// Sets a style cascade for the button element.
    #[must_use]
    pub fn style(mut self, style: understory_style::StyleCascade) -> Self {
        self.mount = self.mount.style(style);
        self
    }

    /// Sets a display name for inspectors/debug views.
    #[must_use]
    pub fn display_name(mut self, display_name: impl Into<Box<str>>) -> Self {
        self.mount = self.mount.display_name(display_name);
        self
    }
}

impl AppendSpec for Button {
    fn append_to(mut self, ui: &mut Ui, parent: ElementId) -> ElementId {
        let mount = core::mem::take(&mut self.mount);
        compose::append_widget_spec(ui, parent, crate::TYPE_BUTTON, self, mount)
    }
}

impl Widget for Button {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Widget measurement uses small display values and Parley APIs take f32."
    )]
    fn measure(
        &self,
        available: Size,
        style: &MeasureStyle<'_>,
        ctx: &mut MeasureCtx<'_>,
    ) -> Option<Size> {
        let text = self.text()?;
        let content_width = (available.width - style.label_padding * 2.0).max(1.0) as f32;
        let text_size = ctx.measure_text(
            text,
            style.font_size as f32,
            style.font_family,
            Some(content_width),
        );
        let intrinsic_width = (text_size.width + style.label_padding * 2.0)
            .min(available.width)
            .max(0.0);
        Some(Size::new(
            intrinsic_width,
            text_size.height + style.label_padding * 2.0,
        ))
    }

    fn display(&self, _id: ElementId, resolved: &ResolvedElement, children: &mut Vec<DisplayNode>) {
        let Some(text) = resolved.text.as_deref() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let text_node = text_label_node(text, Brush::Solid(resolved.foreground), resolved);
        children.push(content_box(
            text_node,
            DisplayAlign::Start,
            DisplayAlign::Center,
            Insets::symmetric(resolved.label_padding, 0.0),
        ));
    }

    fn keyboard_event(
        &mut self,
        id: ElementId,
        event: &KeyboardEvent,
        ctx: &mut KeyboardEventCtx<'_>,
        _text: &mut understory_display::TextEngine,
        batch: &mut InteractionBatch,
    ) -> bool {
        match (&event.key, event.state) {
            (Key::Named(NamedKey::Enter), KeyState::Down) if !event.repeat => {
                ctx.set_pressed(id, true);
                batch.push(Interaction::PressStarted(id));
                ctx.set_pressed(id, false);
                batch.push(Interaction::PressEnded(id));
                batch.push(Interaction::Clicked(id));
                true
            }
            (Key::Character(space), KeyState::Down) if !event.repeat && &**space == " " => {
                if !self.space_armed {
                    self.space_armed = true;
                    ctx.set_pressed(id, true);
                    batch.push(Interaction::PressStarted(id));
                }
                true
            }
            (Key::Character(space), KeyState::Up) if &**space == " " => {
                if self.space_armed {
                    self.space_armed = false;
                    ctx.set_pressed(id, false);
                    batch.push(Interaction::PressEnded(id));
                    batch.push(Interaction::Clicked(id));
                }
                true
            }
            _ => false,
        }
    }

    fn default_pickable(&self) -> bool {
        true
    }

    fn default_focusable(&self) -> bool {
        true
    }

    fn cursor_icon(&self, _element: &Element) -> Option<CursorIcon> {
        Some(CursorIcon::Pointer)
    }

    crate::impl_widget_any!();
}
