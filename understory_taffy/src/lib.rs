// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Taffy ↔ box tree adapter.
//!
//! This crate provides a thin bridge between the [`taffy`] layout engine and
//! [`understory_box_tree::Tree`]. It is intentionally small and opinionated:
//! Taffy owns layout (sizes and positions), while the box tree owns geometry
//! and spatial queries (hit testing, visibility).
//!
//! In this workspace, `taffy` is defined in the workspace dependencies with the `taffy_tree`
//! feature enabled, and the examples (and this crate's dev-dependencies) enable the `flexbox`
//! feature so that `Style` participates in layout. If you depend on this crate from another
//! project, make sure that *your* `taffy` dependency enables at least one layout algorithm (for
//! example `flexbox` or `grid`), or use Taffy's default feature set, so that `Style` participates
//! in layout.
//!
//! ## Status
//!
//! This crate is **experimental**. The core concepts (`TaffyBoxMap` as a non-owning adapter and
//! the `TaffyBoxTree` convenience wrapper) are expected to remain, but function names and error
//! types may be refined based on early adopter feedback.
//!
//! ## Design
//!
//! The adapter keeps a mapping from Taffy nodes to box-tree nodes and offers
//! helpers to synchronize layout results into `LocalNode::local_bounds`.
//! It does *not* create Taffy nodes or run layout itself; callers are
//! expected to:
//!
//! - Build and mutate the Taffy tree as usual.
//! - Call `compute_layout` on their root.
//! - Use [`TaffyBoxTree::attach_node`] / [`TaffyBoxTree::detach_node`] to
//!   mirror the subset of nodes that should participate in spatial queries.
//! - Call [`TaffyBoxTree::sync_layout`] (and optionally
//!   [`TaffyBoxTree::sync_layout_and_commit`]) to push layout results into
//!   the box tree.
//!
//! This keeps responsibilities clear and lets you mix Taffy-managed nodes
//! with box-tree-only nodes (for overlays, popups, debug visuals, etc.).

#![deny(unsafe_code)]

use hashbrown::HashMap;

use kurbo::{Affine, Rect};
use taffy::{Layout, NodeId as TaffyNode, TaffyError, TaffyTree};
use understory_box_tree::{Damage, LocalNode, NodeId, Tree};
use understory_index::{Backend, FlatVec};

