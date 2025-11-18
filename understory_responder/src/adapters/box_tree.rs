// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Adapter helpers for Understory Box Tree.
//!
//! ## Feature
//!
//! Enable with `box_tree_adapter`.
//!
//! ## Notes
//!
//! These helpers convert box-tree query results into responder hits.
//! They do not perform ordering; when only a single candidate exists (e.g., top hit), the depth key value is irrelevant.
//! For lists (e.g., viewport queries), consumers can apply their own ordering if needed.
//!
//! ## Navigation
//!
//! The [`navigation`] module provides filtered tree traversal with wraparound semantics,
//! suitable for keyboard navigation, focus cycling, and similar UI interactions.
//! These functions extend the basic box tree traversal with [`QueryFilter`] support and
//! circular navigation within subtrees.

use alloc::vec::Vec;

use kurbo::{Point, Rect};
use understory_box_tree::{QueryFilter, Tree};

use crate::types::{DepthKey, Localizer, ResolvedHit};

/// Build a single resolved hit for the topmost node under a point.
///
/// Returns `None` if no node matches the filter.
///
/// Notes
/// - Path is populated from the box tree's hit test result so the router does
///   not need a parent lookup.
/// - `DepthKey` is derived from the node's z-index; since only a single candidate
///   is returned, ordering is irrelevant.
pub fn top_hit_for_point(
    tree: &Tree,
    pt: Point,
    filter: QueryFilter,
) -> Option<ResolvedHit<understory_box_tree::NodeId, ()>> {
    let hit = tree.hit_test_point(pt, filter)?;
    let depth_key = tree
        .z_index(hit.node)
        .map(DepthKey::Z)
        .unwrap_or(DepthKey::Z(0));
    Some(ResolvedHit {
        node: hit.node,
        path: Some(hit.path),
        depth_key,
        localizer: Localizer::default(),
        meta: (),
    })
}

/// Build resolved hits for nodes intersecting a world-space rectangle.
///
/// Path is not populated; the router can reconstruct a singleton path (or a
/// parent-aware path if constructed with a parent lookup). Depth keys are set
/// to each node's z-index; the returned list preserves the box tree's original
/// iteration order so downstream consumers can sort as needed.
pub fn hits_for_rect(
    tree: &Tree,
    rect: Rect,
    filter: QueryFilter,
) -> Vec<ResolvedHit<understory_box_tree::NodeId, ()>> {
    tree.intersect_rect(rect, filter)
        .map(|id| ResolvedHit {
            node: id,
            path: None,
            depth_key: tree.z_index(id).map(DepthKey::Z).unwrap_or(DepthKey::Z(0)),
            localizer: Localizer::default(),
            meta: (),
        })
        .collect()
}

/// Tree navigation utilities for UI focus/keyboard traversal.
///
/// These methods provide filtered traversal with wraparound semantics,
/// suitable for keyboard navigation, focus cycling, and similar UI interactions.
pub mod navigation {
    use understory_box_tree::{NodeId, QueryFilter, Tree};

    /// Get the next node in depth-first traversal order that matches the filter.
    /// Returns `None` if no matching node exists or if the current node is stale.
    /// The search wraps around to the beginning of the subtree to enable circular navigation.
    pub fn next_depth_first_filtered(
        tree: &Tree,
        current: NodeId,
        filter: QueryFilter,
    ) -> Option<NodeId> {
        if !tree.is_alive(current) {
            return None;
        }

        let start = current;
        let mut node = current;

        // Keep searching until we find a match or complete a full cycle
        loop {
            if let Some(next) = tree.next_depth_first(node) {
                node = next;
            } else {
                // Wrap around to the root of this subtree when we reach the end
                node = find_root_of(tree, current)?;
            }

            // Check if we've completed a full cycle
            if node == start {
                break;
            }

            // Check if this node matches the filter
            if node_matches_filter(tree, node, filter) {
                return Some(node);
            }
        }

        None
    }

    /// Get the previous node in reverse depth-first traversal order that matches the filter.
    /// Returns `None` if no matching node exists or if the current node is stale.
    /// The search wraps around to the end of the subtree to enable circular navigation.
    pub fn prev_depth_first_filtered(
        tree: &Tree,
        current: NodeId,
        filter: QueryFilter,
    ) -> Option<NodeId> {
        if !tree.is_alive(current) {
            return None;
        }

        let start = current;
        let mut node = current;

        // Keep searching until we find a match or complete a full cycle
        loop {
            if let Some(prev) = tree.prev_depth_first(node) {
                node = prev;
            } else {
                // Wrap around to the last node of this subtree when we reach the beginning
                let root = find_root_of(tree, current)?;
                node = find_last_descendant(tree, root).unwrap_or(root);
            }

            // Check if we've completed a full cycle
            if node == start {
                break;
            }

            // Check if this node matches the filter
            if node_matches_filter(tree, node, filter) {
                return Some(node);
            }
        }

        None
    }

    /// Check if a node matches the given filter criteria.
    fn node_matches_filter(tree: &Tree, id: NodeId, filter: QueryFilter) -> bool {
        if !tree.is_alive(id) {
            return false;
        }

        let Some(flags) = tree.flags(id) else {
            return false;
        };

        filter.matches(flags)
    }

    /// Find the root node of the subtree containing the given node.
    fn find_root_of(tree: &Tree, mut node: NodeId) -> Option<NodeId> {
        if !tree.is_alive(node) {
            return None;
        }

        while let Some(parent) = tree.parent_of(node) {
            node = parent;
        }

        Some(node)
    }

