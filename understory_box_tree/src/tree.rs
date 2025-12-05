// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Core tree implementation: structure, updates, queries.

use alloc::vec::Vec;
use kurbo::{Affine, Point, Rect, RoundedRect};
use understory_index::{Aabb2D, Backend, IndexGeneric, Key as AabbKey, backends::FlatVec};

use crate::damage::Damage;
use crate::types::{ClipBehavior, LocalNode, NodeFlags, NodeId};
use crate::util::{rect_to_aabb, transform_rect_bbox};

/// Top-level region tree.
///
/// The type parameter `B` controls which spatial index backend is used. It
/// defaults to a flat-vector backend (`FlatVec<f64>`), so most callers can
/// simply use [`Tree`] without specifying `B`. Advanced callers can override
/// `B` to use an R-tree or BVH backend from `understory_index`.
///
/// Changes to local node data (bounds, transform, clip, flags, z) do **not**
/// take effect immediately. They are batched and applied when [`Tree::commit`]
/// is called, which recomputes world-space data and synchronizes the spatial
/// index.
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
/// // Changes only take effect after commit.
/// tree.commit();
///
/// let world = tree.world_bounds(root).unwrap();
/// assert_eq!(world, Rect::new(0.0, 0.0, 100.0, 100.0));
/// ```
pub struct Tree<B: Backend<f64> = FlatVec<f64>> {
    nodes: Vec<Option<Node>>, // slots
    generations: Vec<u32>,    // last generation per slot (persists across frees)
    pub(crate) free_list: Vec<usize>,
    pub(crate) epoch: u64,
    pub(crate) index: IndexGeneric<f64, NodeId, B>,
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
    world_bounds: Rect, // AABB of transformed (and clipped) local bounds
    world_clip: Option<Rect>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Dirty {
    layout: bool,
    transform: bool,
    clip: bool,
    z: bool,
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
                z: true,
                index: true,
            },
            index_key: None,
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
        }
    }

    fn mark_subtree_dirty(&mut self, id: NodeId, flags: Dirty) {
        if !self.is_alive(id) {
            return;
        }
        let children = {
            let n = self.node_mut(id);
            n.dirty.layout |= flags.layout;
            n.dirty.transform |= flags.transform;
            n.dirty.clip |= flags.clip;
            n.dirty.z |= flags.z;
            n.dirty.index |= flags.index;
            n.children.clone()
        };
        for c in children {
            self.mark_subtree_dirty(c, flags);
        }
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
            #[allow(
                clippy::cast_possible_truncation,
                reason = "NodeId uses 32-bit indices by design."
            )]
            (idx as u32, generation)
        } else {
            let generation = 1_u32;
            self.nodes.push(Some(Node::new(generation, local)));
            self.generations.push(generation);
            #[allow(
                clippy::cast_possible_truncation,
                reason = "NodeId uses 32-bit indices by design."
            )]
            ((self.nodes.len() - 1) as u32, generation)
        };
        let id = NodeId::new(idx, generation);
        if let Some(p) = parent {
            self.link_parent(id, p);
        }
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
        self.mark_subtree_dirty(
            id,
            Dirty {
                layout: true,
                transform: true,
                clip: true,
                z: true,
                index: true,
            },
        );
    }

    /// Update local transform.
    pub fn set_local_transform(&mut self, id: NodeId, tf: Affine) {
        if let Some(n) = self.node_opt_mut(id) {
            n.local.local_transform = tf;
            n.dirty.transform = true;
            n.dirty.index = true;
        }
    }

    /// Update local clip. Application is controlled by [`ClipBehavior`].
    pub fn set_local_clip(&mut self, id: NodeId, clip: Option<RoundedRect>) {
        if let Some(n) = self.node_opt_mut(id) {
            n.local.local_clip = clip;
            n.dirty.clip = true;
            n.dirty.index = true;
        }
    }

    /// Update how local and ancestor clips are composed for this node.
    pub fn set_clip_behavior(&mut self, id: NodeId, behavior: ClipBehavior) {
        if let Some(n) = self.node_opt_mut(id) {
            n.local.clip_behavior = behavior;
            n.dirty.clip = true;
            n.dirty.index = true;
        }
    }

    /// Update z index.
    pub fn set_z_index(&mut self, id: NodeId, z: i32) {
        if let Some(n) = self.node_opt_mut(id) {
            n.local.z_index = z;
            n.dirty.z = true;
        }
    }

    /// Update local bounds.
    pub fn set_local_bounds(&mut self, id: NodeId, bounds: Rect) {
        if let Some(n) = self.node_opt_mut(id) {
            n.local.local_bounds = bounds;
            n.dirty.layout = true;
            n.dirty.index = true;
        }
    }

    /// Update node flags.
    pub fn set_flags(&mut self, id: NodeId, flags: NodeFlags) {
        if let Some(n) = self.node_opt_mut(id) {
            n.local.flags = flags;
            n.dirty.index = true;
        }
    }

    /// Return the world transform for a live node as of the last [`Tree::commit`].
    ///
    /// The returned [`Affine`] maps from the node's local coordinate space into
    /// the tree's root/world space. Returns `None` for stale identifiers.
    pub fn world_transform(&self, id: NodeId) -> Option<Affine> {
        if !self.is_alive(id) {
            return None;
        }
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
    pub fn commit(&mut self) -> Damage {
        let mut damage = Damage::default();
        let roots: Vec<NodeId> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| match n {
                Some(n) if n.parent.is_none() =>
                {
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "NodeId uses 32-bit indices by design."
                    )]
                    Some(NodeId::new(i as u32, n.generation))
                }
                _ => None,
            })
            .collect();

        for root in roots {
            self.update_world_recursive(root, Affine::IDENTITY, None, &mut damage);
        }

        let idx_damage = self.index.commit();
        if let Some(u) = idx_damage.union() {
            let r = Rect::new(u.min_x, u.min_y, u.max_x, u.max_y);
            damage.dirty_rects.push(r);
        }

        damage
    }

    /// Hit test a world-space point and, if any node matches, return the
    /// topmost node and its path to root as a [`Hit`].
    ///
    /// - The point `pt` is interpreted in world coordinates, after all transforms.
    /// - Nodes must satisfy the [`QueryFilter`] and contain the point within their
    ///   world-space bounds and clip to be eligible.
    /// - Among candidates, higher `z_index` wins; if `z_index` ties, deeper nodes
    ///   in the tree win; if that also ties, the newer [`NodeId`] wins.
    ///
    /// This tie-break is intentionally deterministic for now. In the future this
    /// may be made configurable (for example via a `TieBreakPolicy`).
    pub fn hit_test_point(&self, pt: Point, filter: QueryFilter) -> Option<Hit> {
        let candidates: Vec<NodeId> = self
            .index
            .query_point(pt.x, pt.y)
            .map(|(_, id)| id)
            .collect();
        let mut best: Option<(NodeId, i32, usize)> = None;
        for id in candidates {
            let Some(node) = self.nodes[id.idx()].as_ref() else {
                continue;
            };
            if !filter.matches(node.local.flags) {
                continue;
            }
            if node.local.clip_behavior != ClipBehavior::None
                && let Some(clip) = node.local.local_clip
            {
                let world_pt = node.world.world_transform.inverse() * pt;
                if !clip.rect().contains(world_pt) {
                    continue;
                }
            }
            let depth = self.depth(id);
            match best {
                None => best = Some((id, node.local.z_index, depth)),
                Some((best_id, z_best, depth_best)) => {
                    let z = node.local.z_index;
                    if z > z_best
                        || (z == z_best
                            && (depth > depth_best
                                || (depth == depth_best && id_is_newer(id, best_id))))
                    {
                        best = Some((id, z, depth));
                    }
                }
            }
        }
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
    /// as [`Aabb2D::contains_point`], meaning that a point exactly on the edge of a bounding box
    /// is contained by that bounding box.
    ///
    /// - `point` is interpreted in world coordinates.
    /// - Nodes must satisfy the [`QueryFilter`] and contain the given point to be yielded.
    /// - The returned [`NodeId`]s are in an unspecified order; no z-sorting is applied.
    pub fn containing_point<'a>(
        &'a self,
        point: Point,
        filter: QueryFilter,
    ) -> impl Iterator<Item = NodeId> + 'a {
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

    /// Return the depth of a node in the tree (1-based), or 0 if the id is stale.
    ///
    /// Roots have depth 1, direct children of roots have depth 2, and so on.
    fn depth(&self, mut id: NodeId) -> usize {
        if !self.is_alive(id) {
            return 0;
        }
        let mut d = 0;
        loop {
            d += 1;
            match self.node(id).parent {
                Some(p) => id = p,
                None => break,
            }
        }
        d
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

    fn update_world_recursive(
        &mut self,
        id: NodeId,
        parent_tf: Affine,
        parent_clip: Option<Rect>,
        damage: &mut Damage,
    ) {
        enum IndexOp {
            Update(AabbKey, Aabb2D<f64>),
            Insert(Aabb2D<f64>),
        }
        let (old_bounds, child_ids, (_local, world), index_op) = {
            let node = self.node_mut(id);
            let old = node.world.world_bounds;
            node.world.world_transform = parent_tf * node.local.local_transform;
            let mut world_bounds =
                transform_rect_bbox(node.world.world_transform, node.local.local_bounds);
            let local_clip = node
                .local
                .local_clip
                .map(|rr| transform_rect_bbox(node.world.world_transform, rr.rect()));
            let world_clip = match node.local.clip_behavior {
                ClipBehavior::None => None,
                ClipBehavior::PreferLocal => local_clip.or(parent_clip),
                ClipBehavior::Inherit => match (local_clip, parent_clip) {
                    (Some(local), Some(parent)) => Some(local.intersect(parent)),
                    (Some(local), None) => Some(local),
                    (None, Some(parent)) => Some(parent),
                    (None, None) => None,
                },
            };
            if let Some(c) = world_clip {
                world_bounds = world_bounds.intersect(c);
            }
            node.world.world_bounds = world_bounds;
            node.world.world_clip = world_clip;
            let aabb = rect_to_aabb(world_bounds);
            let op = if let Some(key) = node.index_key {
                IndexOp::Update(key, aabb)
            } else {
                IndexOp::Insert(aabb)
            };
            let child_ids = node.children.clone();
            (old, child_ids, (node.local.clone(), node.world.clone()), op)
        };

        match index_op {
            IndexOp::Update(key, aabb) => self.index.update(key, aabb),
            IndexOp::Insert(aabb) => {
                let key = self.index.insert(aabb, id);
                self.node_mut(id).index_key = Some(key);
            }
        }

        if old_bounds != world.world_bounds {
            if old_bounds.width() > 0.0 && old_bounds.height() > 0.0 {
                damage.dirty_rects.push(old_bounds);
            }
            if world.world_bounds.width() > 0.0 && world.world_bounds.height() > 0.0 {
                damage.dirty_rects.push(world.world_bounds);
            }
        }

        for child in child_ids {
            self.update_world_recursive(child, world.world_transform, world.world_clip, damage);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use core::f64::consts::FRAC_PI_4;
    use kurbo::Vec2;

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
    fn inherited_child_clip_intersects_with_parent_clip() {
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
                clip_behavior: ClipBehavior::Inherit,
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
    fn prefer_local_without_local_clip_inherits_parent_clip() {
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
                clip_behavior: ClipBehavior::PreferLocal,
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
    fn clip_behavior_prefer_local_allows_overflow() {
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
                    Rect::new(80.0, 80.0, 180.0, 180.0),
                    0.0,
                )),
                clip_behavior: ClipBehavior::PreferLocal, // ignore parent clip when local clip is present
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Child bounds should remain unclipped and extend beyond parent's clip.
        let bounds = tree.world_bounds(child).unwrap();
        assert_eq!(bounds, Rect::new(80.0, 80.0, 180.0, 180.0));

        // Hit test at a point outside parent's clip but inside child should succeed.
        let hit = tree.hit_test_point(Point::new(150.0, 150.0), QueryFilter::new());
        assert_eq!(hit.map(|h| h.node), Some(child));
    }

    #[test]
    fn clip_behavior_none_disables_clipping() {
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
                    Rect::new(80.0, 80.0, 90.0, 90.0),
                    0.0,
                )),
                clip_behavior: ClipBehavior::None,
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Bounds should not be clipped by either local or parent clip.
        let bounds = tree.world_bounds(child).unwrap();
        assert_eq!(bounds, Rect::new(80.0, 80.0, 180.0, 180.0));

        // Hit test at a point outside both clips but inside local bounds should still hit.
        let hit = tree.hit_test_point(Point::new(150.0, 150.0), QueryFilter::new());
        assert_eq!(hit.map(|h| h.node), Some(child));
    }

    #[test]
    fn rotated_bbox_expands() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
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
        let _nb = tree.node(n).world.world_bounds;
        let _expected =
            transform_rect_bbox(Affine::rotate(FRAC_PI_4), Rect::new(0.0, 0.0, 10.0, 10.0));
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
}
