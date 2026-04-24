// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Derived scene snapshot over the retained Overstory element tree.

use alloc::{borrow::Cow, boxed::Box, vec, vec::Vec};

use hashbrown::HashMap;
use kurbo::{Affine, Point, Rect, Size};
use understory_box_tree::{LocalNode, NodeFlags, NodeId, QueryFilter, Tree};
use understory_display::{TextAlign, TextEngine};
use understory_property::{DependencyObjectExt, PropertyRegistry, PropertyStore};
use understory_responder::adapters::box_tree::top_hit_for_point;
use understory_style::{
    PseudoClassId, ResolveCx, StyleCascade, StyleCascadeBuilder, StyleSource, Theme, TypeTag,
};

use crate::{
    BuiltInProperties, Color, Element, ElementId, PSEUDO_DISABLED, PSEUDO_FOCUS_VISIBLE,
    PSEUDO_FOCUSED, PSEUDO_HOVER, PSEUDO_PRESSED, SemanticInfo, SemanticRole, SemanticState,
    TYPE_BUTTON, TYPE_COLUMN, TYPE_DIVIDER, TYPE_DROPDOWN, TYPE_PANEL, TYPE_ROOT, TYPE_ROW,
    TYPE_SCROLL_VIEW, TYPE_SPINNER, TYPE_SPLITTER, TYPE_TEXT_BLOCK, TYPE_TEXT_INPUT, TYPE_TOOLTIP,
    ThemeKeys, Widget, WidgetArena, WidgetHandle,
    built_in_styles::BuiltInStyles,
    widget::{MeasureStyle, widget_text},
};

/// Border styling for one resolved element.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct BorderStyle {
    /// Border color.
    pub color: Color,
    /// Border width in scene units.
    pub width: f64,
}

impl Default for BorderStyle {
    fn default() -> Self {
        Self {
            color: Color::TRANSPARENT,
            width: 0.0,
        }
    }
}

/// One resolved retained element, suitable for a renderer-facing adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedElement {
    /// Retained element id.
    pub id: ElementId,
    /// Style type tag.
    pub type_tag: TypeTag,
    /// Depth in the retained tree.
    pub depth: u16,
    /// Final rectangle in view space.
    pub rect: Rect,
    /// Background fill color.
    pub background: Color,
    /// Foreground/text color.
    pub foreground: Color,
    /// Border style.
    pub border: BorderStyle,
    /// Corner radius.
    pub corner_radius: f64,
    /// Optional widget-owned text content.
    pub text: Option<Box<str>>,
    /// Hover state at snapshot time.
    pub hovered: bool,
    /// Press state at snapshot time.
    pub pressed: bool,
    /// Focus state at snapshot time.
    pub focused: bool,
    /// Focus-visible state at snapshot time.
    pub focus_visible: bool,
    /// Disabled state at snapshot time.
    pub disabled: bool,
    /// Resolved font size for text content.
    pub font_size: f64,
    /// Resolved horizontal text padding.
    pub label_padding: f64,
    /// Font family for text content.
    pub font_family: Box<str>,
    /// Text alignment for text content.
    pub text_align: TextAlign,
    /// Whether this element clips its children to its bounds.
    pub clips_content: bool,
    /// Vertical scroll offset applied to children.
    pub scroll_offset: f64,
    /// Widget handle for delegating display to the widget.
    pub widget: Option<WidgetHandle>,
    /// Resolved semantic snapshot for this element.
    pub semantics: SemanticInfo,
}

/// Full resolved scene snapshot for one Overstory frame.
#[derive(Debug)]
pub struct SceneSnapshot {
    view_rect: Rect,
    tree: Tree,
    resolved: Vec<ResolvedElement>,
    element_to_node: Vec<Option<NodeId>>,
    node_to_element: HashMap<NodeId, ElementId>,
}

