// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Draggable splitter widget for resizing an adjacent pane.

use alloc::vec::Vec;

use kurbo::Size;
use peniko::{Brush, Color};
use understory_display::{DisplayAlign, DisplayNode};
use understory_style::ResourceKey;

use crate::{
    Element, ElementId, ResolvedElement, ThemeKeys, Widget, WidgetPointerEvent, content_box,
};

/// Axis/orientation for a splitter handle.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SplitterAxis {
    /// Vertical divider between left/right panes. Dragging adjusts width.
    #[default]
    Vertical,
    /// Horizontal divider between top/bottom panes. Dragging adjusts height.
    Horizontal,
}

/// Which adjacent pane the splitter resizes.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SplitterSide {
    /// Resize the pane before the splitter.
    #[default]
    Leading,
    /// Resize the pane after the splitter.
    Trailing,
}

/// Draggable splitter that resizes one leading pane in its parent container.
///
/// The configured target pane is expected to be the leading sibling in the
/// same row/column as the splitter. The trailing pane is expected to absorb
/// the remaining space with `fill = true`.
#[derive(Clone, Debug)]
pub struct SplitterWidget {
    axis: SplitterAxis,
    side: SplitterSide,
    target: Option<ElementId>,
    min_primary: f64,
    min_secondary: f64,
    drag_offset: f64,
    dragging: bool,
}

impl Default for SplitterWidget {
    fn default() -> Self {
        Self {
            axis: SplitterAxis::Vertical,
            side: SplitterSide::Leading,
            target: None,
            min_primary: 120.0,
            min_secondary: 240.0,
            drag_offset: 0.0,
            dragging: false,
        }
    }
}

impl SplitterWidget {
    /// Creates a vertical splitter controlling the width of one leading pane.
    #[must_use]
    pub fn vertical(target: ElementId) -> Self {
        Self {
            target: Some(target),
            ..Self::default()
        }
    }

    /// Creates a vertical splitter controlling the width of one trailing pane.
    #[must_use]
    pub fn vertical_trailing(target: ElementId) -> Self {
        Self {
            side: SplitterSide::Trailing,
            target: Some(target),
            ..Self::default()
        }
    }

    /// Creates a horizontal splitter controlling the height of one leading pane.
    #[must_use]
    pub fn horizontal(target: ElementId) -> Self {
        Self {
            axis: SplitterAxis::Horizontal,
            target: Some(target),
            ..Self::default()
        }
    }

    /// Creates a horizontal splitter controlling the height of one trailing pane.
    #[must_use]
    pub fn horizontal_trailing(target: ElementId) -> Self {
        Self {
            axis: SplitterAxis::Horizontal,
            side: SplitterSide::Trailing,
            target: Some(target),
            ..Self::default()
        }
    }

    /// Sets the minimum extent for the controlled pane.
    #[must_use]
    pub fn with_min_primary(mut self, min_primary: f64) -> Self {
        self.min_primary = min_primary.max(0.0);
        self
    }

    /// Sets the minimum extent reserved for the trailing pane.
    #[must_use]
    pub fn with_min_secondary(mut self, min_secondary: f64) -> Self {
        self.min_secondary = min_secondary.max(0.0);
        self
    }

    /// Updates the controlled pane target.
    pub fn set_target(&mut self, target: ElementId) {
        self.target = Some(target);
    }

    /// Updates which adjacent pane the splitter controls.
    pub fn set_side(&mut self, side: SplitterSide) {
        self.side = side;
    }

    /// Updates the minimum extent for the controlled pane.
    pub fn set_min_primary(&mut self, min_primary: f64) {
        self.min_primary = min_primary.max(0.0);
    }

    /// Updates the minimum extent reserved for the opposite pane.
    pub fn set_min_secondary(&mut self, min_secondary: f64) {
        self.min_secondary = min_secondary.max(0.0);
    }

    fn grip_size(&self) -> Size {
        match self.axis {
            SplitterAxis::Vertical => Size::new(4.0, 56.0),
            SplitterAxis::Horizontal => Size::new(56.0, 4.0),
        }
    }

    fn splitter_extent(&self, resolved: &ResolvedElement) -> f64 {
        match self.axis {
            SplitterAxis::Vertical => resolved.rect.width(),
            SplitterAxis::Horizontal => resolved.rect.height(),
        }
    }

    fn center_coordinate(&self, rect: kurbo::Rect) -> f64 {
        match self.axis {
            SplitterAxis::Vertical => rect.center().x,
            SplitterAxis::Horizontal => rect.center().y,
        }
    }

    fn point_coordinate(&self, point: kurbo::Point) -> f64 {
        match self.axis {
            SplitterAxis::Vertical => point.x,
            SplitterAxis::Horizontal => point.y,
        }
    }

