// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Box-tree adapter: build focus spaces from an `understory_box_tree::Tree`.
//!
//! This module converts box-tree structure and world-space bounds into
//! [`crate::FocusEntry`] values suitable for focus policies.
//! It treats a subtree rooted at a given [`understory_box_tree::NodeId`] as a
//! focus scope and performs a depth-first traversal to collect candidates.
//!
//! ## Example
//!
//! ```no_run
//! use kurbo::Rect;
//! use understory_box_tree::{LocalNode, NodeFlags, Tree};
//! use understory_focus::adapters::box_tree::build_focus_space_for_scope;
//! use understory_focus::{DefaultPolicy, FocusPolicy, Navigation, WrapMode};
//!
//! // Build a tiny box tree: root with a single focusable child.
//! let mut tree = Tree::new();
//! let root = tree.insert(
//!     None,
//!     LocalNode {
//!         local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
//!         flags: NodeFlags::VISIBLE,
//!         ..LocalNode::default()
//!     },
//! );
//! let button = tree.insert(
//!     Some(root),
//!     LocalNode {
//!         local_bounds: Rect::new(20.0, 20.0, 80.0, 60.0),
//!         flags: NodeFlags::VISIBLE | NodeFlags::FOCUSABLE,
//!         ..LocalNode::default()
//!     },
//! );
//! let _ = tree.commit();
//!
//! // Build a focus space for the subtree rooted at `root`.
//! let mut buf = Vec::new();
//! let space = build_focus_space_for_scope(&tree, root, &(), &mut buf);
//! let policy = DefaultPolicy { wrap: WrapMode::Scope };
//!
//! // In this trivial case, "next" from the only focusable node just wraps.
//! assert_eq!(policy.next(button, Navigation::Next, &space), Some(button));
//! ```

use alloc::vec::Vec;
use understory_box_tree::{NodeFlags, NodeId, Tree};

use crate::{FocusEntry, FocusProps, FocusSpace};

/// Lookup for per-node [`FocusProps`].
///
/// Hosts can implement this trait over a `HashMap`, ECS storage, or any other
/// mapping from node identifiers to focus properties. The adapter will fall
/// back to [`FocusProps::default`] when a node has no entry.
pub trait FocusPropsLookup<K> {
    /// Return focus properties for the given node identifier.
    fn props(&self, id: &K) -> FocusProps;
}

impl<K> FocusPropsLookup<K> for ()
where
    K: Copy,
{
    fn props(&self, _id: &K) -> FocusProps {
        FocusProps::default()
    }
}