impl SceneSnapshot {
    pub(crate) fn build(
        elements: &[Element],
        root: ElementId,
        view_rect: Rect,
        registry: &PropertyRegistry,
        props: &BuiltInProperties,
        theme: &Theme,
        built_in_styles: &BuiltInStyles,
        widget_arena: &WidgetArena,
        text: &mut TextEngine,
    ) -> (Self, Vec<(ElementId, f64, f64)>) {
        let mut tree = Tree::new();
        let mut resolved = Vec::new();
        let mut element_to_node = vec![None; elements.len()];
        let mut node_to_element = HashMap::new();
        let mut scroll_metrics = Vec::new();
        let mut builder = SceneBuilder {
            elements,
            registry,
            props,
            theme,
            built_in_styles,
            widget_arena,
            text,
            tree: &mut tree,
            resolved: &mut resolved,
            element_to_node: &mut element_to_node,
            node_to_element: &mut node_to_element,
            scroll_metrics: &mut scroll_metrics,
            next_z: 0,
        };
        let _ = builder.layout_element(
            root,
            Point::new(view_rect.x0, view_rect.y0),
            view_rect,
            None,
            None,
            0,
        );
        let _ = tree.commit();
        (
            Self {
                view_rect,
                tree,
                resolved,
                element_to_node,
                node_to_element,
            },
            scroll_metrics,
        )
    }

    /// Returns the view rectangle used to build the snapshot.
    #[must_use]
    pub fn view_rect(&self) -> Rect {
        self.view_rect
    }

    /// Returns the resolved elements in stable depth-first order.
    #[must_use]
    pub fn resolved(&self) -> &[ResolvedElement] {
        &self.resolved
    }

    /// Returns the underlying box tree.
    #[must_use]
    pub fn box_tree(&self) -> &Tree {
        &self.tree
    }

    /// Returns the resolved element for an id, if present.
    #[must_use]
    pub fn resolved_element(&self, id: ElementId) -> Option<&ResolvedElement> {
        self.resolved.iter().find(|element| element.id == id)
    }

    /// Returns the projected box-tree node for an element, if present.
    #[must_use]
    pub fn node_for(&self, id: ElementId) -> Option<NodeId> {
        self.element_to_node.get(id.index()).and_then(|node| *node)
    }

    /// Returns the topmost hit element at a point.
    #[must_use]
    pub fn top_hit(&self, point: Point) -> Option<ElementId> {
        let hit = top_hit_for_point(&self.tree, point, QueryFilter::new().visible().pickable())?;
        self.node_to_element.get(&hit.node).copied()
    }

    /// Returns the root-to-target element path for the topmost hit at a point.
    #[must_use]
    pub fn hit_path(&self, point: Point) -> Option<Vec<ElementId>> {
        let hit = top_hit_for_point(&self.tree, point, QueryFilter::new().visible().pickable())?;
        let path = hit.path?;
        let mut out = Vec::with_capacity(path.len());
        for node in path {
            if let Some(element) = self.node_to_element.get(&node).copied() {
                out.push(element);
            }
        }
        Some(out)
    }
}

struct SceneBuilder<'a> {
    elements: &'a [Element],
    registry: &'a PropertyRegistry,
    props: &'a BuiltInProperties,
    theme: &'a Theme,
    built_in_styles: &'a BuiltInStyles,
    widget_arena: &'a WidgetArena,
    text: &'a mut TextEngine,
    tree: &'a mut Tree,
    resolved: &'a mut Vec<ResolvedElement>,
    element_to_node: &'a mut [Option<NodeId>],
    node_to_element: &'a mut HashMap<NodeId, ElementId>,
    scroll_metrics: &'a mut Vec<(ElementId, f64, f64)>,
    next_z: i32,
}