/// Adapter that synchronizes Taffy layout results into an Understory box tree.
///
/// # Examples
///
/// Basic usage: mirror a Taffy node into the box tree and query its bounds:
///
/// ```rust
/// use taffy::prelude::{AvailableSpace, Dimension, Size, Style, TaffyTree};
/// use understory_box_tree::{LocalNode, NodeFlags, QueryFilter};
/// use understory_taffy::{layout_to_rect, TaffyBoxTree};
/// use kurbo::{Point, Rect};
///
/// // Build a trivial Taffy tree with an explicit size on the root.
/// let mut taffy: TaffyTree<()> = TaffyTree::new();
/// let mut style = Style::DEFAULT;
/// style.size.width = Dimension::length(100.0);
/// style.size.height = Dimension::length(50.0);
/// let root = taffy.new_leaf(style).unwrap();
///
/// // Attach the root to the box tree.
/// let mut adapter: TaffyBoxTree = TaffyBoxTree::new();
/// let root_box = adapter.attach_node(
///     root,
///     None,
///     LocalNode {
///         flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
///         ..LocalNode::default()
///     },
/// );
///
/// // Run layout in Taffy.
/// taffy
///     .compute_layout(
///         root,
///         Size {
///             width: AvailableSpace::Definite(100.0),
///             height: AvailableSpace::Definite(50.0),
///         },
///     )
///     .unwrap();
///
/// // Sync into the box tree and commit once.
/// let damage = adapter.sync_layout_and_commit(&taffy).unwrap();
/// assert!(damage.union_rect().is_some());
///
/// // Query world bounds and hit-test via the box tree.
/// let tree = &adapter.tree;
/// let bounds = tree.world_bounds(root_box).unwrap();
/// // Bounds should match the layout reported by Taffy.
/// let layout = taffy.layout(root).unwrap();
/// let expected = layout_to_rect(layout);
/// assert_eq!(bounds, expected);
///
/// // Hit-test inside the root's bounds should resolve to the attached node.
/// let hit = tree
///     .hit_test_point(
///         Point::new(10.0, 10.0),
///         QueryFilter::new().visible().pickable(),
///     )
///     .unwrap();
/// assert_eq!(hit.node, root_box);
/// ```
///
/// Mixing Taffy-managed nodes with box-tree-only nodes:
///
/// ```rust
/// use taffy::prelude::{AvailableSpace, Dimension, Size, Style, TaffyTree};
/// use understory_box_tree::{LocalNode, NodeFlags};
/// use understory_taffy::{layout_to_rect, TaffyBoxTree};
/// use kurbo::Rect;
///
/// let mut taffy: TaffyTree<()> = TaffyTree::new();
/// let mut style = Style::DEFAULT;
/// style.size.width = Dimension::length(100.0);
/// style.size.height = Dimension::length(50.0);
/// let root = taffy.new_leaf(style).unwrap();
///
/// let mut adapter: TaffyBoxTree = TaffyBoxTree::new();
/// let root_box = adapter.attach_node(
///     root,
///     None,
///     LocalNode {
///         flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
///         ..LocalNode::default()
///     },
/// );
///
/// // Extra node that lives only in the box tree (e.g. overlay).
/// let overlay = adapter.tree.insert(
///     Some(root_box),
///     LocalNode {
///         local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
///         flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
///         ..LocalNode::default()
///     },
/// );
///
/// taffy
///     .compute_layout(
///         root,
///         Size {
///             width: AvailableSpace::Definite(100.0),
///             height: AvailableSpace::Definite(50.0),
///         },
///     )
///     .unwrap();
///
/// // Sync only Taffy-managed nodes; overlay is untouched.
/// adapter.sync_layout(&taffy).unwrap();
/// let damage = adapter.tree.commit();
/// assert!(damage.union_rect().is_some());
///
/// let root_bounds = adapter.tree.world_bounds(root_box).unwrap();
/// let overlay_bounds = adapter.tree.world_bounds(overlay).unwrap();
/// let layout = taffy.layout(root).unwrap();
/// let expected = layout_to_rect(layout);
/// assert_eq!(root_bounds, expected);
/// assert_eq!(overlay_bounds, Rect::new(0.0, 0.0, 10.0, 10.0));
/// ```
/// Non-owning mapping from Taffy nodes to box-tree nodes.
///
/// This type does not own a [`Tree`] or a [`TaffyTree`]; it only tracks how `taffy::NodeId`s map
/// to [`NodeId`]s in an existing box tree and provides helpers to keep `LocalNode::local_bounds`
/// in sync with Taffy layout results.
#[derive(Debug, Default)]
pub struct TaffyBoxMap {
    map: HashMap<TaffyNode, NodeId>,
}

impl TaffyBoxMap {
    /// Create an empty mapping.
    ///
    /// This does not allocate upfront; the internal map grows as nodes are attached.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a Taffy node to a new box-tree node.
    ///
    /// - `taffy_node` must remain valid in the caller's Taffy tree.
    /// - `parent` is an optional box-tree parent; use `None` for roots.
    /// - `local_template` provides initial flags/clip/z; its `local_bounds`
    ///   field will be overwritten on the next layout sync.
    ///
    /// Returns the created box-tree [`NodeId`].
    pub fn attach_node<B>(
        &mut self,
        tree: &mut Tree<B>,
        taffy_node: TaffyNode,
        parent: Option<NodeId>,
        mut local_template: LocalNode,
    ) -> NodeId
    where
        B: Backend<f64>,
    {
        // Start with empty bounds; they are filled in on sync.
        local_template.local_bounds = Rect::ZERO;
        let id = tree.insert(parent, local_template);
        self.map.insert(taffy_node, id);
        id
    }

    /// Detach a Taffy node and remove its box-tree node, if present.
    ///
    /// It is safe to call this even if the node was never attached.
    pub fn detach_node<B>(&mut self, tree: &mut Tree<B>, taffy_node: TaffyNode)
    where
        B: Backend<f64>,
    {
        if let Some(id) = self.map.remove(&taffy_node) {
            // TODO: consider propagating a dedicated adapter error if `remove` were to fail,
            // or if we later add invariants around liveness or external bookkeeping.
            tree.remove(id);
        }
    }

