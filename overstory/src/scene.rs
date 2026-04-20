// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Derived scene snapshot over the retained Overstory element tree.

use alloc::{boxed::Box, vec, vec::Vec};

use hashbrown::HashMap;
use kurbo::{Affine, Point, Rect};
use understory_box_tree::{LocalNode, NodeFlags, NodeId, QueryFilter, Tree};
use understory_property::{PropertyRegistry, PropertyStore};
use understory_responder::adapters::box_tree::top_hit_for_point;
use understory_style::{ResolveCx, ResourceKey, Theme};

use crate::{
    BuiltInProperties, ButtonClass, Color, Element, ElementId, ElementKind, LayoutClass,
    PSEUDO_DISABLED, PSEUDO_HOVER, PSEUDO_PRESSED, ThemeKeys,
};

/// Border styling for one resolved element.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct BorderStyle {
    /// Border color.
    pub color: Color,
    /// Border width in scene units.
    pub width: f64,
}

/// One resolved retained element, suitable for a renderer-facing adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedElement {
    /// Retained element id.
    pub id: ElementId,
    /// Element kind.
    pub kind: ElementKind,
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
    /// Optional label text.
    pub label: Option<Box<str>>,
    /// Hover state at snapshot time.
    pub hovered: bool,
    /// Press state at snapshot time.
    pub pressed: bool,
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
    ) -> Self {
        let mut tree = Tree::new();
        let mut resolved = Vec::new();
        let mut element_to_node = vec![None; elements.len()];
        let mut node_to_element = HashMap::new();
        let mut builder = SceneBuilder {
            elements,
            registry,
            props,
            theme,
            tree: &mut tree,
            resolved: &mut resolved,
            element_to_node: &mut element_to_node,
            node_to_element: &mut node_to_element,
            next_z: 0,
        };
        let _ = builder.layout_element(
            root,
            Point::new(view_rect.x0, view_rect.y0),
            view_rect,
            None,
            0,
        );
        let _ = tree.commit();
        Self {
            view_rect,
            tree,
            resolved,
            element_to_node,
            node_to_element,
        }
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
    tree: &'a mut Tree,
    resolved: &'a mut Vec<ResolvedElement>,
    element_to_node: &'a mut [Option<NodeId>],
    node_to_element: &'a mut HashMap<NodeId, ElementId>,
    next_z: i32,
}

impl<'a> SceneBuilder<'a> {
    fn layout_element(
        &mut self,
        id: ElementId,
        origin: Point,
        available_rect: Rect,
        parent_node: Option<NodeId>,
        depth: u16,
    ) -> f64 {
        let Some(element) = self.elements.get(id.index()) else {
            return 0.0;
        };
        let style = self.resolve_style(element);
        if !style.visible {
            return 0.0;
        }

        let measured_height = self.measure_height(id, available_rect.width());
        let width = if matches!(element.kind, ElementKind::Root) {
            available_rect.width()
        } else if style.width > 0.0 {
            style.width.min(available_rect.width()).max(0.0)
        } else {
            available_rect.width().max(0.0)
        };
        let height = if matches!(element.kind, ElementKind::Root) {
            available_rect.height()
        } else {
            measured_height
        };

        let rect = Rect::new(origin.x, origin.y, origin.x + width, origin.y + height);
        let flags = style.flags_for(element.kind);
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
            kind: element.kind,
            depth,
            rect,
            background: style.background,
            foreground: style.foreground,
            border: BorderStyle {
                color: style.border_color,
                width: style.border_width,
            },
            corner_radius: style.corner_radius,
            label: element.label.clone(),
            hovered: element.pseudos.hovered,
            pressed: element.pseudos.pressed,
        });

        if matches!(
            element.kind,
            ElementKind::Root | ElementKind::Panel | ElementKind::Column
        ) {
            let content = inset_rect(rect, style.padding);
            let mut y = content.y0;
            let child_width = content.width().max(0.0);
            let mut previous_visible = false;
            for &child in &element.children {
                let Some(child_element) = self.elements.get(child.index()) else {
                    continue;
                };
                let child_style = self.resolve_style(child_element);
                if !child_style.visible {
                    continue;
                }
                if previous_visible {
                    y += style.gap;
                }
                let child_rect = Rect::new(content.x0, y, content.x0 + child_width, content.y1);
                let child_height = self.layout_element(
                    child,
                    Point::new(content.x0, y),
                    child_rect,
                    Some(node),
                    depth + 1,
                );
                y += child_height;
                previous_visible = true;
            }
        }

        height
    }

    fn measure_height(&self, id: ElementId, available_width: f64) -> f64 {
        let Some(element) = self.elements.get(id.index()) else {
            return 0.0;
        };
        let style = self.resolve_style(element);
        if !style.visible {
            return 0.0;
        }

        if matches!(element.kind, ElementKind::Root) {
            return 0.0;
        }

        if style.height > 0.0 {
            return style.height;
        }

        match element.kind {
            ElementKind::Button => style.height.max(0.0),
            ElementKind::Spacer => 0.0,
            ElementKind::Panel | ElementKind::Column | ElementKind::Root => {
                let width = if style.width > 0.0 {
                    style.width.min(available_width)
                } else {
                    available_width
                };
                let child_width = (width - style.padding * 2.0).max(0.0);
                let mut total = style.padding * 2.0;
                let mut visible_children = 0_u32;
                for &child in &element.children {
                    let height = self.measure_height(child, child_width);
                    if height > 0.0 {
                        if visible_children > 0 {
                            total += style.gap;
                        }
                        total += height;
                        visible_children += 1;
                    }
                }
                total.max(0.0)
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
        let pseudos = build_pseudos(element);
        let inputs = element.selector_inputs(&pseudos);
        let background_key = background_resource_for(element);
        let foreground_key = Some(ThemeKeys::FOREGROUND);
        let border_key = Some(ThemeKeys::BORDER_COLOR);
        let radius_key = Some(ThemeKeys::CORNER_RADIUS);
        let padding_key = match element.kind {
            ElementKind::Root | ElementKind::Panel | ElementKind::Column => {
                Some(ThemeKeys::PADDING)
            }
            _ => None,
        };
        let gap_key = match element.kind {
            ElementKind::Root | ElementKind::Panel | ElementKind::Column => Some(ThemeKeys::GAP),
            _ => None,
        };
        let height_key = match element.kind {
            ElementKind::Button => Some(ThemeKeys::BUTTON_HEIGHT),
            _ => None,
        };

        ResolvedStyle {
            width: cx.get_value(element, &inputs, self.props.width, element.style.as_ref()),
            height: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.height,
                element.style.as_ref(),
                height_key,
            ),
            padding: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.padding,
                element.style.as_ref(),
                padding_key,
            ),
            gap: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.gap,
                element.style.as_ref(),
                gap_key,
            ),
            background: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.background,
                element.style.as_ref(),
                background_key,
            ),
            foreground: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.foreground,
                element.style.as_ref(),
                foreground_key,
            ),
            border_color: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.border_color,
                element.style.as_ref(),
                border_key,
            ),
            border_width: cx.get_value(
                element,
                &inputs,
                self.props.border_width,
                element.style.as_ref(),
            ),
            corner_radius: cx.get_value_with_theme(
                element,
                &inputs,
                self.props.corner_radius,
                element.style.as_ref(),
                radius_key,
            ),
            visible: cx.get_value(element, &inputs, self.props.visible, element.style.as_ref()),
            pickable: cx.get_value(
                element,
                &inputs,
                self.props.pickable,
                element.style.as_ref(),
            ),
            focusable: cx.get_value(
                element,
                &inputs,
                self.props.focusable,
                element.style.as_ref(),
            ),
        }
    }
    fn alloc_z(&mut self) -> i32 {
        let value = self.next_z;
        self.next_z = self.next_z.saturating_add(1);
        value
    }
}