impl<'a> SceneBuilder<'a> {
    fn layout_element(
        &mut self,
        id: ElementId,
        origin: Point,
        available_rect: Rect,
        parent_horizontal: Option<bool>,
        parent_node: Option<NodeId>,
        depth: u16,
    ) -> LayoutSize {
        let Some(element) = self.elements.get(id.index()) else {
            return LayoutSize::ZERO;
        };
        let style = self.resolve_style(element);
        if !style.visible {
            return LayoutSize::ZERO;
        }

        let measured_size = self.measure_size(
            Size::new(available_rect.width(), available_rect.height()),
            element,
            &style,
        );
        let is_root = element.is_root;
        let width = if is_root {
            available_rect.width()
        } else if style.width > 0.0 {
            clamp_explicit_dim(style.width, available_rect.width())
        } else if matches!(parent_horizontal, Some(true)) && !style.fill {
            measured_size.width
        } else {
            available_rect.width().max(0.0)
        };
        let height = if is_root {
            available_rect.height()
        } else if style.height > 0.0 {
            style.height.max(0.0)
        } else if matches!(parent_horizontal, Some(true))
            || (style.fill && matches!(parent_horizontal, Some(false)))
        {
            available_rect.height().max(0.0)
        } else {
            measured_size.height
        };

        let rect = Rect::new(origin.x, origin.y, origin.x + width, origin.y + height);
        let widget_ref = element.widget.and_then(|h| self.widget_arena.get(h));
        let flags = style.flags_for(widget_ref);
        let z_index = self.alloc_z();
        let node = self.tree.insert(
            parent_node,
            LocalNode {
                local_bounds: rect,
                local_transform: Affine::IDENTITY,
                local_clip: None,
                z_index,
                flags,
            },
        );
        self.element_to_node[id.index()] = Some(node);
        self.node_to_element.insert(node, id);

        self.resolved.push(ResolvedElement {
            id,
            type_tag: element.type_tag,
            depth,
            rect,
            background: style.background,
            foreground: style.foreground,
            border: BorderStyle {
                color: style.border_color,
                width: style.border_width,
            },
            corner_radius: style.corner_radius,
            text: element
                .widget
                .and_then(|h| self.widget_arena.get(h))
                .and_then(widget_text)
                .map(Box::from),
            hovered: element.pseudos.hovered,
            pressed: element.pseudos.pressed,
            focused: element.pseudos.focused,
            focus_visible: element.pseudos.focus_visible,
            disabled: !style.enabled,
            font_size: style.font_size,
            label_padding: style.label_padding,
            font_family: style.font_family.clone(),
            text_align: style.text_align,
            clips_content: element.widget.is_some() && element.type_tag == TYPE_SCROLL_VIEW,
            scroll_offset: if element.widget.is_some() && element.type_tag == TYPE_SCROLL_VIEW {
                element
                    .widget
                    .and_then(|h| self.widget_arena.get(h))
                    .and_then(|w| w.as_any().downcast_ref::<crate::widgets::ScrollView>())
                    .map_or(0.0, |w| w.scroll_offset())
            } else {
                0.0
            },
            widget: element.widget,
            semantics: resolve_semantics(element, widget_ref, !style.enabled),
        });

        let is_scroll_view = element.widget.is_some() && element.type_tag == TYPE_SCROLL_VIEW;
        if is_scroll_view {
            use kurbo::RoundedRect;
            self.tree.set_local_clip(
                node,
                Some(RoundedRect::from_rect(rect, style.corner_radius)),
            );
        }

        if element.is_container {
            let content = inset_rect(rect, style.padding);
            let horizontal = element.horizontal;
            let total_extent = if horizontal {
                content.width()
            } else {
                content.height()
            };

            // Pass 1: measure natural child sizes and count fill children.
            let mut used = 0.0_f64;
            let mut fill_count = 0_u32;
            let mut visible_count = 0_u32;
            let mut measured_children = Vec::with_capacity(element.children.len());
            for &child in &element.children {
                let Some(child_element) = self.elements.get(child.index()) else {
                    continue;
                };
                let child_style = self.resolve_style(child_element);
                if !child_style.visible {
                    measured_children.push(LayoutSize::ZERO);
                    continue;
                }
                let measured_child = self.measure_size(
                    Size::new(content.width(), content.height()),
                    child_element,
                    &child_style,
                );
                measured_children.push(measured_child);
                if self.is_promoted_overlay(child_element) {
                    continue;
                }
                visible_count += 1;
                if child_style.fill {
                    fill_count += 1;
                } else {
                    let extent = if horizontal {
                        measured_child.width
                    } else {
                        measured_child.height
                    };
                    used += extent;
                }
            }
            let gap_total = if visible_count > 1 {
                style.gap * f64::from(visible_count - 1)
            } else {
                0.0
            };
            let fill_extent = if fill_count > 0 {
                ((total_extent - used - gap_total) / f64::from(fill_count)).max(0.0)
            } else {
                0.0
            };

            // Pass 2: lay out all children with fill children using their share.
            let mut cursor = if horizontal { content.x0 } else { content.y0 };
            let mut previous_visible = false;
            for (index, &child) in element.children.iter().enumerate() {
                let Some(child_element) = self.elements.get(child.index()) else {
                    continue;
                };
                let child_style = self.resolve_style(child_element);
                if !child_style.visible {
                    continue;
                }
                let measured_child = measured_children[index];
                if self.is_promoted_overlay(child_element) {
                    let child_rect =
                        self.overlay_rect_for(child_element, &child_style, measured_child, content);
                    let _ = self.layout_element(
                        child,
                        child_rect.origin(),
                        child_rect,
                        Some(horizontal),
                        Some(node),
                        depth + 1,
                    );
                    continue;
                }
                if previous_visible {
                    cursor += style.gap;
                }
                let (child_origin, child_rect) = if horizontal {
                    let child_w = if child_style.fill {
                        fill_extent
                    } else {
                        measured_child.width
                    };
                    (
                        Point::new(cursor, content.y0),
                        Rect::new(cursor, content.y0, cursor + child_w, content.y1),
                    )
                } else {
                    let cw = content.width().max(0.0);
                    if child_style.fill {
                        (
                            Point::new(content.x0, cursor),
                            Rect::new(content.x0, cursor, content.x0 + cw, cursor + fill_extent),
                        )
                    } else {
                        (
                            Point::new(content.x0, cursor),
                            Rect::new(content.x0, cursor, content.x0 + cw, content.y1),
                        )
                    }
                };
                let child_size = self.layout_element(
                    child,
                    child_origin,
                    child_rect,
                    Some(horizontal),
                    Some(node),
                    depth + 1,
                );
                cursor += if horizontal {
                    child_size.width
                } else {
                    child_size.height
                };
                previous_visible = true;

                // Apply scroll transform to child box-tree nodes.
                if is_scroll_view && let Some(child_node) = self.element_to_node[child.index()] {
                    self.tree.set_local_transform(
                        child_node,
                        Affine::translate((
                            0.0,
                            -element
                                .widget
                                .and_then(|h| self.widget_arena.get(h))
                                .and_then(|w| {
                                    w.as_any().downcast_ref::<crate::widgets::ScrollView>()
                                })
                                .map_or(0.0, |w| w.scroll_offset()),
                        )),
                    );
                }
            }

            // Record measured content extent for ScrollView write-back.
            // Include padding on both sides so scrolling reveals the full
            // content area.
            if is_scroll_view {
                let content_start = if horizontal { content.x0 } else { content.y0 };
                let content_extent = (cursor - content_start + style.padding).max(0.0);
                self.scroll_metrics.push((id, content_extent, height));
            }
        }

        LayoutSize { width, height }
    }

