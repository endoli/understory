// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Core tree implementation: structure, updates, queries.

use alloc::{vec, vec::Vec};
use kurbo::{Affine, Point, Rect, RoundedRect, Shape};
use understory_index::{Backend, IndexGeneric, Key as AabbKey, backends::FlatVec};

use crate::damage::Damage;
use crate::types::{LocalNode, NodeFlags, NodeId};
use crate::util::{rect_to_aabb, transform_rect_bbox};

/// Top-level region tree.
///
/// The type parameter `B` controls which spatial index backend is used. It
/// defaults to a flat-vector backend ([`FlatVec<f64>`]), so most callers can
/// simply use [`Tree`] without specifying `B`. Advanced callers can override
/// `B` to use a grid, an [R-tree][understory_index::backends::RTree], or a
/// [BVH][understory_index::backends::Bvh] backend from `understory_index`.
///
/// Changes to local node geometry (bounds, transform, clip) dirty the tree and
/// are **not** propagated immediately. The changes are batched and applied
/// when [`Tree::commit`] is called, which recomputes world-space data and
/// synchronizes the spatial index.
///
/// ## Example
///
/// ```rust
/// use kurbo::Rect;
/// use understory_box_tree::{LocalNode, Tree};
///
/// // Create a tree and a single root node.
/// let mut tree = Tree::new();
/// let root = tree.insert(
///     None,
///     LocalNode {
///         local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
///         ..LocalNode::default()
///     },
/// );
///
/// // Changes are propagated on commit.
/// tree.commit();
///
/// let world = tree.world_bounds(root).unwrap();
/// assert_eq!(world, Rect::new(0.0, 0.0, 100.0, 100.0));
/// ```
pub struct Tree<B: Backend<f64> = FlatVec<f64>> {
    /// slots
    nodes: Vec<Option<Node>>,
    /// last generation per slot (persists across frees)
    generations: Vec<u32>,
    pub(crate) free_list: Vec<usize>,
    pub(crate) epoch: u64,
    pub(crate) index: IndexGeneric<f64, NodeId, B>,
    needs_commit: bool,
    dirty_roots: Vec<NodeId>,
}

impl<B: Backend<f64> + core::fmt::Debug> core::fmt::Debug for Tree<B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let total = self.nodes.len();
        let alive = self.nodes.iter().filter(|n| n.is_some()).count();
        let free = self.free_list.len();
        f.debug_struct("Tree")
            .field("nodes_total", &total)
            .field("nodes_alive", &alive)
            .field("free_list", &free)
            .field("epoch", &self.epoch)
            .field("index", &self.index)
            .field("needs_commit", &self.needs_commit)
            .finish_non_exhaustive()
    }
}

impl<B> Default for Tree<B>
where
    B: Backend<f64> + Default,
{
    fn default() -> Self {
        Self::with_backend(B::default())
    }
}

/// Results of a hit test.
#[derive(Clone, Debug)]
pub struct Hit {
    /// The matched node.
    pub node: NodeId,
    /// Path from root to node (inclusive).
    pub path: Vec<NodeId>,
}

/// Filters applied during hit testing and rectangle intersection.
///
/// Used by [`Tree::hit_test_point`] and [`Tree::intersect_rect`] to restrict
/// which nodes participate in queries.
#[derive(Clone, Copy, Debug)]
pub struct QueryFilter {
    /// Bitfield of required node flags. Only nodes containing all these flags will be included.
    pub required_flags: NodeFlags,
}

impl Default for QueryFilter {
    fn default() -> Self {
        Self {
            required_flags: NodeFlags::empty(),
        }
    }
}

impl QueryFilter {
    /// Create a new empty filter (includes all nodes).
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter to only visible nodes.
    pub fn visible(mut self) -> Self {
        self.required_flags |= NodeFlags::VISIBLE;
        self
    }

    /// Filter to only pickable nodes.
    pub fn pickable(mut self) -> Self {
        self.required_flags |= NodeFlags::PICKABLE;
        self
    }

    /// Filter to only focusable nodes.
    pub fn focusable(mut self) -> Self {
        self.required_flags |= NodeFlags::FOCUSABLE;
        self
    }

    /// Check if a node's flags satisfy this filter.
    pub fn matches(&self, node_flags: NodeFlags) -> bool {
        node_flags.contains(self.required_flags)
    }
}

#[derive(Clone, Debug, Default)]
struct WorldNode {
    world_transform: Affine,
    world_transform_inverse: Affine,
    world_bounds: Rect, // AABB of transformed (and clipped) local bounds
    world_clip: Option<Rect>,
    depth: u16,
}

#[derive(Clone, Copy, Debug, Default)]
struct Dirty {
    layout: bool,
    transform: bool,
    clip: bool,
    index: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    generation: u32,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    local: LocalNode,
    world: WorldNode,
    dirty: Dirty,
    index_key: Option<AabbKey>,
    indexed_aabb: Option<understory_index::Aabb2D<f64>>,
}

impl Node {
    fn new(generation: u32, local: LocalNode) -> Self {
        Self {
            generation,
            parent: None,
            children: Vec::new(),
            local,
            world: WorldNode::default(),
            dirty: Dirty {
                layout: true,
                transform: true,
                clip: true,
                index: true,
            },
            index_key: None,
            indexed_aabb: None,
        }
    }
}

impl Tree {
    /// Create a new empty tree using the default backend (`FlatVec<f64>`).
    ///
    /// After inserting nodes or mutating local data, call [`Tree::commit`] to
    /// update world-space transforms/bounds and the spatial index before
    /// issuing queries.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            generations: Vec::new(),
            free_list: Vec::new(),
            epoch: 0,
            index: IndexGeneric::new(),
            needs_commit: false,
            dirty_roots: Vec::new(),
        }
    }
}

impl<B: Backend<f64>> Tree<B> {
    /// Create a new tree with a specific backend.
    pub fn with_backend(backend: B) -> Self {
        Self {
            nodes: Vec::new(),
            generations: Vec::new(),
            free_list: Vec::new(),
            epoch: 0,
            index: IndexGeneric::with_backend(backend),
            needs_commit: false,
            dirty_roots: Vec::new(),
        }
    }

    /// Returns `true` if the tree has uncommitted geometry or structural changes.
    ///
    /// When this returns `true`, cached world-space values (such as
    /// [`Tree::world_transform`] and [`Tree::world_bounds`]) and spatial-index-backed query
    /// results are only valid after calling [`Tree::commit`].
    ///
    /// Note: not all updates require a commit. Changes to [`LocalNode`] flags or `z_index` take
    /// effect immediately and do not affect this value.
    #[inline]
    pub fn needs_commit(&self) -> bool {
        self.needs_commit
    }

    #[inline]
    fn debug_assert_committed(&self) {
        debug_assert!(
            !self.needs_commit,
            "Tree queries require calling `Tree::commit()` after geometry/tree-structure mutations"
        );
    }

    #[inline]
    fn mark_dirty(&mut self, id: NodeId) {
        self.needs_commit = true;
        self.dirty_roots.push(id);
    }

