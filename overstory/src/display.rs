// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Overstory-to-display tree projection.

use alloc::vec::Vec;

use kurbo::{Point, Vec2};
use peniko::Brush;
use understory_display::{DisplayNode, DisplayTree};

use crate::{ResolvedElement, SceneSnapshot, WidgetArena};

#[derive(Debug)]
struct ElementDisplayTree<'a> {
    element: &'a ResolvedElement,
    children: Vec<Self>,
}

impl SceneSnapshot {
    /// Builds a retained display tree from the current resolved Overstory scene.
    ///
    /// This keeps `ResolvedElement` available as a debug/projection artifact,
    /// while giving embedders a direct retained visual tree to measure, place,
    /// and lower into paint backends.
    #[must_use]
    pub fn display_tree(&self, widget_arena: &WidgetArena) -> DisplayTree {
        let root_origin = Point::new(self.view_rect().x0, self.view_rect().y0);
        let mut index = 0;
        let display_root = build_element_tree(self.resolved(), &mut index, &[])
            .expect("scene snapshot should contain a root resolved element");
        DisplayTree::new(display_node_for(root_origin, &display_root, widget_arena))
    }

    /// Builds a display tree for the root surface, excluding promoted elements.
    #[must_use]
    pub fn display_tree_excluding(
        &self,
        widget_arena: &WidgetArena,
        skip_ids: &[crate::ElementId],
    ) -> DisplayTree {
        let root_origin = Point::new(self.view_rect().x0, self.view_rect().y0);
        let mut index = 0;
        let display_root = build_element_tree(self.resolved(), &mut index, skip_ids)
            .expect("scene snapshot should contain a root resolved element");
        DisplayTree::new(display_node_for(root_origin, &display_root, widget_arena))
    }

    /// Builds a display tree for a single promoted element subtree.
    #[must_use]
    pub fn display_tree_for(
        &self,
        widget_arena: &WidgetArena,
        element_id: crate::ElementId,
    ) -> Option<DisplayTree> {
        let resolved = self.resolved();
        let pos = resolved.iter().position(|r| r.id == element_id)?;
        let mut index = pos;
        let element_tree = build_element_tree(resolved, &mut index, &[])?;
        let origin = Point::new(element_tree.element.rect.x0, element_tree.element.rect.y0);
        Some(DisplayTree::new(display_node_for(
            origin,
            &element_tree,
            widget_arena,
        )))
    }
}

fn build_element_tree<'a>(
    resolved: &'a [ResolvedElement],
    index: &mut usize,
    skip_ids: &[crate::ElementId],
) -> Option<ElementDisplayTree<'a>> {
    let element = resolved.get(*index)?;
    let depth = element.depth;
    *index += 1;

    let mut children = Vec::new();
    while let Some(next) = resolved.get(*index) {
        if next.depth <= depth {
            break;
        }
        if next.depth != depth + 1 {
            panic!("resolved scene depth should advance one level at a time");
        }
        // Skip promoted elements and their entire subtrees.
        if skip_ids.contains(&next.id) {
            skip_subtree(resolved, index);
            continue;
        }
        let child =
            build_element_tree(resolved, index, skip_ids).expect("child subtree should parse");
        children.push(child);
    }

    Some(ElementDisplayTree { element, children })
}

/// Advances `index` past an element and all its descendants.
fn skip_subtree(resolved: &[ResolvedElement], index: &mut usize) {
    let element = &resolved[*index];
    let depth = element.depth;
    *index += 1;
    while let Some(next) = resolved.get(*index) {
        if next.depth <= depth {
            break;
        }
        *index += 1;
    }
}