    /// Look up the box-tree node corresponding to a Taffy node.
    #[must_use]
    pub fn box_node(&self, taffy_node: TaffyNode) -> Option<NodeId> {
        self.map.get(&taffy_node).copied()
    }

    /// Look up the Taffy node corresponding to a box-tree node.
    #[must_use]
    pub fn taffy_node(&self, box_node: NodeId) -> Option<TaffyNode> {
        self.map
            .iter()
            .find_map(|(tn, id)| if *id == box_node { Some(*tn) } else { None })
    }

    /// Synchronize layout results for all attached nodes into the box tree.
    ///
    /// - Assumes the caller has already run `compute_layout` on the Taffy tree for any nodes
    ///   referenced in this mapping.
    /// - For each attached node, reads its `Layout` from `taffy` and writes a size-only
    ///   `local_bounds` plus a translation `local_transform` into the box tree. Taffy
    ///   locations are parent-relative; box-tree transforms are cumulative.
    /// - Does *not* call [`Tree::commit`]; callers should commit once after
    ///   all geometry updates are complete.
    ///
    /// For more control (for example, to compute additional visual transforms that depend on
    /// layout), use [`Self::sync_layout_with`] instead.
    pub fn sync_layout<NodeContext, B>(
        &self,
        taffy: &TaffyTree<NodeContext>,
        tree: &mut Tree<B>,
    ) -> Result<(), TaffyError>
    where
        B: Backend<f64>,
    {
        self.sync_layout_with(taffy, tree, |_, box_node, layout, tree| {
            // Taffy positions are parent-relative; represent them as a local translation
            // and keep `local_bounds` anchored at the origin with just the size.
            let rect = layout_size_rect(layout);
            tree.set_local_bounds(box_node, rect);

            let dx = f64::from(layout.location.x);
            let dy = f64::from(layout.location.y);
            let tf = Affine::translate((dx, dy));
            tree.set_local_transform(box_node, tf);
        })
    }

    /// Advanced: synchronize layout using a custom mapping function.
    ///
    /// This variant lets callers compute additional per-node state (for example, visual transforms
    /// that depend on resolved layout sizes or percentages) while still iterating over the mapping
    /// once. The closure is invoked for every attached node with:
    ///
    /// - The Taffy node ID.
    /// - The corresponding box-tree node ID.
    /// - A reference to the node's `Layout` as reported by Taffy.
    /// - A mutable reference to the box tree.
    pub fn sync_layout_with<NodeContext, B, F>(
        &self,
        taffy: &TaffyTree<NodeContext>,
        tree: &mut Tree<B>,
        mut apply: F,
    ) -> Result<(), TaffyError>
    where
        B: Backend<f64>,
        F: FnMut(TaffyNode, NodeId, &Layout, &mut Tree<B>),
    {
        for (taffy_node, box_node) in &self.map {
            let layout = taffy.layout(*taffy_node)?;
            apply(*taffy_node, *box_node, layout, tree);
        }
        Ok(())
    }

    /// Convenience: sync layout and then commit, returning the resulting damage.
    ///
    /// This is a helper for the common case where all geometry updates (from Taffy and any other
    /// sources) flow through this adapter. Callers that combine multiple geometry sources should
    /// prefer [`sync_layout`] and perform a single [`Tree::commit`] themselves instead.
    pub fn sync_layout_and_commit<NodeContext, B>(
        &self,
        taffy: &TaffyTree<NodeContext>,
        tree: &mut Tree<B>,
    ) -> Result<Damage, TaffyError>
    where
        B: Backend<f64>,
    {
        self.sync_layout(taffy, tree)?;
        Ok(tree.commit())
    }
}

/// Owning convenience wrapper that bundles a box tree with a [`TaffyBoxMap`].
#[derive(Debug)]
pub struct TaffyBoxTree<B: Backend<f64> = FlatVec<f64>> {
    /// The underlying box tree used for spatial indexing and queries.
    pub tree: Tree<B>,
    /// Mapping from Taffy nodes into the box tree.
    pub map: TaffyBoxMap,
}