    /// Find the rightmost (last in depth-first order) descendant of a node.
    fn find_last_descendant(tree: &Tree, mut node: NodeId) -> Option<NodeId> {
        loop {
            let children = tree.children_of(node);
            if let Some(&last_child) = children.last()
                && tree.is_alive(last_child)
            {
                node = last_child;
                continue;
            }
            return Some(node);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use kurbo::Rect;
        use understory_box_tree::{LocalNode, NodeFlags};

        #[test]
        fn filtered_traversal_visible_only() {
            let mut tree = Tree::new();

            // Build tree: root(visible) -> [a(hidden), b(visible) -> c(visible)]
            let root = tree.insert(
                None,
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );
            let _a = tree.insert(
                Some(root),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::empty(), // hidden
                    ..Default::default()
                },
            );
            let b = tree.insert(
                Some(root),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );
            let c = tree.insert(
                Some(b),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );

            let filter = QueryFilter {
                required_flags: NodeFlags::VISIBLE,
            };

            // From root, next visible should be b (skipping hidden a)
            let next = next_depth_first_filtered(&tree, root, filter).unwrap();
            assert_eq!(next, b);

            // From b, next visible should be c
            let next = next_depth_first_filtered(&tree, b, filter).unwrap();
            assert_eq!(next, c);

            // From c, should wrap around to root (no more visible after c)
            let next = next_depth_first_filtered(&tree, c, filter);
            assert_eq!(next, Some(root));

            // Reverse: from c, prev visible should be b
            let prev = prev_depth_first_filtered(&tree, c, filter).unwrap();
            assert_eq!(prev, b);

            // From b, prev visible should be root
            let prev = prev_depth_first_filtered(&tree, b, filter).unwrap();
            assert_eq!(prev, root);
        }

        #[test]
        fn filtered_traversal_pickable_only() {
            let mut tree = Tree::new();

            // Build tree where only some nodes are pickable
            let root = tree.insert(
                None,
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::PICKABLE,
                    ..Default::default()
                },
            );
            let _a = tree.insert(
                Some(root),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE, // visible but not pickable
                    ..Default::default()
                },
            );
            let b = tree.insert(
                Some(root),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::PICKABLE | NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );

            let filter = QueryFilter {
                required_flags: NodeFlags::PICKABLE,
            };

            // From root, next pickable should be b (skipping non-pickable a)
            let next = next_depth_first_filtered(&tree, root, filter).unwrap();
            assert_eq!(next, b);

            // From b, should wrap around to root
            let next = next_depth_first_filtered(&tree, b, filter);
            assert_eq!(next, Some(root));
        }

        #[test]
        fn filtered_traversal_no_matches() {
            let mut tree = Tree::new();

            // Build tree with no pickable nodes
            let root = tree.insert(
                None,
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );
            let a = tree.insert(
                Some(root),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );

            let filter = QueryFilter {
                required_flags: NodeFlags::PICKABLE,
            };

            // Should return None since no nodes are pickable
            assert!(next_depth_first_filtered(&tree, root, filter).is_none());
            assert!(prev_depth_first_filtered(&tree, root, filter).is_none());
            assert!(next_depth_first_filtered(&tree, a, filter).is_none());
        }

        #[test]
        fn filtered_traversal_wraparound() {
            let mut tree = Tree::new();

            // Build tree: root -> [visible_child, hidden_child]
            let root = tree.insert(
                None,
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );
            let visible_child = tree.insert(
                Some(root),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );
            let _hidden_child = tree.insert(
                Some(root),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::empty(), // hidden
                    ..Default::default()
                },
            );

            let filter = QueryFilter {
                required_flags: NodeFlags::VISIBLE,
            };

            // From visible_child (last visible), next should wrap to root
            let next = next_depth_first_filtered(&tree, visible_child, filter).unwrap();
            assert_eq!(next, root);

            // From root, next visible should be visible_child
            let next = next_depth_first_filtered(&tree, root, filter).unwrap();
            assert_eq!(next, visible_child);
        }

        #[test]
        fn filtered_traversal_respects_liveness() {
            let mut tree = Tree::new();

            let root = tree.insert(
                None,
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );
            let child = tree.insert(
                Some(root),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );

            let filter = QueryFilter {
                required_flags: NodeFlags::VISIBLE,
            };

            // Should work with live nodes
            assert!(next_depth_first_filtered(&tree, root, filter).is_some());

            tree.remove(child);

            // Should return None for stale nodes
            assert!(next_depth_first_filtered(&tree, child, filter).is_none());
            assert!(prev_depth_first_filtered(&tree, child, filter).is_none());
        }

        #[test]
        fn filtered_traversal_wraparound_within_subtree() {
            let mut tree = Tree::new();

            // Build two separate subtrees with mixed visibility
            let root1 = tree.insert(
                None,
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );
            let _child1_hidden = tree.insert(
                Some(root1),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::empty(), // hidden
                    ..Default::default()
                },
            );
            let child1_visible = tree.insert(
                Some(root1),
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );

            let _root2 = tree.insert(
                None,
                LocalNode {
                    local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    flags: NodeFlags::VISIBLE,
                    ..Default::default()
                },
            );

            let filter = QueryFilter {
                required_flags: NodeFlags::VISIBLE,
            };

            // From child1_visible (last visible in subtree1), should wrap to root1 (not cross to subtree2)
            let next = next_depth_first_filtered(&tree, child1_visible, filter).unwrap();
            assert_eq!(next, root1);

            // From root1, next visible should be child1_visible (skipping hidden child)
            let next = next_depth_first_filtered(&tree, root1, filter).unwrap();
            assert_eq!(next, child1_visible);
        }
    }
}