/// Build a [`FocusSpace`] for a subtree rooted at `scope_root`.
///
/// - Traverses the box tree in depth-first order starting at `scope_root`.
/// - Includes only nodes that are:
///   - Live in the tree.
///   - Marked focusable via [`NodeFlags::FOCUSABLE`].
///   - Marked visible via [`NodeFlags::VISIBLE`] (to avoid focusing hidden nodes).
///   - Enabled according to [`FocusProps::enabled`].
/// - Uses [`Tree::world_bounds`](Tree::world_bounds) to populate `rect` and
///   computes `scope_depth` relative to `scope_root` (root has depth 0).
///
/// The `out` buffer is cleared and reused to store [`FocusEntry`] values; the
/// returned [`FocusSpace`] borrows from this buffer, so `out` must outlive the
/// focus space.
pub fn build_focus_space_for_scope<'a, B, P>(
    tree: &Tree<B>,
    scope_root: NodeId,
    props_lookup: &P,
    out: &'a mut Vec<FocusEntry<NodeId>>,
) -> FocusSpace<'a, NodeId>
where
    B: understory_index::backend::Backend<f64>,
    P: FocusPropsLookup<NodeId>,
{
    out.clear();

    if !tree.is_alive(scope_root) {
        return FocusSpace { nodes: &[] };
    }

    // Depth-first traversal with an explicit stack to stay within the subtree
    // rooted at `scope_root`. Track depth relative to the scope root.
    let mut stack: Vec<(NodeId, u8)> = Vec::new();
    stack.push((scope_root, 0));

    while let Some((id, depth)) = stack.pop() {
        if !tree.is_alive(id) {
            continue;
        }

        if let (Some(flags), Some(bounds)) = (tree.flags(id), tree.world_bounds(id)) {
            let fp = props_lookup.props(&id);
            let focusable = flags.contains(NodeFlags::FOCUSABLE);
            let visible = flags.contains(NodeFlags::VISIBLE);
            if focusable && visible && fp.enabled {
                out.push(FocusEntry {
                    id,
                    rect: bounds,
                    order: fp.order,
                    group: fp.group,
                    enabled: fp.enabled,
                    scope_depth: depth,
                });
            }
        }

        // Push children in reverse order so the traversal of the stack matches
        // the natural left-to-right order from the tree.
        let children = tree.children_of(id);
        let next_depth = depth.saturating_add(1);
        for &child in children.iter().rev() {
            stack.push((child, next_depth));
        }
    }

    FocusSpace {
        nodes: out.as_slice(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Rect;
    use understory_box_tree::LocalNode;

    #[test]
    fn builds_focus_space_for_focusable_nodes() {
        let mut tree = Tree::new();

        // Root: visible but not focusable.
        let root = tree.insert(
            None,
            LocalNode {
                flags: NodeFlags::VISIBLE,
                ..LocalNode::default()
            },
        );
        // Child A: visible + focusable.
        let a = tree.insert(
            Some(root),
            LocalNode {
                flags: NodeFlags::VISIBLE | NodeFlags::FOCUSABLE,
                ..LocalNode::default()
            },
        );
        // Child B: hidden.
        let _b = tree.insert(
            Some(root),
            LocalNode {
                flags: NodeFlags::empty(),
                ..LocalNode::default()
            },
        );
        let _ = tree.commit();

        let mut buf = Vec::new();
        let space = build_focus_space_for_scope(&tree, root, &(), &mut buf);

        assert_eq!(space.nodes.len(), 1);
        let entry = &space.nodes[0];
        assert_eq!(entry.id, a);
        assert!(entry.enabled);
        assert_eq!(entry.scope_depth, 1);
    }

    #[test]
    fn respects_custom_focus_props() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                flags: NodeFlags::VISIBLE | NodeFlags::FOCUSABLE,
                ..LocalNode::default()
            },
        );
        let disabled_child = tree.insert(
            Some(root),
            LocalNode {
                flags: NodeFlags::VISIBLE | NodeFlags::FOCUSABLE,
                ..LocalNode::default()
            },
        );
        let _ = tree.commit();

        struct MapLookup {
            disabled: NodeId,
        }
        impl FocusPropsLookup<NodeId> for MapLookup {
            fn props(&self, id: &NodeId) -> FocusProps {
                if *id == self.disabled {
                    FocusProps {
                        enabled: false,
                        ..FocusProps::default()
                    }
                } else {
                    FocusProps::default()
                }
            }
        }

        let lookup = MapLookup {
            disabled: disabled_child,
        };
        let mut buf = Vec::new();
        let space = build_focus_space_for_scope(&tree, root, &lookup, &mut buf);

        // Root is focusable and enabled; disabled_child should be skipped.
        assert_eq!(space.nodes.len(), 1);
        assert_eq!(space.nodes[0].id, root);
    }

    #[test]
    fn integration_tree_focus_space_and_policy() {
        use crate::{DefaultPolicy, FocusPolicy, Navigation, WrapMode};

        let mut tree = Tree::new();

        // Root is visible but not focusable; children are visible + focusable.
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                flags: NodeFlags::VISIBLE,
                ..LocalNode::default()
            },
        );
        let left = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(10.0, 10.0, 40.0, 40.0),
                flags: NodeFlags::VISIBLE | NodeFlags::FOCUSABLE,
                ..LocalNode::default()
            },
        );
        let right = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(80.0, 10.0, 110.0, 40.0),
                flags: NodeFlags::VISIBLE | NodeFlags::FOCUSABLE,
                ..LocalNode::default()
            },
        );
        let _ = tree.commit();

        // Build a focus space over the subtree rooted at `root`.
        let mut buf = Vec::new();
        let space = build_focus_space_for_scope(&tree, root, &(), &mut buf);
        assert_eq!(space.nodes.len(), 2);

        let policy = DefaultPolicy {
            wrap: WrapMode::Scope,
        };

        // Linear "next" from left moves to right.
        assert_eq!(policy.next(left, Navigation::Next, &space), Some(right));
        // Directional "Right" from left also prefers the right-hand child.
        assert_eq!(policy.next(left, Navigation::Right, &space), Some(right));
    }
}