    fn measure_size(
        &mut self,
        available: Size,
        element: &Element,
        style: &ResolvedStyle,
    ) -> LayoutSize {
        if !style.visible || element.is_root {
            return LayoutSize::ZERO;
        }

        if let Some(handle) = element.widget
            && let Some(widget) = self.widget_arena.get(handle)
        {
            let mut ctx = crate::MeasureCtx::new(self.text);
            if let Some(measured) = widget.measure(available, &style.measure_style(), &mut ctx) {
                return LayoutSize {
                    width: if style.width > 0.0 {
                        clamp_explicit_dim(style.width, available.width)
                    } else {
                        measured.width.max(0.0)
                    },
                    height: if style.height > 0.0 {
                        style.height.max(0.0)
                    } else {
                        measured.height.max(0.0)
                    },
                };
            }
        }

        if !element.is_container {
            return LayoutSize {
                width: if style.width > 0.0 {
                    clamp_explicit_dim(style.width, available.width)
                } else {
                    0.0
                },
                height: if style.height > 0.0 {
                    style.height.max(0.0)
                } else {
                    0.0
                },
            };
        }

        let base_width = if style.width > 0.0 {
            clamp_explicit_dim(style.width, available.width)
        } else {
            0.0
        };
        if element.horizontal {
            let mut total_width = style.padding * 2.0;
            let mut max_height = 0.0_f64;
            let mut visible_children = 0_u32;
            for &child in &element.children {
                let Some(child_element) = self.elements.get(child.index()) else {
                    continue;
                };
                let child_style = self.resolve_style(child_element);
                if self.is_promoted_overlay(child_element) {
                    continue;
                }
                let child_size = self.measure_size(
                    Size::new(available.width, available.height),
                    child_element,
                    &child_style,
                );
                if child_size.width > 0.0 || child_size.height > 0.0 {
                    if visible_children > 0 {
                        total_width += style.gap;
                    }
                    total_width += child_size.width;
                    max_height = max_height.max(child_size.height);
                    visible_children += 1;
                }
            }
            LayoutSize {
                width: if style.width > 0.0 {
                    base_width
                } else {
                    total_width.max(0.0)
                },
                height: if style.height > 0.0 {
                    style.height.max(0.0)
                } else {
                    (style.padding * 2.0 + max_height).max(0.0)
                },
            }
        } else {
            let mut max_width = 0.0_f64;
            let mut total_height = style.padding * 2.0;
            let mut visible_children = 0_u32;
            for &child in &element.children {
                let Some(child_element) = self.elements.get(child.index()) else {
                    continue;
                };
                let child_style = self.resolve_style(child_element);
                if self.is_promoted_overlay(child_element) {
                    continue;
                }
                let child_size = self.measure_size(
                    Size::new(available.width, available.height),
                    child_element,
                    &child_style,
                );
                if child_size.width > 0.0 || child_size.height > 0.0 {
                    if visible_children > 0 {
                        total_height += style.gap;
                    }
                    total_height += child_size.height;
                    max_width = max_width.max(child_size.width);
                    visible_children += 1;
                }
            }
            LayoutSize {
                width: if style.width > 0.0 {
                    base_width
                } else {
                    (style.padding * 2.0 + max_width).max(0.0)
                },
                height: if style.height > 0.0 {
                    style.height.max(0.0)
                } else {
                    total_height.max(0.0)
                },
            }
        }
    }