#[derive(Copy, Clone, Debug)]
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
    pickable: bool,
    focusable: bool,
}

impl ResolvedStyle {
    fn flags_for(self, kind: ElementKind) -> NodeFlags {
        let mut flags = NodeFlags::VISIBLE;
        if self.pickable || matches!(kind, ElementKind::Button) {
            flags |= NodeFlags::PICKABLE;
        }
        if self.focusable || matches!(kind, ElementKind::Button) {
            flags |= NodeFlags::FOCUSABLE;
        }
        flags
    }
}

fn build_pseudos(element: &Element) -> Vec<understory_style::PseudoClassId> {
    let mut pseudos = Vec::with_capacity(3);
    if element.pseudos.hovered {
        pseudos.push(PSEUDO_HOVER);
    }
    if element.pseudos.pressed {
        pseudos.push(PSEUDO_PRESSED);
    }
    if element.pseudos.disabled {
        pseudos.push(PSEUDO_DISABLED);
    }
    pseudos
}

fn inset_rect(rect: Rect, inset: f64) -> Rect {
    Rect::new(
        rect.x0 + inset,
        rect.y0 + inset,
        (rect.x1 - inset).max(rect.x0 + inset),
        (rect.y1 - inset).max(rect.y0 + inset),
    )
}

fn background_resource_for(element: &Element) -> Option<ResourceKey> {
    match element.kind {
        ElementKind::Root => Some(ThemeKeys::ROOT_BACKGROUND),
        ElementKind::Panel => {
            if element.classes.contains(LayoutClass::Sidebar.class_id()) {
                Some(ThemeKeys::SIDEBAR_BACKGROUND)
            } else {
                Some(ThemeKeys::PANEL_BACKGROUND)
            }
        }
        ElementKind::Button => {
            let primary = element.classes.contains(ButtonClass::Primary.class_id());
            match (primary, element.pseudos.pressed, element.pseudos.hovered) {
                (true, true, _) => Some(ThemeKeys::PRIMARY_PRESSED_BACKGROUND),
                (true, false, true) => Some(ThemeKeys::PRIMARY_HOVER_BACKGROUND),
                (true, false, false) => Some(ThemeKeys::PRIMARY_BACKGROUND),
                (false, true, _) => Some(ThemeKeys::BUTTON_PRESSED_BACKGROUND),
                (false, false, true) => Some(ThemeKeys::BUTTON_HOVER_BACKGROUND),
                (false, false, false) => Some(ThemeKeys::BUTTON_BACKGROUND),
            }
        }
        ElementKind::Column | ElementKind::Spacer => None,
    }
}