impl<B> Default for TaffyBoxTree<B>
where
    B: Backend<f64> + Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<B> TaffyBoxTree<B>
where
    B: Backend<f64> + Default,
{
    /// Create a new adapter with an empty box tree and mapping.
    ///
    /// The Taffy tree remains owned by the caller; this type owns the box tree
    /// and the mapping from Taffy nodes into it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tree: Tree::with_backend(B::default()),
            map: TaffyBoxMap::new(),
        }
    }
}

impl<B> TaffyBoxTree<B>
where
    B: Backend<f64>,
{
    /// Access the underlying box tree.
    #[must_use]
    pub fn tree(&self) -> &Tree<B> {
        &self.tree
    }

    /// Mutably access the underlying box tree.
    #[must_use]
    pub fn tree_mut(&mut self) -> &mut Tree<B> {
        &mut self.tree
    }

    /// Attach a Taffy node to a new box-tree node.
    pub fn attach_node(
        &mut self,
        taffy_node: TaffyNode,
        parent: Option<NodeId>,
        local_template: LocalNode,
    ) -> NodeId {
        self.map
            .attach_node(&mut self.tree, taffy_node, parent, local_template)
    }

    /// Detach a Taffy node and remove its box-tree node, if present.
    pub fn detach_node(&mut self, taffy_node: TaffyNode) {
        self.map.detach_node(&mut self.tree, taffy_node);
    }

    /// Look up the box-tree node corresponding to a Taffy node.
    #[must_use]
    pub fn box_node(&self, taffy_node: TaffyNode) -> Option<NodeId> {
        self.map.box_node(taffy_node)
    }

    /// Look up the Taffy node corresponding to a box-tree node.
    #[must_use]
    pub fn taffy_node(&self, box_node: NodeId) -> Option<TaffyNode> {
        self.map.taffy_node(box_node)
    }

    /// Synchronize layout results for all attached nodes into the box tree.
    ///
    /// This forwards to [`TaffyBoxMap::sync_layout`]. It assumes that you have already called
    /// `compute_layout` on the relevant Taffy nodes for the current frame.
    pub fn sync_layout<NodeContext>(
        &mut self,
        taffy: &TaffyTree<NodeContext>,
    ) -> Result<(), TaffyError> {
        self.map.sync_layout(taffy, &mut self.tree)
    }

    /// Convenience: sync layout and then commit, returning the resulting damage.
    pub fn sync_layout_and_commit<NodeContext>(
        &mut self,
        taffy: &TaffyTree<NodeContext>,
    ) -> Result<Damage, TaffyError> {
        self.map.sync_layout_and_commit(taffy, &mut self.tree)
    }
}

/// Convert a Taffy layout into a local `Rect` suitable for `LocalNode::local_bounds`.
///
/// ```rust
/// use taffy::prelude::Layout;
/// use understory_taffy::layout_to_rect;
/// use kurbo::Rect;
///
/// let mut layout = Layout::new();
/// layout.location.x = 5.0;
/// layout.location.y = 10.0;
/// layout.size.width = 20.0;
/// layout.size.height = 30.0;
///
/// let rect = layout_to_rect(&layout);
/// assert_eq!(rect, Rect::new(5.0, 10.0, 25.0, 40.0));
/// ```
#[must_use]
pub fn layout_to_rect(layout: &Layout) -> Rect {
    let x0 = layout.location.x;
    let y0 = layout.location.y;
    let x1 = x0 + layout.size.width;
    let y1 = y0 + layout.size.height;
    Rect::new(f64::from(x0), f64::from(y0), f64::from(x1), f64::from(y1))
}