    fn resolve_style(&self, element: &Element) -> ResolvedStyle {
        let lookup = |key: ElementId| -> Option<(&PropertyStore<ElementId>, Option<ElementId>)> {
            self.elements
                .get(key.index())
                .map(|entry| (&entry.store, entry.parent))
        };
        let cx = ResolveCx::new(self.registry, self.theme, lookup);
        let enabled = element.get_effective_local(self.props.enabled, self.registry);
        let pseudos = build_pseudos(element, enabled);
        let inputs = element.selector_inputs(&pseudos);
        let built_in_style = self.built_in_styles.for_element(element);
        let combined_style = compose_style_cascades(built_in_style, element.style.as_ref());
        let foreground_key = Some(ThemeKeys::FOREGROUND);
        let border_key = Some(ThemeKeys::BORDER_COLOR);
        let radius_key = Some(ThemeKeys::CORNER_RADIUS);
        let padding_key = if element.is_container {
            Some(ThemeKeys::PADDING)
        } else {
            None
        };
        let gap_key = if element.is_container {
            Some(ThemeKeys::GAP)
        } else {
            None
        };

        let mut resolved = ResolvedStyle {
            width: cx.get_value(element, &inputs, self.props.width, combined_style.as_ref()),
            height: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.height,
                combined_style.as_ref(),
                None,
            ),
            padding: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.padding,
                combined_style.as_ref(),
                padding_key,
            ),
            gap: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.gap,
                combined_style.as_ref(),
                gap_key,
            ),
            background: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.background,
                combined_style.as_ref(),
                None,
            ),
            foreground: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.foreground,
                combined_style.as_ref(),
                foreground_key,
            ),
            border_color: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.border_color,
                combined_style.as_ref(),
                border_key,
            ),
            border_width: cx.get_value(
                element,
                &inputs,
                self.props.border_width,
                combined_style.as_ref(),
            ),
            corner_radius: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.corner_radius,
                combined_style.as_ref(),
                radius_key,
            ),
            visible: cx.get_value(
                element,
                &inputs,
                self.props.visible,
                combined_style.as_ref(),
            ),
            enabled,
            pickable: cx.get_value(
                element,
                &inputs,
                self.props.pickable,
                combined_style.as_ref(),
            ),
            focusable: cx.get_value(
                element,
                &inputs,
                self.props.focusable,
                combined_style.as_ref(),
            ),
            fill: cx.get_value(element, &inputs, self.props.fill, combined_style.as_ref()),
            font_size: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.font_size,
                combined_style.as_ref(),
                Some(ThemeKeys::FONT_SIZE),
            ),
            label_padding: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.label_padding,
                combined_style.as_ref(),
                Some(ThemeKeys::LABEL_PADDING),
            ),
            font_family: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.font_family,
                combined_style.as_ref(),
                Some(ThemeKeys::FONT_FAMILY),
            ),
            text_align: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.text_align,
                combined_style.as_ref(),
                Some(ThemeKeys::TEXT_ALIGN),
            ),
        };
        // Guarantee that text properties are never zero/empty — widgets
        // should be able to read them directly without fallback logic.
        if resolved.font_size <= 0.0 {
            resolved.font_size = 16.0;
        }
        if resolved.label_padding <= 0.0 {
            resolved.label_padding = 12.0;
        }
        if resolved.font_family.is_empty() {
            resolved.font_family = Box::from("sans-serif");
        }
        resolved
    }

    fn is_promoted_overlay(&self, element: &Element) -> bool {
        element
            .widget
            .and_then(|handle| self.widget_arena.get(handle))
            .and_then(|widget| widget.surface_role())
            .is_some()
    }

    fn overlay_origin(&self, element: &Element, fallback: Point) -> Point {
        let Some(handle) = element.widget else {
            return fallback;
        };
        let Some(widget) = self.widget_arena.get(handle) else {
            return fallback;
        };
        if let Some(tooltip) = widget.as_any().downcast_ref::<crate::widgets::Tooltip>() {
            return tooltip.position().unwrap_or(fallback);
        }
        if let Some(dropdown) = widget.as_any().downcast_ref::<crate::widgets::Dropdown>() {
            return dropdown.position().unwrap_or(fallback);
        }
        fallback
    }

    fn overlay_rect_for(
        &self,
        element: &Element,
        style: &ResolvedStyle,
        measured: LayoutSize,
        content: Rect,
    ) -> Rect {
        let origin = self.overlay_origin(element, content.origin());
        let width = if style.width > 0.0 {
            clamp_explicit_dim(style.width, content.width())
        } else {
            measured.width.max(0.0)
        };
        let height = if style.height > 0.0 {
            style.height.max(0.0)
        } else {
            measured.height.max(0.0)
        };
        Rect::from_origin_size(origin, Size::new(width, height))
    }

    fn alloc_z(&mut self) -> i32 {
        let value = self.next_z;
        self.next_z = self.next_z.saturating_add(1);
        value
    }
}