    /// Insert a new node as a child of `parent` (or as a root if `None`).
    ///
    /// The returned [`NodeId`] becomes live immediately, but world-space data
    /// (`world_transform`, `world_bounds`) and the spatial index are only
    /// updated on the next call to [`Tree::commit`].
    pub fn insert(&mut self, parent: Option<NodeId>, local: LocalNode) -> NodeId {
        let (idx, generation) = if let Some(idx) = self.free_list.pop() {
            let generation = self.generations[idx].saturating_add(1);
            self.generations[idx] = generation;
            self.nodes[idx] = Some(Node::new(generation, local));
            #[expect(
                clippy::cast_possible_truncation,
                reason = "NodeId uses 32-bit indices by design."
            )]
            (idx as u32, generation)
        } else {
            let generation = 1_u32;
            self.nodes.push(Some(Node::new(generation, local)));
            self.generations.push(generation);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "NodeId uses 32-bit indices by design."
            )]
            ((self.nodes.len() - 1) as u32, generation)
        };
        let id = NodeId::new(idx, generation);
        if let Some(p) = parent {
            self.link_parent(id, p);
        }
        self.mark_dirty(id);
        id
    }

    /// Remove a node (and its subtree) from the tree.
    ///
    /// The node becomes stale immediately, but damage and spatial index updates
    /// are finalized on the next call to [`Tree::commit`].
    pub fn remove(&mut self, id: NodeId) {
        if !self.is_alive(id) {
            return;
        }
        self.needs_commit = true;
        if let Some(parent) = self.node(id).parent {
            self.unlink_parent(id, parent);
        }
        let children = self.node(id).children.clone();
        for child in children {
            self.remove(child);
        }
        if let Some(key) = self.node(id).index_key {
            self.index.remove(key);
        }
        self.nodes[id.idx()] = None;
        self.free_list.push(id.idx());
    }

    /// Reparent `id` under `new_parent`.
    ///
    /// This marks the subtree dirty; world-space transforms/bounds and the
    /// spatial index are updated on the next call to [`Tree::commit`].
    pub fn reparent(&mut self, id: NodeId, new_parent: Option<NodeId>) {
        if !self.is_alive(id) {
            return;
        }
        if let Some(parent) = self.node(id).parent {
            self.unlink_parent(id, parent);
        }
        if let Some(p) = new_parent {
            self.link_parent(id, p);
        }
        self.mark_dirty(id);
        let node = self.node_mut(id);
        node.dirty.transform = true;
        node.dirty.clip = true;
        node.dirty.index = true;
    }

    /// Update local transform.
    ///
    /// This marks the node as dirty for transform and index updates and sets
    /// [`Tree::needs_commit`] to `true`.
    ///
    /// - [`Tree::get_or_compute_world_transform`] and [`Tree::get_or_compute_world_bounds`] can
    ///   compute this node's world data independently of [`Tree::commit`].
    /// - [`Tree::commit`] is what synchronizes world bounds into the spatial index.
    /// - Hit testing and other spatial-index-backed queries rely on that index sync, so they
    ///   require [`Tree::commit`] after geometry changes.
    pub fn set_local_transform(&mut self, id: NodeId, tf: Affine) {
        let changed = match self.node_opt_mut(id) {
            Some(n) if n.local.local_transform != tf => {
                n.local.local_transform = tf;
                n.dirty.transform = true;
                n.dirty.index = true;
                true
            }
            _ => false,
        };
        if changed {
            self.mark_dirty(id);
        }
    }

    /// Update local clip.
    ///
    /// This marks the node as dirty for clip and index updates and sets
    /// [`Tree::needs_commit`] to `true`.
    ///
    /// - [`Tree::get_or_compute_world_transform`] and [`Tree::get_or_compute_world_bounds`] can
    ///   compute this node's world data independently of [`Tree::commit`].
    /// - [`Tree::commit`] is what synchronizes world bounds into the spatial index.
    /// - Hit testing and other spatial-index-backed queries rely on that index sync, so they
    ///   require [`Tree::commit`] after geometry changes.
    pub fn set_local_clip(&mut self, id: NodeId, clip: Option<RoundedRect>) {
        let changed = match self.node_opt_mut(id) {
            Some(n) if n.local.local_clip != clip => {
                n.local.local_clip = clip;
                n.dirty.clip = true;
                n.dirty.index = true;
                true
            }
            _ => false,
        };
        if changed {
            self.mark_dirty(id);
        }
    }

    /// Update z index.
    ///
    /// The change takes effect immediately and does not require a [`Tree::commit`].
    pub fn set_z_index(&mut self, id: NodeId, z: i32) {
        if let Some(n) = self.node_opt_mut(id)
            && n.local.z_index != z
        {
            n.local.z_index = z;
        }
    }

    /// Update local bounds.
    ///
    /// This marks the node as dirty for layout and index updates and sets
    /// [`Tree::needs_commit`] to `true`.
    ///
    /// - [`Tree::get_or_compute_world_transform`] and [`Tree::get_or_compute_world_bounds`] can
    ///   compute this node's world data independently of [`Tree::commit`].
    /// - [`Tree::commit`] is what synchronizes world bounds into the spatial index.
    /// - Hit testing and other spatial-index-backed queries rely on that index sync, so they
    ///   require [`Tree::commit`] after geometry changes.
    pub fn set_local_bounds(&mut self, id: NodeId, bounds: Rect) {
        let changed = match self.node_opt_mut(id) {
            Some(n) if n.local.local_bounds != bounds => {
                n.local.local_bounds = bounds;
                n.dirty.layout = true;
                n.dirty.index = true;
                true
            }
            _ => false,
        };
        if changed {
            self.mark_dirty(id);
        }
    }

    /// Update node flags.
    ///
    /// The change takes effect immediately and does not require a [`Tree::commit`].
    pub fn set_flags(&mut self, id: NodeId, flags: NodeFlags) {
        let Some(n) = self.node_opt_mut(id) else {
            return;
        };
        if n.local.flags == flags {
            return;
        }
        n.local.flags = flags;
    }

    /// Return the world transform for a live node as of the last [`Tree::commit`].
    ///
    /// The returned [`Affine`] maps from the node's local coordinate space into
    /// the tree's root/world space. Returns `None` for stale identifiers.
    pub fn world_transform(&self, id: NodeId) -> Option<Affine> {
        if !self.is_alive(id) {
            return None;
        }
        self.debug_assert_committed();
        self.nodes
            .get(id.idx())
            .and_then(|slot| slot.as_ref())
            .map(|node| node.world.world_transform)
    }

    /// Return the world-space axis-aligned bounding box for a live node.
    ///
    /// This is the loose AABB computed during [`Tree::commit`], after applying
    /// local transforms and any active clips. It fully contains the transformed
    /// bounds but may not be tight, especially under rotation or rounded clips.
    /// This is the same AABB used for spatial indexing and rectangle queries.
    /// Returns `None` for stale identifiers.
    pub fn world_bounds(&self, id: NodeId) -> Option<Rect> {
        if !self.is_alive(id) {
            return None;
        }
        self.debug_assert_committed();
        self.nodes
            .get(id.idx())
            .and_then(|slot| slot.as_ref())
            .map(|node| node.world.world_bounds)
    }

    /// Access a node for debugging; panics if `id` is stale.
    pub(crate) fn node(&self, id: NodeId) -> &Node {
        self.nodes[id.idx()].as_ref().expect("dangling NodeId")
    }

    /// Access a node mutably for debugging; panics if `id` is stale.
    pub(crate) fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.nodes[id.idx()].as_mut().expect("dangling NodeId")
    }

    /// Run the batched update and return coarse damage.
    ///
    /// This recomputes world-space transforms, bounds, and clips for all live
    /// nodes reachable from roots, synchronizes their AABBs into the spatial
    /// index, and returns a [`Damage`] summary capturing added/removed/moved
    /// regions. Call this after mutating any `LocalNode` fields or tree
    /// structure before issuing queries.
    ///
    /// Note: not all updates require a commit. Changes to [`LocalNode`] flags or
    /// `z_index` take effect immediately (they do not affect world-space data or
    /// the spatial index).
    pub fn commit(&mut self) -> Damage {
        if !self.needs_commit {
            return Damage::default();
        }
        let mut damage = Damage::default();
        let mut starts = core::mem::take(&mut self.dirty_roots);
        starts.retain(|id| self.is_alive(*id));
        starts.sort_by_key(|id| (id.1, id.0));
        starts.dedup_by_key(|id| (id.1, id.0));

        // Remove any start node that is already covered by a start ancestor that will traverse
        // into its descendants.
        //
        // We only traverse a subtree when an ancestor transform/clip changed. If an ancestor is
        // dirty only for local layout/index, it will be updated but its children won't be visited.
        let mut top_level = Vec::new();
        for &id in &starts {
            let mut covered = false;
            let mut current = self.node(id).parent;
            while let Some(p) = current {
                if starts
                    .binary_search_by_key(&(p.1, p.0), |x| (x.1, x.0))
                    .is_ok()
                {
                    let ancestor_dirty = self.node(p).dirty;
                    if ancestor_dirty.transform || ancestor_dirty.clip {
                        covered = true;
                        break;
                    }
                }
                current = self.node(p).parent;
            }
            if !covered {
                top_level.push(id);
            }
        }

        for id in top_level {
            let (parent_tf, parent_clip, depth) = if let Some(parent) = self.node(id).parent {
                let p = self.node(parent);
                (
                    p.world.world_transform,
                    p.world.world_clip,
                    p.world.depth.saturating_add(1),
                )
            } else {
                (Affine::IDENTITY, None, 1_u16)
            };
            self.update_world_subtree(id, parent_tf, parent_clip, depth, false, &mut damage);
        }

        let idx_damage = self.index.commit();
        if let Some(u) = idx_damage.union() {
            let r = Rect::new(u.min_x, u.min_y, u.max_x, u.max_y);
            damage.dirty_rects.push(r);
        }

        self.needs_commit = false;
        damage
    }

    /// Hit test a world-space point and, if any node matches, return the
    /// topmost node and its path to root as a [`Hit`].
    ///
    /// - `point` is interpreted in world coordinates.
    /// - Nodes must satisfy the [`QueryFilter`] and contain the point within their
    ///   world-space bounds and clip to be eligible.
    /// - Among candidates, higher `z_index` wins; if `z_index` ties, deeper nodes
    ///   in the tree win; if that also ties, the newer [`NodeId`] wins.
    ///
    /// This tie-break is intentionally deterministic for now. In the future this
    /// may be made configurable (for example via a `TieBreakPolicy`).
    pub fn hit_test_point(&self, point: Point, filter: QueryFilter) -> Option<Hit> {
        self.debug_assert_committed();
        let mut best: Option<(NodeId, i32, u16)> = None;
        self.index.visit_point(point.x, point.y, |_, id| {
            // The spatial index provides a coarse world-AABB candidate set. Everything below is
            // precise filtering in local coordinates (bounds/clips) plus deterministic tie-breaks.
            let Some(node) = self.nodes.get(id.idx()).and_then(|slot| slot.as_ref()) else {
                return;
            };
            if node.generation != id.1 || !filter.matches(node.local.flags) {
                return;
            }

            // Finely test whether `point` is within the node's bounds and the node's own clip.
            let local_point = node.world.world_transform_inverse * point;
            if !node.local.local_bounds.contains(local_point) {
                return;
            }
            if let Some(clip) = node.local.local_clip
                && !clip.contains(local_point)
            {
                return;
            }

            // Walk ancestors towards the root checking their clips for precise hit filtering.
            //
            // This is intentionally only done for candidates that pass the local bounds/clip
            // checks, since ancestor traversal is comparatively expensive.
            let mut current = node.parent;
            while let Some(parent_id) = current {
                let parent = self.node(parent_id);
                debug_assert_eq!(
                    parent.generation, parent_id.1,
                    "parent slot generation mismatch"
                );
                if let Some(clip) = parent.local.local_clip {
                    let parent_local_point = parent.world.world_transform_inverse * point;
                    if !clip.contains(parent_local_point) {
                        return;
                    }
                }
                current = parent.parent;
            }

            let depth = node.world.depth;
            let z = node.local.z_index;
            match best {
                None => best = Some((id, z, depth)),
                Some((id_best, z_best, depth_best)) => {
                    if z > z_best
                        || (z == z_best
                            && (depth > depth_best
                                || (depth == depth_best && id_is_newer(id, id_best))))
                    {
                        best = Some((id, z, depth));
                    }
                }
            }
        });

        best.map(|(node, _, _)| Hit {
            node,
            path: self.path_to_root(node),
        })
    }

    /// Iterate live nodes whose world-space bounds intersect a world-space rectangle.
    ///
    /// Edges of the rectangle and bounding boxes are included in the intersection, meaning that a
    /// rectangle and bounding box that share (part of) an edge are considered to overlap.
    ///
    /// - `rect` is interpreted in world coordinates.
    /// - Nodes must satisfy the [`QueryFilter`] and have a non-empty intersection
    ///   between their world-space bounds and the supplied rectangle to be yielded.
    /// - The returned [`NodeId`]s are in an unspecified order; no z-sorting is applied.
    pub fn intersect_rect<'a>(
        &'a self,
        rect: Rect,
        filter: QueryFilter,
    ) -> impl Iterator<Item = NodeId> + 'a {
        self.debug_assert_committed();
        let q = rect_to_aabb(rect);
        self.index
            .query_rect(q)
            .map(|(_, id)| id)
            .filter(move |id| {
                let Some(node) = self.nodes[id.idx()].as_ref() else {
                    return false;
                };
                filter.matches(node.local.flags)
            })
    }

    /// Iterate live nodes whose world-space bounds contain a world-space point.
    ///
    /// Edges of the bounding boxes are included in the contains-check, having the same semantics
    /// as [`Aabb2D::contains_point`][understory_index::Aabb2D::contains_point], meaning that a
    /// point exactly on the edge of a bounding box is contained by that bounding box.
    ///
    /// - `point` is interpreted in world coordinates.
    /// - Nodes must satisfy the [`QueryFilter`] and contain the given point to be yielded.
    /// - The returned [`NodeId`]s are in an unspecified order; no z-sorting is applied.
    pub fn containing_point<'a>(
        &'a self,
        point: Point,
        filter: QueryFilter,
    ) -> impl Iterator<Item = NodeId> + 'a {
        self.debug_assert_committed();
        self.index
            .query_point(point.x, point.y)
            .map(|(_, id)| id)
            .filter(move |id| {
                let Some(node) = self.nodes[id.idx()].as_ref() else {
                    return false;
                };
                filter.matches(node.local.flags)
            })
    }
}