    fn clamped_primary_extent(
        &self,
        point: kurbo::Point,
        resolved: &ResolvedElement,
        target: &ResolvedElement,
        parent_rect: kurbo::Rect,
    ) -> f64 {
        let requested_center = self.point_coordinate(point) - self.drag_offset;
        let splitter_extent = self.splitter_extent(resolved);
        let parent_extent = axis_extent(self.axis, parent_rect);
        let max_primary = (parent_extent - splitter_extent - self.min_secondary).max(0.0);
        let min_primary = self.min_primary.min(max_primary);
        let requested_primary = match self.side {
            SplitterSide::Leading => {
                let origin = axis_start(self.axis, target.rect);
                requested_center - origin - splitter_extent * 0.5
            }
            SplitterSide::Trailing => {
                let end = axis_end(self.axis, parent_rect);
                end - requested_center - splitter_extent * 0.5
            }
        };
        requested_primary.clamp(min_primary, max_primary)
    }

    fn apply_primary_extent(
        &self,
        target: ElementId,
        extent: f64,
        ctx: &mut crate::PointerEventCtx<'_>,
    ) {
        let props = ctx.properties();
        match self.axis {
            SplitterAxis::Vertical => ctx.set_local(target, props.width, extent),
            SplitterAxis::Horizontal => ctx.set_local(target, props.height, extent),
        }
    }
}

impl Widget for SplitterWidget {
    fn display(&self, _id: ElementId, resolved: &ResolvedElement, children: &mut Vec<DisplayNode>) {
        let grip = DisplayNode::fixed_frame(
            self.grip_size(),
            DisplayNode::fill_rounded_rect(
                999.0,
                Brush::Solid(grip_color(resolved.foreground, resolved.pressed)),
            ),
        );
        children.push(content_box(
            grip,
            DisplayAlign::Center,
            DisplayAlign::Center,
            understory_display::Insets::uniform(0.0),
        ));
    }

    fn handle_pointer_event(
        &mut self,
        id: ElementId,
        event: &WidgetPointerEvent,
        resolved: &ResolvedElement,
        ctx: &mut crate::PointerEventCtx<'_>,
        _text: &mut understory_display::TextEngine,
        _batch: &mut crate::InteractionBatch,
    ) -> bool {
        let Some(target) = self.target else {
            return false;
        };
        match *event {
            WidgetPointerEvent::Down { point } => {
                self.dragging = true;
                self.drag_offset =
                    self.point_coordinate(point) - self.center_coordinate(resolved.rect);
                true
            }
            WidgetPointerEvent::Move { point } if self.dragging => {
                let Some(target_resolved) = ctx.resolved_element(target) else {
                    return false;
                };
                let Some(parent) = ctx.parent(id) else {
                    return false;
                };
                let Some(parent_rect) = ctx.rect(parent) else {
                    return false;
                };
                let extent =
                    self.clamped_primary_extent(point, resolved, target_resolved, parent_rect);
                self.apply_primary_extent(target, extent, ctx);
                true
            }
            WidgetPointerEvent::Up { .. } | WidgetPointerEvent::Cancel if self.dragging => {
                self.dragging = false;
                true
            }
            _ => false,
        }
    }

    fn background_key(&self, element: &Element) -> Option<ResourceKey> {
        if element.pseudos.pressed {
            Some(ThemeKeys::SPLITTER_ACTIVE_BACKGROUND)
        } else if element.pseudos.hovered {
            Some(ThemeKeys::SPLITTER_HOVER_BACKGROUND)
        } else {
            None
        }
    }

    fn default_pickable(&self) -> bool {
        true
    }

    crate::impl_widget_any!();
}

fn grip_color(base: Color, pressed: bool) -> Color {
    let rgba = base.to_rgba8();
    let alpha = if pressed { 224 } else { 168 };
    Color::from_rgba8(rgba.r, rgba.g, rgba.b, alpha)
}

fn axis_start(axis: SplitterAxis, rect: kurbo::Rect) -> f64 {
    match axis {
        SplitterAxis::Vertical => rect.x0,
        SplitterAxis::Horizontal => rect.y0,
    }
}

fn axis_end(axis: SplitterAxis, rect: kurbo::Rect) -> f64 {
    match axis {
        SplitterAxis::Vertical => rect.x1,
        SplitterAxis::Horizontal => rect.y1,
    }
}

fn axis_extent(axis: SplitterAxis, rect: kurbo::Rect) -> f64 {
    match axis {
        SplitterAxis::Vertical => rect.width(),
        SplitterAxis::Horizontal => rect.height(),
    }
}