fn display_node_for(
    parent_origin: Point,
    node: &ElementDisplayTree<'_>,
    widget_arena: &WidgetArena,
) -> DisplayNode {
    let element = node.element;
    let mut children = Vec::new();
    let size = element.rect.size();

    if element.background.to_rgba8().a != 0 {
        let background = Brush::Solid(element.background);
        children.push(if element.corner_radius > 0.0 {
            DisplayNode::fill_rounded_rect(element.corner_radius, background)
        } else {
            DisplayNode::fill_rect(background)
        });
    }

    if element.border.width > 0.0 && element.border.color.to_rgba8().a != 0 {
        let border = Brush::Solid(element.border.color);
        let stroke = kurbo::Stroke::new(element.border.width);
        children.push(if element.corner_radius > 0.0 {
            DisplayNode::stroke_rounded_rect(element.corner_radius, stroke, border)
        } else {
            DisplayNode::stroke_rect(stroke, border)
        });
    }

    // Delegate label and visual content rendering to the widget if present.
    if let Some(handle) = element.widget
        && let Some(widget) = widget_arena.get(handle)
    {
        widget.display(element.id, element, &mut children);
    }

    let child_nodes: Vec<DisplayNode> = node
        .children
        .iter()
        .map(|child| display_node_for(element.rect.origin(), child, widget_arena))
        .collect();

    if element.clips_content && !child_nodes.is_empty() {
        children.push(DisplayNode::clip_rect(DisplayNode::offset(
            Vec2::new(0.0, -element.scroll_offset),
            DisplayNode::stack(child_nodes),
        )));
    } else {
        children.extend(child_nodes);
    }

    DisplayNode::offset(
        Vec2::new(
            element.rect.x0 - parent_origin.x,
            element.rect.y0 - parent_origin.y,
        ),
        DisplayNode::fixed_frame(size, DisplayNode::stack(children)),
    )
}

#[cfg(test)]
mod tests {
    use crate::{Ui, default_theme};

    #[test]
    fn display_tree_preserves_scene_parent_child_structure() {
        let mut ui = Ui::new(default_theme());
        ui.set_view_rect(kurbo::Rect::new(0.0, 0.0, 220.0, 140.0));
        ui.set_local(ui.root(), ui.properties().padding, 0.0);
        ui.set_local(ui.root(), ui.properties().gap, 10.0);

        let shell = ui.append_child(ui.root(), crate::TYPE_ROW);
        ui.set_local(shell, ui.properties().padding, 0.0);
        ui.set_local(shell, ui.properties().gap, 10.0);

        let left = ui.append_child(shell, crate::TYPE_PANEL);
        ui.set_local(left, ui.properties().width, 80.0);
        ui.set_local(left, ui.properties().padding, 8.0);

        let child = ui.append_child(left, crate::TYPE_BUTTON);
        ui.set_label(child, "Child");
        ui.set_local(child, ui.properties().height, 28.0);

        let right = ui.append_child(shell, crate::TYPE_PANEL);
        ui.set_local(right, ui.properties().padding, 8.0);

        let (tree, _) = ui.display_tree();
        let root = tree.root();
        let understory_display::DisplayNodeKind::Offset { child, .. } = root.kind() else {
            panic!("expected root offset node");
        };
        let understory_display::DisplayNodeKind::FixedFrame { child, .. } = child.kind() else {
            panic!("expected root fixed frame");
        };
        let understory_display::DisplayNodeKind::Stack { children } = child.kind() else {
            panic!("expected root stack");
        };

        assert!(
            children.len() >= 2,
            "root stack should contain the background plus the shell row"
        );
        let shell_node = children.last().expect("shell row should be present");
        let understory_display::DisplayNodeKind::Offset { child, .. } = shell_node.kind() else {
            panic!("expected shell offset node");
        };
        let understory_display::DisplayNodeKind::FixedFrame { child, .. } = child.kind() else {
            panic!("expected shell fixed frame");
        };
        let understory_display::DisplayNodeKind::Stack { children } = child.kind() else {
            panic!("expected shell stack");
        };

        assert_eq!(children.len(), 2, "shell should contain both panels");
    }
}