#[inline]
fn id_is_newer(a: NodeId, b: NodeId) -> bool {
    (a.1 > b.1) || (a.1 == b.1 && a.0 > b.0)
}

impl<B: Backend<f64>> Tree<B> {
    // --- internals ---

    /// Returns true if `id` refers to a live node.
    ///
    /// A `NodeId` is considered live if its slot exists and its generation matches
    /// the current generation stored in that slot.
    /// See [`NodeId`] docs for the generational semantics.
    pub fn is_alive(&self, id: NodeId) -> bool {
        self.nodes
            .get(id.idx())
            .and_then(|n| n.as_ref())
            .map(|n| n.generation == id.1)
            .unwrap_or(false)
    }

    /// Returns the z-index of a node if the identifier is live.
    pub fn z_index(&self, id: NodeId) -> Option<i32> {
        if !self.is_alive(id) {
            return None;
        }
        self.nodes
            .get(id.idx())
            .and_then(|slot| slot.as_ref())
            .map(|node| node.local.z_index)
    }

    /// Returns the parent of a node if live, or `None` for roots or stale ids.
    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        if !self.is_alive(id) {
            return None;
        }
        self.nodes
            .get(id.idx())
            .and_then(|slot| slot.as_ref())
            .and_then(|node| node.parent)
    }

    /// Returns the flags of a node if the identifier is live.
    pub fn flags(&self, id: NodeId) -> Option<NodeFlags> {
        if !self.is_alive(id) {
            return None;
        }
        self.nodes
            .get(id.idx())
            .and_then(|slot| slot.as_ref())
            .map(|node| node.local.flags)
    }

    /// Get the next node in depth-first traversal order.
    ///
    /// Returns `None` if no next node exists or if the current node is stale.
    /// This is a standard tree traversal that does not wrap around.
    pub fn next_depth_first(&self, current: NodeId) -> Option<NodeId> {
        if !self.is_alive(current) {
            return None;
        }

        self.next_in_order(current)
    }

    /// Get the previous node in reverse depth-first traversal order.
    ///
    /// Returns `None` if no previous node exists or if the current node is stale.
    /// This is a standard tree traversal that does not wrap around.
    pub fn prev_depth_first(&self, current: NodeId) -> Option<NodeId> {
        if !self.is_alive(current) {
            return None;
        }

        self.prev_in_order(current)
    }

    /// Get the children of a node, or empty slice if node is stale.
    pub fn children_of(&self, id: NodeId) -> &[NodeId] {
        if !self.is_alive(id) {
            return &[];
        }
        &self.node(id).children
    }

    /// Get the world transform for a live node, computing and caching it if needed.
    ///
    /// This returns cached data when the target path is already up to date,
    /// even if unrelated parts of the tree are dirty. If the node (or an
    /// ancestor that affects its world transform) is dirty, this computes
    /// on-demand by composing ancestor local transforms so it reflects
    /// uncommitted local changes.
    ///
    /// This updates cached world values for the target node but does not touch
    /// the spatial index and does not modify [`Tree::needs_commit`].
    ///
    /// Returns `None` if the node ID is stale.
    pub fn get_or_compute_world_transform(&mut self, id: NodeId) -> Option<Affine> {
        if !self.is_alive(id) {
            return None;
        }
        if !self.needs_world_transform_recompute(id) {
            return Some(self.node(id).world.world_transform);
        }
        self.compute_world_node(id)?;
        Some(self.node(id).world.world_transform)
    }

    /// Get world-space bounds for a node, computing and caching them if needed.
    ///
    /// This returns cached data when the target path is already up to date,
    /// even if unrelated parts of the tree are dirty. If the node (or an
    /// ancestor that affects its world bounds) is dirty, this computes the
    /// world transform by walking up the tree, transforms local bounds, and
    /// applies clips (both local and ancestor) to produce the final
    /// world-space AABB.
    ///
    /// Like [`Tree::get_or_compute_world_transform`], this updates cached world values
    /// for the target node only. It does not touch the spatial index and does
    /// not modify [`Tree::needs_commit`].
    ///
    /// Returns `None` if the node ID is stale or the node has been removed.
    ///
    /// This is useful when you need to know the world bounds immediately after making changes,
    /// without waiting for the next [`Tree::commit`]. For example, when computing layout constraints
    /// or checking bounds after moving a node.
    ///
    /// # Example
    /// ```
    /// use understory_box_tree::{Tree, LocalNode};
    /// use kurbo::{Rect, Affine, Vec2};
    ///
    /// let mut tree = Tree::new();
    /// let root = tree.insert(
    ///     None,
    ///     LocalNode {
    ///         local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
    ///         local_transform: Affine::translate(Vec2::new(50.0, 50.0)),
    ///         ..Default::default()
    ///     },
    /// );
    ///
    /// // Compute bounds without committing
    /// let bounds = tree.get_or_compute_world_bounds(root).unwrap();
    /// assert_eq!(bounds, Rect::new(50.0, 50.0, 150.0, 150.0));
    /// ```
    pub fn get_or_compute_world_bounds(&mut self, id: NodeId) -> Option<Rect> {
        if !self.is_alive(id) {
            return None;
        }
        if !self.needs_world_bounds_recompute(id) {
            return Some(self.node(id).world.world_bounds);
        }
        self.compute_world_node(id)?;
        Some(self.node(id).world.world_bounds)
    }

    fn needs_world_transform_recompute(&self, id: NodeId) -> bool {
        if !self.needs_commit {
            return false;
        }

        let mut current = Some(id);
        let mut is_target = true;
        while let Some(node_id) = current {
            let node = self.node(node_id);
            let needs = if is_target {
                node.dirty.transform
            } else {
                node.dirty.transform || node.dirty.clip
            };
            if needs {
                return true;
            }
            current = node.parent;
            is_target = false;
        }
        false
    }

    fn needs_world_bounds_recompute(&self, id: NodeId) -> bool {
        if !self.needs_commit {
            return false;
        }

        let mut current = Some(id);
        let mut is_target = true;
        while let Some(node_id) = current {
            let node = self.node(node_id);
            let needs = if is_target {
                node.dirty.layout || node.dirty.transform || node.dirty.clip
            } else {
                node.dirty.transform || node.dirty.clip
            };
            if needs {
                return true;
            }
            current = node.parent;
            is_target = false;
        }
        false
    }

    fn compute_world_node(&mut self, id: NodeId) -> Option<()> {
        if !self.is_alive(id) {
            return None;
        }

        // Walk up to the root, then process root -> node.
        let path = self.path_to_root(id);
        let mut parent_tf = Affine::IDENTITY;
        let mut parent_clip: Option<Rect> = None;
        let mut depth = 0_u16;
        let mut world: Option<WorldNode> = None;

        for node_id in path {
            depth = depth.saturating_add(1);
            let computed = {
                let node = self.node(node_id);
                Self::compute_world_from_parent(&node.local, parent_tf, parent_clip, depth)
            };
            parent_tf = computed.world_transform;
            parent_clip = computed.world_clip;
            if node_id == id {
                world = Some(computed);
            }
        }

        let world = world?;
        let node = self.node_mut(id);
        node.world = world;
        // On-demand world recompute makes world-space values current for this node.
        // Keep dirty flags intact so `commit` still performs the required spatial-index sync
        // and descendant propagation when ancestor transform/clip changes are pending.

        Some(())
    }

    fn compute_world_from_parent(
        local: &LocalNode,
        parent_tf: Affine,
        parent_clip: Option<Rect>,
        depth: u16,
    ) -> WorldNode {
        let world_transform = parent_tf * local.local_transform;
        let world_transform_inverse = world_transform.inverse();

        let local_clip = local
            .local_clip
            .map(|rr| transform_rect_bbox(world_transform, rr.rect()));
        let world_clip = match (local_clip, parent_clip) {
            (Some(local), Some(parent)) => Some(local.intersect(parent)),
            (Some(local), None) => Some(local),
            (None, Some(parent)) => Some(parent),
            (None, None) => None,
        };

        let mut world_bounds = transform_rect_bbox(world_transform, local.local_bounds);
        if let Some(clip) = world_clip {
            world_bounds = world_bounds.intersect(clip);
        }

        WorldNode {
            world_transform,
            world_transform_inverse,
            world_bounds,
            world_clip,
            depth,
        }
    }

    fn next_in_order(&self, current: NodeId) -> Option<NodeId> {
        let children = &self.node(current).children;
        if let Some(&first_child) = children.first()
            && self.is_alive(first_child)
        {
            return Some(first_child);
        }

        let mut node = current;
        while let Some(parent) = self.parent_of(node) {
            if let Some(next_sibling) = self.next_sibling(node) {
                return Some(next_sibling);
            }
            node = parent;
        }
        None
    }

    fn prev_in_order(&self, current: NodeId) -> Option<NodeId> {
        if let Some(prev_sibling) = self.prev_sibling(current) {
            return self.last_in_subtree(&[prev_sibling]);
        }

        self.parent_of(current)
    }

    fn next_sibling(&self, node: NodeId) -> Option<NodeId> {
        let parent = self.parent_of(node)?;
        let siblings = &self.node(parent).children;
        let pos = siblings.iter().position(|&id| id == node)?;
        siblings.get(pos + 1).copied()
    }

    fn prev_sibling(&self, node: NodeId) -> Option<NodeId> {
        let parent = self.parent_of(node)?;
        let siblings = &self.node(parent).children;
        let pos = siblings.iter().position(|&id| id == node)?;
        if pos > 0 {
            siblings.get(pos - 1).copied()
        } else {
            None
        }
    }

    fn last_in_subtree(&self, nodes: &[NodeId]) -> Option<NodeId> {
        if let Some(&node) = nodes.first()
            && self.is_alive(node)
        {
            let children = &self.node(node).children;
            if let Some(last_child) = children.last()
                && self.is_alive(*last_child)
            {
                return self.last_in_subtree(&[*last_child]);
            }
            return Some(node);
        }
        None
    }

    fn node_opt_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        let n = self.nodes.get_mut(id.idx())?.as_mut()?;
        if n.generation != id.1 {
            return None;
        }
        Some(n)
    }

    fn link_parent(&mut self, id: NodeId, parent: NodeId) {
        let parent_node = self.node_mut(parent);
        parent_node.children.push(id);
        self.node_mut(id).parent = Some(parent);
    }

    fn unlink_parent(&mut self, id: NodeId, parent: NodeId) {
        let p = self.node_mut(parent);
        p.children.retain(|c| *c != id);
        self.node_mut(id).parent = None;
    }

    fn path_to_root(&self, mut id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        loop {
            out.push(id);
            let parent = self.node(id).parent;
            match parent {
                Some(p) => id = p,
                None => break,
            }
        }
        out.reverse();
        out
    }

    fn update_world_subtree(
        &mut self,
        root_id: NodeId,
        root_tf: Affine,
        root_clip: Option<Rect>,
        root_depth: u16,
        inherited_dirty: bool,
        damage: &mut Damage,
    ) {
        enum IndexOp {
            Update(AabbKey, understory_index::Aabb2D<f64>),
            Insert(understory_index::Aabb2D<f64>),
        }

        // Update world-space data by walking depth-first from `root_id`. We only walk into
        // descendants when an ancestor transform/clip has changed (because that affects
        // descendant world-space state).
        let mut stack = vec![(root_id, root_tf, root_clip, root_depth, inherited_dirty)];

        while let Some((id, current_tf, current_clip, depth, inherited_dirty)) = stack.pop() {
            let mut index_op: Option<IndexOp> = None;
            {
                let node = self.node_mut(id);
                let dirty = node.dirty;
                let subtree_inherited_dirty = inherited_dirty || dirty.transform || dirty.clip;

                // Even if only z/flags changed, we still want to clear the dirty bits, but we can
                // skip recomputing world-space geometry.
                let needs_update_world =
                    inherited_dirty || dirty.layout || dirty.transform || dirty.clip || dirty.index;

                if needs_update_world {
                    let old_world_bounds = node.world.world_bounds;
                    node.world = Self::compute_world_from_parent(
                        &node.local,
                        current_tf,
                        current_clip,
                        depth,
                    );

                    let bounds_changed = old_world_bounds != node.world.world_bounds;
                    if bounds_changed {
                        if old_world_bounds.width() > 0.0 && old_world_bounds.height() > 0.0 {
                            damage.dirty_rects.push(old_world_bounds);
                        }
                        if node.world.world_bounds.width() > 0.0
                            && node.world.world_bounds.height() > 0.0
                        {
                            damage.dirty_rects.push(node.world.world_bounds);
                        }
                    }

                    // Use the last synchronized AABB as the source of truth for index changes.
                    // Cached world values can be updated via get_or_compute_* before commit.
                    let aabb = rect_to_aabb(node.world.world_bounds);
                    if node.index_key.is_none() || node.indexed_aabb != Some(aabb) {
                        index_op = Some(if let Some(key) = node.index_key {
                            IndexOp::Update(key, aabb)
                        } else {
                            IndexOp::Insert(aabb)
                        });
                    }
                }

                node.dirty = Dirty::default();

                // Push all children to the stack if an ancestor change affects them.
                if subtree_inherited_dirty {
                    let world_clip = node.world.world_clip;
                    for &child in node.children.iter().rev() {
                        stack.push((
                            child,
                            node.world.world_transform,
                            world_clip,
                            depth.saturating_add(1),
                            subtree_inherited_dirty,
                        ));
                    }
                }
            }

            if let Some(op) = index_op {
                match op {
                    IndexOp::Update(key, aabb) => {
                        self.index.update(key, aabb);
                        self.node_mut(id).indexed_aabb = Some(aabb);
                    }
                    IndexOp::Insert(aabb) => {
                        let key = self.index.insert(aabb, id);
                        let node = self.node_mut(id);
                        node.index_key = Some(key);
                        node.indexed_aabb = Some(aabb);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use kurbo::Vec2;
    use understory_index::Aabb2D;

    use super::*;

    #[derive(Default)]
    struct BackendCounts {
        inserts: AtomicUsize,
        updates: AtomicUsize,
        removes: AtomicUsize,
        clears: AtomicUsize,
    }

    struct CountingBackend<B> {
        inner: B,
        counts: Arc<BackendCounts>,
    }

    impl<B> CountingBackend<B> {
        fn new(inner: B, counts: Arc<BackendCounts>) -> Self {
            Self { inner, counts }
        }
    }

    impl<B: Backend<f64>> Backend<f64> for CountingBackend<B> {
        fn insert(&mut self, slot: usize, aabb: Aabb2D<f64>) {
            self.counts.inserts.fetch_add(1, Ordering::Relaxed);
            self.inner.insert(slot, aabb);
        }

        fn update(&mut self, slot: usize, aabb: Aabb2D<f64>) {
            self.counts.updates.fetch_add(1, Ordering::Relaxed);
            self.inner.update(slot, aabb);
        }

        fn remove(&mut self, slot: usize) {
            self.counts.removes.fetch_add(1, Ordering::Relaxed);
            self.inner.remove(slot);
        }

        fn clear(&mut self) {
            self.counts.clears.fetch_add(1, Ordering::Relaxed);
            self.inner.clear();
        }

        fn visit_point<F: FnMut(usize)>(&self, x: f64, y: f64, f: F) {
            self.inner.visit_point(x, y, f);
        }

        fn visit_rect<F: FnMut(usize)>(&self, rect: Aabb2D<f64>, f: F) {
            self.inner.visit_rect(rect, f);
        }
    }

    /// Returns whether the two sets of node IDs are equal. The two sets do not need to be ordered.
    ///
    /// # Panics
    ///
    /// This panics if one of the two sets contains duplicates.
    #[must_use]
    fn set_equality(a: &[NodeId], b: &[NodeId]) -> bool {
        for (idx, node) in a.iter().enumerate() {
            if a[0..idx].contains(node) || a[idx + 1..].contains(node) {
                panic!("there are duplicates in set `a`");
            }
        }
        for (idx, node) in b.iter().enumerate() {
            if b[0..idx].contains(node) || b[idx + 1..].contains(node) {
                panic!("there are duplicates in set `b`");
            }
        }
        a.len() == b.len() && b.iter().all(|node| a.contains(node))
    }

    #[test]
    fn commit_noop_does_not_touch_backend() {
        let counts = Arc::new(BackendCounts::default());
        let backend = CountingBackend::new(FlatVec::<f64>::default(), counts.clone());
        let mut tree: Tree<CountingBackend<FlatVec<f64>>> = Tree::with_backend(backend);

        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 5.0, 5.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        let inserts0 = counts.inserts.load(Ordering::Relaxed);
        let updates0 = counts.updates.load(Ordering::Relaxed);
        let removes0 = counts.removes.load(Ordering::Relaxed);
        let clears0 = counts.clears.load(Ordering::Relaxed);

        let _ = tree.commit();

        assert_eq!(counts.inserts.load(Ordering::Relaxed), inserts0);
        assert_eq!(counts.updates.load(Ordering::Relaxed), updates0);
        assert_eq!(counts.removes.load(Ordering::Relaxed), removes0);
        assert_eq!(counts.clears.load(Ordering::Relaxed), clears0);
    }

    #[test]
    fn commit_skips_backend_update_when_clipped_bounds_unchanged() {
        let counts = Arc::new(BackendCounts::default());
        let backend = CountingBackend::new(FlatVec::<f64>::default(), counts.clone());
        let mut tree: Tree<CountingBackend<FlatVec<f64>>> = Tree::with_backend(backend);

        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                local_clip: Some(RoundedRect::from_rect(Rect::new(0.0, 0.0, 10.0, 10.0), 0.0)),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                // Large enough to fully cover the parent's clip even if we nudge it slightly.
                local_bounds: Rect::new(-100.0, -100.0, 1000.0, 1000.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        let updates0 = counts.updates.load(Ordering::Relaxed);
        tree.set_local_transform(child, Affine::translate(Vec2::new(0.25, 0.0)));
        let _ = tree.commit();
        assert_eq!(
            counts.updates.load(Ordering::Relaxed),
            updates0,
            "world bounds are unchanged (fully clipped), so the spatial backend should not be updated"
        );
    }

    #[test]
    fn commit_does_not_drop_dirty_descendant_when_ancestor_layout_only() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let parent = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(parent),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Mutate the ancestor in a way that *does not* require visiting descendants, and also
        // mutate a descendant's transform. A commit must update the descendant even if the
        // ancestor is also dirty.
        tree.set_local_bounds(parent, Rect::new(0.0, 0.0, 200.0, 200.0));
        tree.set_local_transform(child, Affine::translate(Vec2::new(50.0, 0.0)));
        let _ = tree.commit();

        let child_bounds = tree.world_bounds(child).unwrap();
        assert_eq!(child_bounds, Rect::new(50.0, 0.0, 60.0, 10.0));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Tree queries require calling `Tree::commit()`")]
    fn hit_test_without_commit_panics_in_debug() {
        let mut tree = Tree::new();
        tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        let _ = tree.hit_test_point(Point::new(5.0, 5.0), QueryFilter::new());
    }

    #[test]
    fn insert_and_hit_test() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                ..Default::default()
            },
        );
        let _a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(10.0, 10.0, 60.0, 60.0),
                z_index: 0,
                ..Default::default()
            },
        );
        let b = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 120.0, 120.0),
                z_index: 10,
                ..Default::default()
            },
        );
        let _ = tree.commit();

        let hit = tree
            .hit_test_point(
                Point::new(50.0, 50.0),
                QueryFilter::new().visible().pickable(),
            )
            .unwrap();
        assert_eq!(hit.node, b, "topmost by z should win");
        assert_eq!(hit.path.first().copied(), Some(root));
        assert_eq!(hit.path.last().copied(), Some(b));
    }

    #[test]
    fn transform_and_damage() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let n = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();
        tree.set_local_transform(n, Affine::translate(Vec2::new(50.0, 0.0)));
        let dmg = tree.commit();
        assert!(dmg.union_rect().is_some());
    }

    #[test]
    fn noop_commit_returns_default_and_queries_unchanged() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                ..Default::default()
            },
        );
        let top = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 120.0, 120.0),
                z_index: 10,
                ..Default::default()
            },
        );
        let _ = tree.commit();

        let hit_before = tree
            .hit_test_point(Point::new(50.0, 50.0), QueryFilter::new())
            .unwrap();
        assert_eq!(hit_before.node, top);

        let dmg = tree.commit();
        assert!(dmg.dirty_rects.is_empty());
        assert!(dmg.union_rect().is_none());

        let hit_after = tree
            .hit_test_point(Point::new(50.0, 50.0), QueryFilter::new())
            .unwrap();
        assert_eq!(hit_after.node, top);
        assert_eq!(hit_after.path, hit_before.path);
    }

    #[test]
    fn set_z_index_does_not_require_commit() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                ..Default::default()
            },
        );
        let a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 120.0, 120.0),
                z_index: 0,
                ..Default::default()
            },
        );
        let b = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 120.0, 120.0),
                z_index: 10,
                ..Default::default()
            },
        );
        let _ = tree.commit();

        assert_eq!(
            tree.hit_test_point(Point::new(50.0, 50.0), QueryFilter::new())
                .unwrap()
                .node,
            b
        );

        tree.set_z_index(a, 20);
        assert_eq!(
            tree.hit_test_point(Point::new(50.0, 50.0), QueryFilter::new())
                .unwrap()
                .node,
            a
        );
    }

    #[test]
    fn set_flags_does_not_require_commit() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                flags: NodeFlags::VISIBLE,
                ..Default::default()
            },
        );
        let n = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 120.0, 120.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        assert_eq!(
            tree.hit_test_point(
                Point::new(50.0, 50.0),
                QueryFilter::new().visible().pickable()
            )
            .unwrap()
            .node,
            n
        );

        tree.set_flags(n, NodeFlags::VISIBLE);
        assert!(
            tree.hit_test_point(
                Point::new(50.0, 50.0),
                QueryFilter::new().visible().pickable()
            )
            .is_none()
        );
    }

    #[test]
    fn inside_aabb_but_outside_local_bounds() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                local_transform: Affine::rotate(45_f64.to_radians()),
                ..Default::default()
            },
        );
        tree.insert(
            Some(root),
            LocalNode {
                // In world space, this rectangle is rotated by 45 degrees due to the parent's
                // transform, resulting in a larger world-space axis-aligned bounding box.
                local_bounds: Rect::new(-100.0, -100.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Hit testing a world-space point that is inside the axis-aligned bounding box of the
        // rotated local bounds, but outside the actual rotated local bounds, should yield no
        // results.
        let miss = tree.hit_test_point(Point::new(90.0, 90.0), QueryFilter::new());
        assert!(miss.is_none());
    }

    #[test]
    fn child_clip_intersects_with_parent_clip() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                local_clip: Some(RoundedRect::from_rect(
                    Rect::new(0.0, 0.0, 100.0, 100.0),
                    0.0,
                )),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(80.0, 80.0, 180.0, 180.0),
                local_clip: Some(RoundedRect::from_rect(
                    Rect::new(60.0, 60.0, 160.0, 160.0),
                    0.0,
                )),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Effective clip should be the intersection of parent and child clips: (80..100, 80..100).
        let bounds = tree.world_bounds(child).unwrap();
        assert_eq!(bounds, Rect::new(80.0, 80.0, 100.0, 100.0));

        // A point inside the child's local clip but outside the parent's clip must not hit.
        let miss = tree.hit_test_point(Point::new(150.0, 150.0), QueryFilter::new());
        assert!(miss.is_none());
    }

    #[test]
    fn inherits_parent_clip() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                local_clip: Some(RoundedRect::from_rect(
                    Rect::new(0.0, 0.0, 100.0, 100.0),
                    0.0,
                )),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(80.0, 80.0, 180.0, 180.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Child should inherit parent's clip when it has no local clip of its own.
        let bounds = tree.world_bounds(child).unwrap();
        assert_eq!(bounds, Rect::new(80.0, 80.0, 100.0, 100.0));

        // A point outside the parent's clip must not hit the child.
        let miss = tree.hit_test_point(Point::new(150.0, 150.0), QueryFilter::new());
        assert!(miss.is_none());
    }

    #[test]
    fn ancestor_rounded_rect_clip_blocks_hit() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                local_clip: Some(RoundedRect::from_rect(
                    Rect::new(0.0, 0.0, 100.0, 100.0),
                    20.0,
                )),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        let clipped_hits = tree.hit_test_point(Point::new(5.0, 5.0), QueryFilter::new());
        assert!(
            clipped_hits.is_none(),
            "corner outside rounded clip should not hit"
        );

        let hits = tree
            .hit_test_point(Point::new(25.0, 25.0), QueryFilter::new())
            .unwrap();
        assert_eq!(hits.node, child);
    }

    #[test]
    fn liveness_insert_remove_reuse() {
        let mut tree = Tree::new();
        // Insert a root, then a child.
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );

        assert!(tree.is_alive(root));
        assert!(tree.is_alive(a));

        // Remove child; id becomes stale.
        tree.remove(a);
        assert!(!tree.is_alive(a));

        // Insert new child; might reuse slot but generation bumps.
        let b = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        assert!(tree.is_alive(b));
        assert!(!tree.is_alive(a));
        // Sanity: either same slot or different, but if same slot, generation must be greater.
        if a.0 == b.0 {
            assert!(b.1 > a.1, "generation must increase on reuse");
        }
    }

    #[test]
    fn test_rtree_backend() {
        use understory_index::backends::RTreeF64;

        // Use an R-tree backend and verify basic hit-testing still works.
        let mut tree: Tree<RTreeF64<NodeId>> = Tree::with_backend(RTreeF64::<NodeId>::default());
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();
        let hit = tree.hit_test_point(Point::new(50.0, 50.0), QueryFilter::new());
        assert_eq!(hit.map(|h| h.node), Some(root));
    }

    #[test]
    fn test_bvh_backend() {
        use understory_index::backends::BvhF64;

        // Use a BVH backend and verify basic hit-testing still works.
        let mut tree: Tree<BvhF64> = Tree::with_backend(BvhF64::default());
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();
        let hit = tree.hit_test_point(Point::new(50.0, 50.0), QueryFilter::new());
        assert_eq!(hit.map(|h| h.node), Some(root));
    }

    #[test]
    fn test_grid_backend() {
        use understory_index::backends::GridF64;

        // Use a grid backend and verify basic hit-testing still works.
        let mut tree: Tree<GridF64> = Tree::with_backend(GridF64::new(50.0));
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();
        let hit = tree.hit_test_point(Point::new(50.0, 50.0), QueryFilter::new());
        assert_eq!(hit.map(|h| h.node), Some(root));
    }

    #[test]
    fn newer_than_semantics() {
        // Construct synthetic NodeId pairs and verify newer ordering.
        let old = NodeId::new(10, 1);
        let newer_same_slot = NodeId::new(10, 2);
        let same_gen_higher_slot = NodeId::new(11, 2);
        let same_gen_lower_slot = NodeId::new(9, 2);

        // Private helper is in scope within the module.
        assert!(id_is_newer(newer_same_slot, old));
        assert!(id_is_newer(same_gen_higher_slot, newer_same_slot));
        assert!(!id_is_newer(same_gen_lower_slot, newer_same_slot));
    }

    #[test]
    fn hit_equal_z_newer_wins() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                ..Default::default()
            },
        );

        // Two overlapping children at the same z.
        let a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 120.0, 120.0),
                z_index: 5,
                ..Default::default()
            },
        );
        let b = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 120.0, 120.0),
                z_index: 5,
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Sanity: with equal z and equal depth, the newer of (a, b) should win; typically b is newer.
        let hit1 = tree
            .hit_test_point(
                Point::new(60.0, 60.0),
                QueryFilter::new().visible().pickable(),
            )
            .unwrap();
        let expected1 = if id_is_newer(b, a) { b } else { a };
        assert_eq!(hit1.node, expected1);

        // Make a stale by removing it, then insert c reusing a's slot (generation++),
        // still equal z and overlapping; c is strictly newer than b by generation.
        tree.remove(a);
        let c = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 120.0, 120.0),
                z_index: 5,
                ..Default::default()
            },
        );
        let _ = tree.commit();
        assert!(id_is_newer(c, b));

        let hit2 = tree
            .hit_test_point(
                Point::new(60.0, 60.0),
                QueryFilter::new().visible().pickable(),
            )
            .unwrap();
        assert_eq!(hit2.node, c, "newer id should win on equal z and depth");
    }

    #[test]
    fn z_index_accessor_respects_liveness() {
        let mut tree = Tree::new();
        let node = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                z_index: 7,
                ..Default::default()
            },
        );
        assert_eq!(tree.z_index(node), Some(7));
        tree.remove(node);
        assert_eq!(tree.z_index(node), None, "stale ids must return None");
        let new_node = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                z_index: 3,
                ..Default::default()
            },
        );
        assert_eq!(tree.z_index(new_node), Some(3));
        assert!(id_is_newer(new_node, node));
    }

    #[test]
    fn deeper_node_wins_over_ancestor_at_equal_z() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                z_index: 0,
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 160.0, 160.0),
                z_index: 0,
                ..Default::default()
            },
        );
        let grandchild = tree.insert(
            Some(child),
            LocalNode {
                local_bounds: Rect::new(80.0, 80.0, 120.0, 120.0),
                z_index: 0,
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Point inside all three; deepest (grandchild) should win even if NodeId
        // allocation order or reuse would prefer another by id alone.
        let hit = tree
            .hit_test_point(
                Point::new(100.0, 100.0),
                QueryFilter::new().visible().pickable(),
            )
            .unwrap();
        assert_eq!(hit.node, grandchild);
        assert_eq!(hit.path, vec![root, child, grandchild]);
    }

    #[test]
    fn id_tiebreak_only_used_when_depth_and_z_equal() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                z_index: 0,
                ..Default::default()
            },
        );
        // Two overlapping children at the same depth and z.
        let a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 160.0, 160.0),
                z_index: 0,
                ..Default::default()
            },
        );
        let b = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(40.0, 40.0, 160.0, 160.0),
                z_index: 0,
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Both overlap the point; whichever is newer by NodeId wins when depth and z are equal.
        let hit = tree
            .hit_test_point(
                Point::new(100.0, 100.0),
                QueryFilter::new().visible().pickable(),
            )
            .unwrap();
        let expected = if id_is_newer(b, a) { b } else { a };
        assert_eq!(hit.node, expected);
        // Path still includes root then the chosen child.
        assert_eq!(hit.path.first().copied(), Some(root));
        assert_eq!(hit.path.last().copied(), Some(expected));
    }

    #[test]
    fn update_bounds_and_damage_and_hit() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let n = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        let hit_before = tree
            .hit_test_point(
                Point::new(50.0, 50.0),
                QueryFilter::new().visible().pickable(),
            )
            .expect("expected initial hit at root");
        assert_eq!(hit_before.node, root);
        assert_eq!(hit_before.path.first().copied(), Some(root));
        assert_eq!(hit_before.path.last().copied(), Some(root));

        tree.set_local_bounds(n, Rect::new(40.0, 40.0, 60.0, 60.0));
        let dmg = tree.commit();
        assert!(dmg.union_rect().is_some());

        let hit_after = tree
            .hit_test_point(
                Point::new(50.0, 50.0),
                QueryFilter::new().visible().pickable(),
            )
            .expect("expected hit after bounds update");
        assert_eq!(hit_after.node, n);
        assert_eq!(hit_after.path.first().copied(), Some(root));
        assert_eq!(hit_after.path.last().copied(), Some(n));
    }

    #[test]
    fn parent_of_respects_liveness_and_roots() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        assert_eq!(tree.parent_of(child), Some(root));
        assert_eq!(tree.parent_of(root), None);
        tree.remove(child);
        assert_eq!(tree.parent_of(child), None);
    }

    #[test]
    fn query_filter_focusable_only() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
                ..Default::default()
            },
        );
        let focusable_child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(10.0, 10.0, 60.0, 60.0),
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE | NodeFlags::FOCUSABLE,
                ..Default::default()
            },
        );
        let _non_focusable_child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(70.0, 10.0, 120.0, 60.0),
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Test hit_test_point with focusable_only filter
        let hit_focusable = tree.hit_test_point(
            Point::new(30.0, 30.0),
            QueryFilter::new().visible().pickable().focusable(),
        );
        assert_eq!(hit_focusable.unwrap().node, focusable_child);

        let hit_non_focusable = tree.hit_test_point(
            Point::new(90.0, 30.0),
            QueryFilter::new().visible().pickable().focusable(),
        );
        assert!(hit_non_focusable.is_none());

        // Test intersect_rect with focusable_only filter
        let focusable_intersections: Vec<NodeId> = tree
            .intersect_rect(
                Rect::new(0.0, 0.0, 200.0, 200.0),
                QueryFilter::new().visible().pickable().focusable(),
            )
            .collect();
        // only `focusable_child`, and not `root` and `non_focusable_child`
        assert!(set_equality(&focusable_intersections, &[focusable_child]));

        // Test contains_point with focusable_only filter
        let focusable_containing: Vec<NodeId> = tree
            .containing_point(
                Point::new(70., 70.),
                QueryFilter::new().visible().pickable().focusable(),
            )
            .collect();
        // nothing, as the only focusable child is `focusable_child`, and we're testing a point
        // outside it
        assert!(set_equality(&focusable_containing, &[]));
    }

    #[test]
    fn query_filter_pickable_only() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
                ..Default::default()
            },
        );
        let pickable_child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(10.0, 10.0, 60.0, 60.0),
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
                ..Default::default()
            },
        );
        let non_pickable_child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(70.0, 10.0, 120.0, 60.0),
                flags: NodeFlags::VISIBLE,
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Test intersect_rect with pickable_only filter
        let pickable_intersections: Vec<NodeId> = tree
            .intersect_rect(
                Rect::new(0.0, 0.0, 200.0, 200.0),
                QueryFilter::new().visible().pickable(),
            )
            .collect();
        // root + pickable_child
        assert!(set_equality(
            &pickable_intersections,
            &[root, pickable_child]
        ));

        // Test contains_point with pickable_only filter
        let pickable_containing: Vec<NodeId> = tree
            .containing_point(
                Point::new(75.0, 10.0),
                QueryFilter::new().visible().pickable(),
            )
            .collect();
        // root only, because the point is outside `pickable_child`
        assert!(set_equality(&pickable_containing, &[root]));

        // Test intersect_rect without pickable_only filter - should include all visible nodes
        let all_visible_intersections: Vec<NodeId> = tree
            .intersect_rect(
                Rect::new(0.0, 0.0, 200.0, 200.0),
                QueryFilter::new().visible(),
            )
            .collect();
        // all nodes
        assert!(set_equality(
            &all_visible_intersections,
            &[root, pickable_child, non_pickable_child]
        ));

        // Test contains_point without pickable_only filter
        let all_visible_containing: Vec<NodeId> = tree
            .containing_point(Point::new(75.0, 10.0), QueryFilter::new().visible())
            .collect();
        // `root` and `non_pickable_child` (and note the point is exactly on the top edge of
        // `non_pickable_child`), the point is outside `pickable_child`
        assert!(set_equality(
            &all_visible_containing,
            &[root, non_pickable_child]
        ));
    }

    #[test]
    fn world_transform_and_bounds_match_updates() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                local_transform: Affine::translate(Vec2::new(10.0, 20.0)),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                local_transform: Affine::translate(Vec2::new(5.0, 7.0)),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Root transform is just its local transform.
        let root_tf = tree.world_transform(root).expect("root should be live");
        assert_eq!(root_tf, Affine::translate(Vec2::new(10.0, 20.0)));

        // Child transform composes parent and local.
        let child_tf = tree.world_transform(child).expect("child should be live");
        let expected_child_tf =
            Affine::translate(Vec2::new(10.0, 20.0)) * Affine::translate(Vec2::new(5.0, 7.0));
        assert_eq!(child_tf, expected_child_tf);

        // World bounds match the transformed local bounds.
        let child_bounds = tree
            .world_bounds(child)
            .expect("child should have world bounds");
        let expected_bounds =
            transform_rect_bbox(expected_child_tf, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(child_bounds, expected_bounds);
    }

    #[test]
    fn world_transform_and_bounds_respect_liveness() {
        let mut tree = Tree::new();
        let node = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        assert!(tree.world_transform(node).is_some());
        assert!(tree.world_bounds(node).is_some());

        tree.remove(node);

        // Stale ids must not expose transforms or bounds.
        assert!(tree.world_transform(node).is_none());
        assert!(tree.world_bounds(node).is_none());
    }

    #[test]
    fn depth_first_traversal() {
        let mut tree = Tree::new();
        // Build tree: root -> [a -> [c, d], b]
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let b = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let c = tree.insert(
            Some(a),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let d = tree.insert(
            Some(a),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );

        // Depth-first order should be: root -> a -> c -> d -> b
        let next_a = tree.next_depth_first(root).unwrap();
        assert_eq!(next_a, a);

        let next_c = tree.next_depth_first(a).unwrap();
        assert_eq!(next_c, c);

        let next_d = tree.next_depth_first(c).unwrap();
        assert_eq!(next_d, d);

        let next_b = tree.next_depth_first(d).unwrap();
        assert_eq!(next_b, b);

        // End of traversal
        assert!(tree.next_depth_first(b).is_none());
    }

    #[test]
    fn reverse_depth_first_traversal() {
        let mut tree = Tree::new();
        // Build tree: root -> [a -> [c, d], b]
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let b = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let c = tree.insert(
            Some(a),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let d = tree.insert(
            Some(a),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );

        // Reverse depth-first order should be: b -> d -> c -> a -> root
        let prev_d = tree.prev_depth_first(b).unwrap();
        assert_eq!(prev_d, d);

        let prev_c = tree.prev_depth_first(d).unwrap();
        assert_eq!(prev_c, c);

        let prev_a = tree.prev_depth_first(c).unwrap();
        assert_eq!(prev_a, a);

        let prev_root = tree.prev_depth_first(a).unwrap();
        assert_eq!(prev_root, root);

        // Beginning of traversal
        assert!(tree.prev_depth_first(root).is_none());
    }

    #[test]
    fn children_of_accessor() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let b = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );

        let children = tree.children_of(root);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], a);
        assert_eq!(children[1], b);

        assert!(tree.children_of(a).is_empty());
        assert!(tree.children_of(b).is_empty());

        tree.remove(a);
        // Stale ids return empty slice
        assert!(tree.children_of(a).is_empty());
    }

    #[test]
    fn traversal_respects_liveness() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );

        assert!(tree.next_depth_first(root).is_some());
        assert!(tree.prev_depth_first(child).is_some());

        tree.remove(child);

        // Stale ids return None for traversal
        assert!(tree.next_depth_first(child).is_none());
        assert!(tree.prev_depth_first(child).is_none());
    }

    #[test]
    fn depth_changes_during_traversal() {
        let mut tree = Tree::new();
        // Build tree: root -> a -> b -> c
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let b = tree.insert(
            Some(a),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );
        let c = tree.insert(
            Some(b),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );

        // Forward traversal
        let next = tree.next_depth_first(root).unwrap();
        assert_eq!(next, a);

        let next = tree.next_depth_first(a).unwrap();
        assert_eq!(next, b);

        let next = tree.next_depth_first(b).unwrap();
        assert_eq!(next, c);

        // Reverse traversal
        let prev = tree.prev_depth_first(c).unwrap();
        assert_eq!(prev, b);

        let prev = tree.prev_depth_first(b).unwrap();
        assert_eq!(prev, a);

        let prev = tree.prev_depth_first(a).unwrap();
        assert_eq!(prev, root);
    }

    #[test]
    fn get_or_compute_world_transform_without_commit() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                local_transform: Affine::translate(Vec2::new(10.0, 20.0)),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 50.0, 50.0),
                local_transform: Affine::translate(Vec2::new(5.0, 7.0)),
                ..Default::default()
            },
        );

        // Compute world transform without committing
        let world_tf = tree.get_or_compute_world_transform(child).unwrap();
        let expected =
            Affine::translate(Vec2::new(10.0, 20.0)) * Affine::translate(Vec2::new(5.0, 7.0));
        assert_eq!(world_tf, expected);

        // Now commit and verify the cached value matches
        let _ = tree.commit();
        let cached_tf = tree.world_transform(child).unwrap();
        assert_eq!(cached_tf, expected);

        // Modify local transform without committing
        tree.set_local_transform(child, Affine::translate(Vec2::new(15.0, 25.0)));

        // Compute should reflect the new transform
        let new_world_tf = tree.get_or_compute_world_transform(child).unwrap();
        let new_expected =
            Affine::translate(Vec2::new(10.0, 20.0)) * Affine::translate(Vec2::new(15.0, 25.0));
        assert_eq!(new_world_tf, new_expected);

        // Cache is updated by compute_* without touching commit requirements.
        assert_eq!(tree.node(child).world.world_transform, new_expected);
        assert!(tree.node(child).dirty.transform);
        assert!(!tree.node(child).dirty.clip);
        assert!(!tree.node(child).dirty.layout);
        assert!(tree.node(child).dirty.index);
        assert!(tree.needs_commit());
    }

    #[test]
    fn get_or_compute_world_transform_stale_id() {
        let mut tree = Tree::new();
        let node = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            },
        );

        assert!(tree.get_or_compute_world_transform(node).is_some());

        tree.remove(node);

        assert!(tree.get_or_compute_world_transform(node).is_none());
    }

    #[test]
    fn get_or_compute_world_bounds_without_commit() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                local_transform: Affine::translate(Vec2::new(10.0, 20.0)),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 50.0, 50.0),
                local_transform: Affine::translate(Vec2::new(5.0, 7.0)),
                ..Default::default()
            },
        );

        // Compute bounds without committing
        let world_bounds = tree.get_or_compute_world_bounds(child).unwrap();
        let expected = Rect::new(15.0, 27.0, 65.0, 77.0); // child at (5,7) + parent at (10,20)
        assert_eq!(world_bounds, expected);

        // Now commit and verify the cached value matches
        let _ = tree.commit();
        let cached_bounds = tree.world_bounds(child).unwrap();
        assert_eq!(cached_bounds, expected);

        // Modify local transform without committing
        tree.set_local_transform(child, Affine::translate(Vec2::new(15.0, 25.0)));

        let new_world_bounds = tree.get_or_compute_world_bounds(child).unwrap();
        let new_expected = Rect::new(25.0, 45.0, 75.0, 95.0); // child at (15,25) + parent at (10,20)
        assert_eq!(new_world_bounds, new_expected);

        // Cache is updated by compute_* without touching commit requirements.
        assert_eq!(tree.node(child).world.world_bounds, new_expected);
        assert!(tree.node(child).dirty.transform);
        assert!(!tree.node(child).dirty.clip);
        assert!(!tree.node(child).dirty.layout);
        assert!(tree.node(child).dirty.index);
        assert!(tree.needs_commit());
    }

    #[test]
    fn compute_world_does_not_touch_spatial_index_or_commit_flag() {
        let counts = Arc::new(BackendCounts::default());
        let backend = CountingBackend::new(FlatVec::<f64>::default(), counts.clone());
        let mut tree: Tree<CountingBackend<FlatVec<f64>>> = Tree::with_backend(backend);

        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 25.0, 25.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        tree.set_local_transform(child, Affine::translate(Vec2::new(10.0, 5.0)));
        let needs_commit_before = tree.needs_commit();
        let inserts_before = counts.inserts.load(Ordering::Relaxed);
        let updates_before = counts.updates.load(Ordering::Relaxed);
        let removes_before = counts.removes.load(Ordering::Relaxed);
        let clears_before = counts.clears.load(Ordering::Relaxed);

        let _ = tree.get_or_compute_world_transform(child);
        let _ = tree.get_or_compute_world_bounds(child);

        assert_eq!(tree.needs_commit(), needs_commit_before);
        assert_eq!(counts.inserts.load(Ordering::Relaxed), inserts_before);
        assert_eq!(counts.updates.load(Ordering::Relaxed), updates_before);
        assert_eq!(counts.removes.load(Ordering::Relaxed), removes_before);
        assert_eq!(counts.clears.load(Ordering::Relaxed), clears_before);
    }

    #[test]
    fn commit_after_compute_world_bounds_still_updates_spatial_index() {
        let mut tree = Tree::new();
        let child = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        let initial_hit = tree
            .hit_test_point(Point::new(5.0, 5.0), QueryFilter::new())
            .unwrap();
        assert_eq!(initial_hit.node, child);

        tree.set_local_transform(child, Affine::translate(Vec2::new(100.0, 0.0)));

        // This should not prevent commit from synchronizing the index.
        let _ = tree.get_or_compute_world_bounds(child);
        let _ = tree.commit();

        let moved_hit = tree
            .hit_test_point(Point::new(105.0, 5.0), QueryFilter::new())
            .unwrap();
        assert_eq!(moved_hit.node, child);
        assert!(
            tree.hit_test_point(Point::new(5.0, 5.0), QueryFilter::new())
                .is_none(),
            "old location should not be in the index after commit"
        );
    }

    #[test]
    fn get_or_compute_world_bounds_with_clips() {
        use kurbo::RoundedRect;

        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                local_transform: Affine::IDENTITY,
                local_clip: Some(RoundedRect::from_rect(
                    Rect::new(0.0, 0.0, 100.0, 100.0),
                    0.0,
                )),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 150.0, 150.0),
                local_transform: Affine::translate(Vec2::new(50.0, 50.0)),
                ..Default::default()
            },
        );

        // Child bounds should be clipped by parent's clip region
        let world_bounds = tree.get_or_compute_world_bounds(child).unwrap();
        // Child is at (50,50) with size (150,150), so would be (50,50,200,200)
        // But parent clips at (0,0,100,100), so result is (50,50,100,100)
        let expected = Rect::new(50.0, 50.0, 100.0, 100.0);
        assert_eq!(world_bounds, expected);

        // Verify it matches cached value after commit
        let _ = tree.commit();
        let cached_bounds = tree.world_bounds(child).unwrap();
        assert_eq!(cached_bounds, expected);
    }

    #[test]
    fn get_or_compute_world_bounds_with_scale() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                local_transform: Affine::scale(2.0),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 50.0, 50.0),
                local_transform: Affine::translate(Vec2::new(10.0, 10.0)),
                ..Default::default()
            },
        );

        // Child's local bounds (0,0,50,50) at local position (10,10) in parent space
        // Parent has 2x scale, so world bounds should be (20,20,120,120)
        let world_bounds = tree.get_or_compute_world_bounds(child).unwrap();
        let expected = Rect::new(20.0, 20.0, 120.0, 120.0);
        assert_eq!(world_bounds, expected);
    }

    #[test]
    fn get_or_compute_world_bounds_stale_id() {
        let mut tree = Tree::new();
        let node = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );

        assert!(tree.get_or_compute_world_bounds(node).is_some());

        tree.remove(node);

        assert!(tree.get_or_compute_world_bounds(node).is_none());
    }

    #[test]
    fn get_or_compute_world_transform_ignores_unrelated_dirty_nodes() {
        let mut tree = Tree::new();
        let root = tree.insert(None, LocalNode::default());
        let target = tree.insert(Some(root), LocalNode::default());
        let sibling = tree.insert(Some(root), LocalNode::default());
        let _ = tree.commit();

        let sentinel = Affine::translate(Vec2::new(123.0, 456.0));
        tree.node_mut(target).world.world_transform = sentinel;

        tree.set_local_transform(sibling, Affine::translate(Vec2::new(9.0, 8.0)));
        assert!(tree.needs_commit());
        assert!(!tree.node(target).dirty.transform);
        assert!(!tree.node(root).dirty.transform);
        assert!(!tree.node(root).dirty.clip);

        let world_tf = tree.get_or_compute_world_transform(target).unwrap();
        assert_eq!(world_tf, sentinel);
    }

    #[test]
    fn get_or_compute_world_bounds_ignores_unrelated_dirty_nodes() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        let target = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(1.0, 1.0, 4.0, 4.0),
                ..Default::default()
            },
        );
        let sibling = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 5.0, 5.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        let sentinel = Rect::new(50.0, 60.0, 70.0, 80.0);
        tree.node_mut(target).world.world_bounds = sentinel;

        tree.set_local_transform(sibling, Affine::translate(Vec2::new(3.0, 2.0)));
        assert!(tree.needs_commit());
        assert!(!tree.node(target).dirty.layout);
        assert!(!tree.node(target).dirty.transform);
        assert!(!tree.node(target).dirty.clip);
        assert!(!tree.node(root).dirty.transform);
        assert!(!tree.node(root).dirty.clip);

        let world_bounds = tree.get_or_compute_world_bounds(target).unwrap();
        assert_eq!(world_bounds, sentinel);
    }

    #[test]
    fn get_or_compute_dirty_ancestor_still_propagates_descendant_commit() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(1.0, 1.0, 3.0, 3.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        tree.set_local_transform(root, Affine::translate(Vec2::new(10.0, 0.0)));
        let _ = tree.get_or_compute_world_bounds(root);
        let _ = tree.commit();

        assert_eq!(
            tree.world_bounds(child),
            Some(Rect::new(11.0, 1.0, 13.0, 3.0))
        );
    }
}