#[derive(Clone, Debug)]
struct ResolvedStyle {
    width: f64,
    height: f64,
    padding: f64,
    gap: f64,
    background: Color,
    foreground: Color,
    border_color: Color,
    border_width: f64,
    corner_radius: f64,
    visible: bool,
    enabled: bool,
    pickable: bool,
    focusable: bool,
    fill: bool,
    font_size: f64,
    label_padding: f64,
    font_family: Box<str>,
    text_align: TextAlign,
}

impl ResolvedStyle {
    fn measure_style(&self) -> MeasureStyle<'_> {
        MeasureStyle {
            width: self.width,
            height: self.height,
            padding: self.padding,
            gap: self.gap,
            fill: self.fill,
            font_size: self.font_size,
            label_padding: self.label_padding,
            font_family: &self.font_family,
            text_align: self.text_align,
        }
    }

    fn flags_for(&self, widget: Option<&dyn Widget>) -> NodeFlags {
        let mut flags = NodeFlags::VISIBLE;
        if self.enabled && (self.pickable || widget.is_some_and(|w| w.default_pickable())) {
            flags |= NodeFlags::PICKABLE;
        }
        if self.enabled && (self.focusable || widget.is_some_and(|w| w.default_focusable())) {
            flags |= NodeFlags::FOCUSABLE;
        }
        flags
    }
}