/// Convert a Taffy layout into a size-only `Rect` anchored at the origin.
///
/// This is useful when representing layout as `local_bounds` plus a separate transform:
/// the rect carries only width/height, and the layout location becomes a translation.
fn layout_size_rect(layout: &Layout) -> Rect {
    let w = f64::from(layout.size.width);
    let h = f64::from(layout.size.height);
    Rect::new(0.0, 0.0, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use taffy::prelude::{AvailableSpace, Dimension, Size, Style};
    use understory_box_tree::{NodeFlags, QueryFilter};
    use understory_index::RTreeF64;

    #[test]
    fn attach_sync_and_detach_with_flatvec_backend() {
        let mut taffy: TaffyTree<()> = TaffyTree::new();
        let mut style = Style::DEFAULT;
        style.size.width = Dimension::length(100.0);
        style.size.height = Dimension::length(50.0);
        let root = taffy.new_leaf(style).unwrap();

        let mut tree: Tree<FlatVec<f64>> = Tree::with_backend(FlatVec::default());
        let mut map = TaffyBoxMap::new();

        let root_box = map.attach_node(
            &mut tree,
            root,
            None,
            LocalNode {
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
                ..LocalNode::default()
            },
        );

        taffy
            .compute_layout(
                root,
                Size {
                    width: AvailableSpace::Definite(100.0),
                    height: AvailableSpace::Definite(50.0),
                },
            )
            .unwrap();

        let damage = map.sync_layout_and_commit(&taffy, &mut tree).unwrap();
        assert!(damage.union_rect().is_some());

        let bounds = tree.world_bounds(root_box).unwrap();
        let layout = taffy.layout(root).unwrap();
        let expected = layout_to_rect(layout);
        assert_eq!(bounds, expected);

        // Detach and ensure the node is removed from the tree.
        map.detach_node(&mut tree, root);
        assert!(tree.world_bounds(root_box).is_none());
    }

    #[test]
    fn sync_layout_does_not_touch_unmapped_nodes() {
        let mut taffy: TaffyTree<()> = TaffyTree::new();
        let mut style = Style::DEFAULT;
        style.size.width = Dimension::length(80.0);
        style.size.height = Dimension::length(40.0);
        let root = taffy.new_leaf(style).unwrap();

        let mut tree: Tree<FlatVec<f64>> = Tree::with_backend(FlatVec::default());
        let mut map = TaffyBoxMap::new();

        let root_box = map.attach_node(
            &mut tree,
            root,
            None,
            LocalNode {
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
                ..LocalNode::default()
            },
        );

        // Insert an unmapped overlay node directly into the box tree.
        let overlay = tree.insert(
            Some(root_box),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
                ..LocalNode::default()
            },
        );

        taffy
            .compute_layout(
                root,
                Size {
                    width: AvailableSpace::Definite(80.0),
                    height: AvailableSpace::Definite(40.0),
                },
            )
            .unwrap();

        let _ = map.sync_layout_and_commit(&taffy, &mut tree).unwrap();

        let overlay_after = tree.world_bounds(overlay).unwrap();
        assert_eq!(
            overlay_after,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            "unmapped nodes must retain their own bounds"
        );
    }

    #[test]
    fn attach_and_sync_with_rtree_backend() {
        let mut taffy: TaffyTree<()> = TaffyTree::new();
        let mut style = Style::DEFAULT;
        style.size.width = Dimension::length(50.0);
        style.size.height = Dimension::length(20.0);
        let root = taffy.new_leaf(style).unwrap();

        let mut tree: Tree<RTreeF64<NodeId>> = Tree::with_backend(RTreeF64::<NodeId>::default());
        let mut map = TaffyBoxMap::new();

        let root_box = map.attach_node(
            &mut tree,
            root,
            None,
            LocalNode {
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
                ..LocalNode::default()
            },
        );

        taffy
            .compute_layout(
                root,
                Size {
                    width: AvailableSpace::Definite(50.0),
                    height: AvailableSpace::Definite(20.0),
                },
            )
            .unwrap();

        let damage = map.sync_layout_and_commit(&taffy, &mut tree).unwrap();
        assert!(damage.union_rect().is_some());

        // Verify hit-testing with the R-tree backend still works on synced bounds.
        let bounds = tree.world_bounds(root_box).unwrap();
        let layout = taffy.layout(root).unwrap();
        let expected = layout_to_rect(layout);
        assert_eq!(bounds, expected);

        let hit = tree
            .hit_test_point(
                kurbo::Point::new(bounds.x0 + 1.0, bounds.y0 + 1.0),
                QueryFilter::new().visible().pickable(),
            )
            .unwrap();
        assert_eq!(hit.node, root_box);
    }
}