#[derive(Copy, Clone, Debug, Default)]
struct LayoutSize {
    width: f64,
    height: f64,
}

impl LayoutSize {
    const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };
}

fn build_pseudos(element: &Element, enabled: bool) -> Vec<PseudoClassId> {
    let mut pseudos = Vec::with_capacity(5);
    if element.pseudos.hovered {
        pseudos.push(PSEUDO_HOVER);
    }
    if element.pseudos.pressed {
        pseudos.push(PSEUDO_PRESSED);
    }
    if !enabled {
        pseudos.push(PSEUDO_DISABLED);
    }
    if element.pseudos.focused {
        pseudos.push(PSEUDO_FOCUSED);
    }
    if element.pseudos.focus_visible {
        pseudos.push(PSEUDO_FOCUS_VISIBLE);
    }
    pseudos
}

fn resolve_semantics(
    element: &Element,
    widget: Option<&dyn Widget>,
    disabled: bool,
) -> SemanticInfo {
    let role = widget
        .map(|widget| widget.semantic_role())
        .unwrap_or_else(|| role_for_type_tag(element.type_tag()));
    let name = widget
        .and_then(|widget| widget.semantic_name())
        .map(CowIntoBoxStr::into_box_str)
        .or_else(|| element.display_name().map(Box::from));
    let value = widget
        .and_then(|widget| widget.semantic_value())
        .map(CowIntoBoxStr::into_box_str);
    let busy = widget.is_some_and(|widget| widget.semantic_busy());

    SemanticInfo {
        role,
        name,
        value,
        state: SemanticState {
            disabled,
            focused: element.pseudos.focused,
            focus_visible: element.pseudos.focus_visible,
            hovered: element.pseudos.hovered,
            pressed: element.pseudos.pressed,
            busy,
        },
    }
}

fn role_for_type_tag(type_tag: TypeTag) -> SemanticRole {
    match type_tag {
        TYPE_ROOT | TYPE_PANEL | TYPE_ROW | TYPE_COLUMN => SemanticRole::Group,
        TYPE_BUTTON => SemanticRole::Button,
        TYPE_SCROLL_VIEW => SemanticRole::ScrollArea,
        TYPE_TEXT_BLOCK => SemanticRole::Text,
        TYPE_TEXT_INPUT => SemanticRole::TextInput,
        TYPE_TOOLTIP => SemanticRole::Tooltip,
        TYPE_DROPDOWN => SemanticRole::Generic,
        TYPE_SPLITTER => SemanticRole::Splitter,
        TYPE_DIVIDER => SemanticRole::Separator,
        TYPE_SPINNER => SemanticRole::ProgressIndicator,
        _ => SemanticRole::Generic,
    }
}

trait CowIntoBoxStr {
    fn into_box_str(self) -> Box<str>;
}

impl CowIntoBoxStr for Cow<'_, str> {
    fn into_box_str(self) -> Box<str> {
        match self {
            Cow::Borrowed(text) => Box::from(text),
            Cow::Owned(text) => text.into_boxed_str(),
        }
    }
}

/// Clamps an explicit dimension to the available space.
fn clamp_explicit_dim(style_value: f64, available: f64) -> f64 {
    style_value.min(available).max(0.0)
}

fn inset_rect(rect: Rect, inset: f64) -> Rect {
    Rect::new(
        rect.x0 + inset,
        rect.y0 + inset,
        (rect.x1 - inset).max(rect.x0 + inset),
        (rect.y1 - inset).max(rect.y0 + inset),
    )
}

fn compose_style_cascades(
    widget_style: Option<&StyleCascade>,
    element_style: Option<&StyleCascade>,
) -> Option<StyleCascade> {
    let mut builder = StyleCascadeBuilder::new();
    let mut any = false;

    for cascade in [widget_style, element_style].into_iter().flatten() {
        for source in cascade.sources() {
            any = true;
            builder = match source {
                StyleSource::Style { origin, style } => builder.push_style(*origin, style.clone()),
                StyleSource::Sheet { origin, sheet } => builder.push_sheet(*origin, sheet.clone()),
            };
        }
    }

    any.then(|| builder.build())
}
