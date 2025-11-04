// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_box_tree --heading-base-level=0

//! Understory Box Tree: a Kurbo-native, spatially indexed box tree.
//!
//! Understory Box Tree is a reusable building block for UIs, canvas and vector editors, and CAD viewers.
//!
//! - Represents a hierarchy of regions with local transforms, clips, z-order, and flags.
//! - Provides hit testing and rectangle intersection queries over world-space AABBs.
//! - Supports batched updates with a [`Tree::commit`] step that yields coarse damage regions.
//!
//! It aims for a stable, minimal API and leaves room to evolve internals (for example a pluggable spatial index) without churn at call sites.
//!
//! ## Where this fits: three-tree model
//!
//! We’re standardizing on a simple separation of concerns for UI stacks.
//! - Widget tree: interaction/state.
//! - Box tree: geometry/spatial indexing (this crate).
//! - Render tree: display list (future crate).
//!
//! The box tree computes world-space AABBs from local bounds, transforms, and clips, and synchronizes them into a spatial index for fast hit testing and visibility queries.
//! This decouples scene structure from the spatial acceleration and makes debugging and incremental updates tractable.
//!
//! ## Not a layout engine
//!
//! This crate does not perform layout (measurement or arrangement) or apply layout policies such as flex, grid, or stack.
//! Upstream code is expected to compute positions and sizes using whatever layout system you choose and then update this tree with the resulting world-space boxes, transforms, optional clips, and z-order.
//! Think of this as a scene and spatial index, not a layout system.
//!
//! ## Integration with Understory Index
//!
//! This crate uses [`understory_index`] for spatial queries. You can choose the backend and scalar to
//! fit your workload (flat vector, uniform grid for `f32`/`f64`/`i64`, R-tree or BVH). Float inputs are
//! assumed to be finite (no NaNs). AABBs are conservative for non-axis transforms and rounded clips.
//!
//! See [`understory_index::Index`], [`understory_index::GridF32`]/[`understory_index::GridF64`]/[`understory_index::GridI64`],
//! [`understory_index::RTreeF32`]/[`understory_index::RTreeF64`]/[`understory_index::RTreeI64`], and
//! [`understory_index::BVHF32`]/[`understory_index::BVHF64`]/[`understory_index::BVHI64`] for details.
//!
//! ## API overview
//!
//! - [`Tree`]: container managing nodes and the spatial index synchronization.
//! - [`LocalNode`]: per-node local data (bounds, transform, optional clip, z, flags).
//!   See [`LocalNode::flags`] for visibility/picking controls.
//! - [`NodeFlags`]: visibility and picking controls.
//! - [`NodeId`]: generational handle of a node.
//! - [`QueryFilter`]: restricts hit/intersect results (visible/pickable).
//!   See [`NodeFlags::VISIBLE`] and [`NodeFlags::PICKABLE`].
//!
//! Key operations:
//! - [`Tree::insert`](Tree::insert) → [`NodeId`]
//! - [`Tree::set_local_transform`](Tree::set_local_transform) / [`Tree::set_local_clip`](Tree::set_local_clip)
//! - [`Tree::commit`](Tree::commit) → damage summary; updates world data and the spatial index.
//! - [`Tree::hit_test_point`](Tree::hit_test_point) and [`Tree::intersect_rect`](Tree::intersect_rect).
//!
//! ## Damage and debugging notes
//!
//! - [`Tree::commit`] batches adds/updates/removals and produces coarse damage (added/removed AABBs and
//!   old/new pairs for moved nodes). This is enough to bound a paint traversal in most UIs.
//! - World AABBs are conservative under rotation/shear and rounded-rect clips are approximated by
//!   their axis-aligned bounds for acceleration; precise hit-filtering is applied where cheap.
//!
//! ## Examples
//!
//! - `examples/basic_box_tree.rs`: builds a trivial tree, commits, and runs a couple of queries.
//! - `examples/visible_list.rs`: demonstrates using `intersect_rect` to compute a visible set,
//!   a building block for virtualization.
//!
//! ### Minimal usage
//!
//! ```
//! use understory_box_tree::{Tree, LocalNode, QueryFilter};
//! use kurbo::{Rect, Affine, Vec2, Point};
//!
//! // Build a tiny tree.
//! let mut tree = Tree::new();
//!
//! let root = tree.insert(
//!     None,
//!     LocalNode { local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0), ..Default::default() },
//! );
//!
//! let child = tree.insert(
//!     Some(root),
//!     LocalNode { local_bounds: Rect::new(10.0, 10.0, 60.0, 60.0), ..Default::default() },
//! );
//!
//! // Synchronize and compute damage.
//! let _ = tree.commit();
//!
//! // Move and hit-test.
//! tree.set_local_transform(child, Affine::translate(Vec2::new(10.0, 0.0)));
//! let _ = tree.commit();
//!
//! let filter = QueryFilter { visible_only: true, pickable_only: true };
//! let hit = tree.hit_test_point(Point::new(25.0, 25.0), filter).unwrap();
//! assert_eq!(hit.node, child);
//! ```
//!
//! ### Visible set using a viewport rectangle
//!
//! ```
//! use understory_box_tree::{Tree, LocalNode, QueryFilter};
//! use kurbo::Rect;
//!
//! let mut tree = Tree::new();
//!
//! let root = tree.insert(
//!     None,
//!     LocalNode { local_bounds: Rect::new(0.0, 0.0, 1000.0, 1000.0), ..Default::default() },
//! );
//!
//! // Insert rows.
//! for i in 0..10u32 {
//!     let y = i as f64 * 50.0;
//!     let _ = tree.insert(
//!         Some(root),
//!         LocalNode {
//!             local_bounds: Rect::new(0.0, y, 200.0, y + 40.0),
//!             z_index: i as i32,
//!             ..Default::default()
//!         },
//!     );
//! }
//!
//! let _ = tree.commit();
//!
//! // Compute visible set.
//! let filter = QueryFilter { visible_only: true, pickable_only: true };
//! let viewport = Rect::new(0.0, 120.0, 200.0, 220.0);
//! let visible: Vec<_> = tree.intersect_rect(viewport, filter).collect();
//! assert!(visible.len() >= 2);
//! ```
//!
//! This crate is `no_std` and uses `alloc`.
//!
//! # Example
//!
//! ```rust
//! use understory_box_tree::{Tree, LocalNode, QueryFilter};
//! use kurbo::{Rect, Affine, Vec2, Point};
//!
//! // Build a small tree.
//! let mut tree = Tree::new();
//!
//! let root = tree.insert(
//!     None,
//!     LocalNode { local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0), ..Default::default() },
//! );
//!
//! let a = tree.insert(
//!     Some(root),
//!     LocalNode { local_bounds: Rect::new(10.0, 10.0, 60.0, 60.0), z_index: 0, ..Default::default() },
//! );
//!
//! let b = tree.insert(
//!     Some(root),
//!     LocalNode { local_bounds: Rect::new(40.0, 40.0, 120.0, 120.0), z_index: 10, ..Default::default() },
//! );
//!
//! let _damage0 = tree.commit();
//!
//! // Move node A to the right and compute damage.
//! tree.set_local_transform(a, Affine::translate(Vec2::new(20.0, 0.0)));
//! let damage = tree.commit();
//! assert!(damage.union_rect().is_some());
//!
//! // Hit-test prefers the higher z-index (node B).
//! let filter = QueryFilter { visible_only: true, pickable_only: true };
//! let hit = tree.hit_test_point(Point::new(50.0, 50.0), filter).unwrap();
//! assert_eq!(hit.node, b);
//! ```
//!
//! See the `basic` example in this crate for a runnable version with printed output.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use bitflags::bitflags;
use kurbo::{Affine, Point, Rect, RoundedRect};
use understory_index::{Aabb2D, Index as AabbIndex, Key as AabbKey};

/// Identifier for a node in the tree (generational).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct NodeId(u32, u32);

impl NodeId {
    fn new(idx: u32, generation: u32) -> Self {
        Self(idx, generation)
    }

    fn idx(self) -> usize {
        self.0 as usize
    }
}

bitflags! {
    /// Node flags controlling visibility and picking.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct NodeFlags: u8 {
        /// Node is visible (participates in rendering and intersection queries).
        const VISIBLE  = 0b0000_0001;
        /// Node is pickable (participates in hit testing).
        const PICKABLE = 0b0000_0010;
    }
}

impl Default for NodeFlags {
    fn default() -> Self {
        Self::VISIBLE | Self::PICKABLE
    }
}

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}

/// Local geometry for a node.
#[derive(Clone, Debug)]
pub struct LocalNode {
    /// Local (untransformed) bounds. For non-axis-aligned content, use a conservative AABB.
    pub local_bounds: Rect,
    /// Local transform relative to parent space.
    pub local_transform: Affine,
    /// Optional local clip (rounded-rect). AABB is used for spatial indexing; precise hit test is best-effort.
    pub local_clip: Option<RoundedRect>,
    /// Z-order within parent stacking context. Higher is drawn on top.
    pub z_index: i32,
    /// Visibility and picking flags.
    ///
    /// See [`NodeFlags`] for available bits and how they interact with [`QueryFilter`].
    pub flags: NodeFlags,
}

impl Default for LocalNode {
    fn default() -> Self {
        Self {
            local_bounds: Rect::ZERO,
            local_transform: Affine::IDENTITY,
            local_clip: None,
            z_index: 0,
            flags: NodeFlags::default(),
        }
    }
}

/// Aggregate world-space data cached per node.
#[derive(Clone, Debug, Default)]
struct WorldNode {
    world_transform: Affine,
    world_bounds: Rect, // AABB of transformed (and clipped) local bounds
    world_clip: Option<Rect>,
}

/// Per-node dirty state.
#[derive(Clone, Copy, Debug, Default)]
struct Dirty {
    layout: bool,
    transform: bool,
    clip: bool,
    z: bool,
    index: bool,
}

#[derive(Clone, Debug)]
struct Node {
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

/// A batched set of changes derived from [`Tree::commit`].
#[derive(Clone, Debug, Default)]
pub struct Damage {
    /// World-space rectangles that should be repainted.
    pub dirty_rects: Vec<Rect>,
}

impl Damage {
    /// Returns the union of all damage rects.
    pub fn union_rect(&self) -> Option<Rect> {
        let mut it = self.dirty_rects.iter().copied();
        let first = it.next()?;
        Some(it.fold(first, |acc, r| acc.union(r)))
    }
}

/// Top-level region tree.
pub struct Tree {
    nodes: Vec<Option<Node>>, // generational slots
    free_list: Vec<usize>,
    epoch: u64,
    // Naive spatial index: we will scan nodes; can be swapped for an R-tree later.
    index: AabbIndex<f64, NodeId>,
}

impl core::fmt::Debug for Tree {
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
/// Used by [`Tree::hit_test_point`] and [`Tree::intersect_rect`].
#[derive(Clone, Copy, Debug, Default)]
pub struct QueryFilter {
    /// If true, only consider nodes marked [`NodeFlags::VISIBLE`].
    pub visible_only: bool,
    /// If true, only consider nodes marked [`NodeFlags::PICKABLE`] (hit-test).
    pub pickable_only: bool,
}

impl Tree {
    /// Create a new empty tree.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            free_list: Vec::new(),
            epoch: 0,
            index: AabbIndex::default(),
        }
    }

    /// Default creates an empty tree.
    pub fn default_tree() -> Self {
        Self::new()
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
    pub fn insert(&mut self, parent: Option<NodeId>, local: LocalNode) -> NodeId {
        let (idx, generation) = if let Some(idx) = self.free_list.pop() {
            let generation = self.nodes[idx].as_ref().map(|n| n.generation).unwrap_or(0) + 1;
            self.nodes[idx] = Some(Node::new(generation, local));
            #[allow(
                clippy::cast_possible_truncation,
                reason = "NodeId uses 32-bit indices by design."
            )]
            (idx as u32, generation)
        } else {
            let generation = 1_u32;
            self.nodes.push(Some(Node::new(generation, local)));
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
    pub fn remove(&mut self, id: NodeId) {
        if !self.is_alive(id) {
            return;
        }
        // Detach from parent first
        if let Some(parent) = self.node(id).parent {
            self.unlink_parent(id, parent);
        }
        // Depth-first remove children
        let children = self.node(id).children.clone();
        for child in children {
            self.remove(child);
        }
        // Remove from spatial index if present
        if let Some(key) = self.node(id).index_key {
            self.index.remove(key);
        }
        // Free slot
        self.nodes[id.idx()] = None;
        self.free_list.push(id.idx());
    }

    /// Reparent `id` under `new_parent`.
    pub fn reparent(&mut self, id: NodeId, new_parent: Option<NodeId>) {
        if !self.is_alive(id) {
            return;
        }
        // Unlink from current
        if let Some(parent) = self.node(id).parent {
            self.unlink_parent(id, parent);
        }
        if let Some(p) = new_parent {
            self.link_parent(id, p);
        }
        self.mark_subtree_dirty(
            id,
            Dirty {
                transform: true,
                layout: false,
                clip: false,
                z: false,
                index: true,
            },
        );
    }

    /// Update local bounds.
    pub fn set_local_bounds(&mut self, id: NodeId, bounds: Rect) {
        if let Some(node) = self.node_opt_mut(id) {
            node.local.local_bounds = bounds;
            node.dirty.layout = true;
            node.dirty.index = true;
        }
    }

    /// Update local transform.
    pub fn set_local_transform(&mut self, id: NodeId, transform: Affine) {
        if let Some(node) = self.node_opt_mut(id) {
            node.local.local_transform = transform;
            node.dirty.transform = true;
            node.dirty.index = true;
        }
    }

    /// Update clip.
    pub fn set_local_clip(&mut self, id: NodeId, clip: Option<RoundedRect>) {
        if let Some(node) = self.node_opt_mut(id) {
            node.local.local_clip = clip;
            node.dirty.clip = true;
            node.dirty.index = true;
        }
    }

    /// Update z index.
    pub fn set_z_index(&mut self, id: NodeId, z: i32) {
        if let Some(node) = self.node_opt_mut(id) {
            node.local.z_index = z;
            node.dirty.z = true;
            node.dirty.index = true;
        }
    }

    /// Update flags.
    pub fn set_flags(&mut self, id: NodeId, flags: NodeFlags) {
        if let Some(node) = self.node_opt_mut(id) {
            node.local.flags = flags;
            // flags do not affect world transforms/bounds; no geometry dirty
        }
    }

    /// Commit pending changes, updating world-space caches and returning coarse damage.
    pub fn commit(&mut self) -> Damage {
        self.epoch = self.epoch.wrapping_add(1);
        let mut damage = Damage::default();

        // For simplicity, recompute world data for dirty nodes by walking from each root.
        // Later: maintain a list of dirty roots and do incremental updates.
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

        // Synchronize spatial index backend and optionally merge its coarse damage.
        let idx_damage = self.index.commit();
        if let Some(u) = idx_damage.union() {
            let r = Rect::new(u.min_x, u.min_y, u.max_x, u.max_y);
            damage.dirty_rects.push(r);
        }

        damage
    }

    /// Hit test a world-space point. Returns the topmost matching node.
    /// Returns the topmost node at a point.
    ///
    /// Honors [`QueryFilter`].
    pub fn hit_test_point(&self, pt: Point, filter: QueryFilter) -> Option<Hit> {
        // Use spatial index to gather candidates, then filter and choose topmost by z.
        let candidates: Vec<NodeId> = self
            .index
            .query_point(pt.x, pt.y)
            .map(|(_, id)| id)
            .collect();
        let mut best: Option<(NodeId, i32)> = None;
        for id in candidates {
            let Some(node) = self.nodes[id.idx()].as_ref() else {
                continue;
            };
            if filter.visible_only && !node.local.flags.contains(NodeFlags::VISIBLE) {
                continue;
            }
            if filter.pickable_only && !node.local.flags.contains(NodeFlags::PICKABLE) {
                continue;
            }
            if let Some(clip) = node.local.local_clip {
                let world_pt = node.world.world_transform.inverse() * pt;
                if !clip.rect().contains(world_pt) {
                    continue;
                }
            }
            match best {
                None => best = Some((id, node.local.z_index)),
                Some((_, z_best)) if node.local.z_index >= z_best => {
                    best = Some((id, node.local.z_index));
                }
                _ => {}
            }
        }
        best.map(|(node, _)| Hit {
            node,
            path: self.path_to_root(node),
        })
    }

    /// Iterate nodes intersecting a world-space rect.
    /// Returns nodes whose world AABBs intersect `rect`.
    ///
    /// Honors [`QueryFilter`].
    pub fn intersect_rect<'a>(
        &'a self,
        rect: Rect,
        filter: QueryFilter,
    ) -> impl Iterator<Item = NodeId> + 'a {
        let q = rect_to_aabb(rect);
        let ids: Vec<NodeId> = self.index.query_rect(q).map(|(_, id)| id).collect();
        ids.into_iter().filter(move |id| {
            let Some(node) = self.nodes[id.idx()].as_ref() else {
                return false;
            };
            if filter.visible_only && !node.local.flags.contains(NodeFlags::VISIBLE) {
                return false;
            }
            true
        })
    }

    // --- internals ---

    fn is_alive(&self, id: NodeId) -> bool {
        self.nodes
            .get(id.idx())
            .and_then(|n| n.as_ref())
            .map(|n| n.generation == id.1)
            .unwrap_or(false)
    }

    fn node(&self, id: NodeId) -> &Node {
        self.nodes[id.idx()].as_ref().expect("dangling NodeId")
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.nodes[id.idx()].as_mut().expect("dangling NodeId")
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
            // update transforms
            node.world.world_transform = parent_tf * node.local.local_transform;
            let mut world_bounds =
                transform_rect_bbox(node.world.world_transform, node.local.local_bounds);
            // clip in world space (conservative AABB)
            let world_clip = node
                .local
                .local_clip
                .map(|rr| transform_rect_bbox(node.world.world_transform, rr.rect()))
                .or(parent_clip);
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

        // apply index update/insert now that the node borrow is released
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

/// Transform an axis-aligned `Rect` by an `Affine` and return a conservative
/// axis-aligned bounding box in world space.
fn transform_rect_bbox(affine: Affine, rect: Rect) -> Rect {
    let [a, b, c, d, e, f] = affine.as_coeffs();
    let min_x = (a * rect.x0).min(a * rect.x1) + (c * rect.y0).min(c * rect.y1);
    let max_x = (a * rect.x0).max(a * rect.x1) + (c * rect.y0).max(c * rect.y1);
    let min_y = (b * rect.x0).min(b * rect.x1) + (d * rect.y0).min(d * rect.y1);
    let max_y = (b * rect.x0).max(b * rect.x1) + (d * rect.y0).max(d * rect.y1);
    Rect::new(min_x + e, min_y + f, max_x + e, max_y + f)
}

fn rect_to_aabb(r: Rect) -> Aabb2D<f64> {
    Aabb2D::new(r.x0, r.y0, r.x1, r.y1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f64::consts::FRAC_PI_4;
    use kurbo::Vec2;

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
                QueryFilter {
                    visible_only: true,
                    pickable_only: true,
                },
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
        // rotate 45 degrees around origin
        let rot = Affine::rotate(FRAC_PI_4);
        tree.set_local_transform(n, rot);
        let _ = tree.commit();
        let w = tree.node(n).world.world_bounds.width();
        assert!(w > 10.0, "bbox should expand when rotated");
    }

    #[test]
    fn deep_reparent_updates_index() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        let a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                local_transform: Affine::translate(Vec2::new(100.0, 0.0)),
                ..Default::default()
            },
        );
        let b = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                local_transform: Affine::translate(Vec2::new(0.0, 100.0)),
                ..Default::default()
            },
        );
        let leaf = tree.insert(
            Some(a),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        let filter = QueryFilter {
            visible_only: true,
            pickable_only: false,
        };
        // Initially under A's transform at x ~ [100..110]
        let hits_a: Vec<_> = tree
            .intersect_rect(Rect::new(100.0, 0.0, 110.0, 10.0), filter)
            .collect();
        assert!(hits_a.contains(&leaf));

        // Reparent leaf under B; now should be at y ~ [100..110]
        tree.reparent(leaf, Some(b));
        let _ = tree.commit();
        let hits_b: Vec<_> = tree
            .intersect_rect(Rect::new(0.0, 100.0, 10.0, 110.0), filter)
            .collect();
        assert!(hits_b.contains(&leaf));
    }

    #[test]
    fn removing_node_removes_from_index() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
                ..Default::default()
            },
        );
        let r = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(10.0, 10.0, 30.0, 30.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Intersect with the child's rect should include the child
        let viewport = Rect::new(10.0, 10.0, 30.0, 30.0);
        let filter = QueryFilter {
            visible_only: true,
            pickable_only: false,
        };
        let has_child = tree.intersect_rect(viewport, filter).any(|id| id == r);
        assert!(has_child);

        // Remove and commit; child should no longer appear
        tree.remove(r);
        let _ = tree.commit();
        let has_child = tree.intersect_rect(viewport, filter).any(|id| id == r);
        assert!(!has_child);
    }

    #[test]
    fn visible_window_counts() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 10000.0),
                ..Default::default()
            },
        );
        let rows = 50_usize;
        let mut ids = Vec::with_capacity(rows);
        for i in 0..rows {
            let y0 = i as f64 * 10.0;
            let id = tree.insert(
                Some(root),
                LocalNode {
                    local_bounds: Rect::new(0.0, y0, 200.0, y0 + 10.0),
                    ..Default::default()
                },
            );
            ids.push(id);
        }
        let _ = tree.commit();

        let filter = QueryFilter {
            visible_only: true,
            pickable_only: false,
        };
        // 50px viewport should cover ~5 rows at top (exclude the root itself)
        let vis0: Vec<_> = tree
            .intersect_rect(Rect::new(0.0, 0.0, 200.0, 50.0), filter)
            .filter(|&id| id != root)
            .collect();
        assert!(vis0.len() >= 5 && vis0.len() <= 6);

        // Midway viewport should also cover ~5 rows; verify some expected indices present
        let vis_mid: Vec<_> = tree
            .intersect_rect(Rect::new(0.0, 95.0, 200.0, 145.0), filter)
            .filter(|&id| id != root)
            .collect();
        let present_mid: Vec<_> = vis_mid
            .iter()
            .filter_map(|id| ids.iter().position(|x| x == id))
            .collect();
        assert!(present_mid.iter().any(|&i| i == 9 || i == 10));
    }

    #[test]
    fn deep_subtree_reparent_updates_all_descendants() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 1000.0, 1000.0),
                ..Default::default()
            },
        );

        // Build two branches under root
        let a = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                local_transform: Affine::translate(Vec2::new(100.0, 0.0)),
                ..Default::default()
            },
        );
        let b = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                local_transform: Affine::translate(Vec2::new(0.0, 200.0)),
                ..Default::default()
            },
        );

        // Deep subtree under A
        let a1 = tree.insert(
            Some(a),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                local_transform: Affine::translate(Vec2::new(50.0, 0.0)),
                ..Default::default()
            },
        );
        let a2 = tree.insert(
            Some(a1),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                local_transform: Affine::translate(Vec2::new(0.0, 30.0)),
                ..Default::default()
            },
        );
        let leaf1 = tree.insert(
            Some(a2),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                ..Default::default()
            },
        );
        let leaf2 = tree.insert(
            Some(a2),
            LocalNode {
                local_bounds: Rect::new(10.0, 0.0, 20.0, 10.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        let filter = QueryFilter {
            visible_only: true,
            pickable_only: false,
        };
        // Initial world rect should be around x in [150..170], y in [30..40]
        let hits0: Vec<_> = tree
            .intersect_rect(Rect::new(150.0, 30.0, 170.0, 40.0), filter)
            .collect();
        assert!(hits0.contains(&leaf1) && hits0.contains(&leaf2));

        // Reparent a1 under branch B; subtree should move to around x in [50..70], y in [230..240]
        tree.reparent(a1, Some(b));
        let _ = tree.commit();
        let hits1: Vec<_> = tree
            .intersect_rect(Rect::new(50.0, 230.0, 70.0, 240.0), filter)
            .collect();
        assert!(hits1.contains(&leaf1) && hits1.contains(&leaf2));

        // Ensure old location is no longer populated
        let stale: Vec<_> = tree
            .intersect_rect(Rect::new(150.0, 30.0, 170.0, 40.0), filter)
            .collect();
        assert!(!stale.contains(&leaf1) && !stale.contains(&leaf2));
    }

    #[test]
    fn rotation_and_shear_aabb_are_conservative() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        let node = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
                local_transform: Affine::rotate(FRAC_PI_4),
                ..Default::default()
            },
        );
        let _ = tree.commit();
        let nb = tree.node(node).world.world_bounds;
        let expected =
            transform_rect_bbox(Affine::rotate(FRAC_PI_4), Rect::new(0.0, 0.0, 10.0, 10.0));
        let eps = 1e-9;
        assert!((nb.x0 - expected.x0).abs() < eps);
        assert!((nb.y0 - expected.y0).abs() < eps);
        assert!((nb.x1 - expected.x1).abs() < eps);
        assert!((nb.y1 - expected.y1).abs() < eps);

        // Shear
        tree.set_local_transform(node, Affine::new([1.0, 0.0, 0.5, 1.0, 0.0, 0.0]));
        let _ = tree.commit();
        let nb2 = tree.node(node).world.world_bounds;
        let expected2 = transform_rect_bbox(
            Affine::new([1.0, 0.0, 0.5, 1.0, 0.0, 0.0]),
            Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        assert!((nb2.x0 - expected2.x0).abs() < eps);
        assert!((nb2.y0 - expected2.y0).abs() < eps);
        assert!((nb2.x1 - expected2.x1).abs() < eps);
        assert!((nb2.y1 - expected2.y1).abs() < eps);
    }

    #[test]
    fn rounded_clip_intersection_edges() {
        let mut tree = Tree::new();
        let root = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 500.0, 500.0),
                ..Default::default()
            },
        );
        // Child large bounds, but clip outside of its bounds so it should not appear
        let child = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                ..Default::default()
            },
        );
        let _ = tree.commit();

        // Apply a clip completely outside
        let rr = RoundedRect::from_rect(Rect::new(300.0, 300.0, 400.0, 400.0), 5.0);
        tree.set_local_clip(child, Some(rr));
        let _ = tree.commit();

        let filter = QueryFilter {
            visible_only: true,
            pickable_only: false,
        };
        let hits: Vec<_> = tree
            .intersect_rect(Rect::new(0.0, 0.0, 210.0, 210.0), filter)
            .collect();
        assert!(!hits.contains(&child));

        // Now test parent clip restricting child (no child local clip set)
        tree.set_local_clip(child, None);
        // Parent gets a tight clip window (20..30)
        let parent_clip = RoundedRect::from_rect(Rect::new(20.0, 20.0, 30.0, 30.0), 2.0);
        tree.set_local_clip(root, Some(parent_clip));
        let _ = tree.commit();

        // Query a region outside of parent clip should not include child
        let out_hits: Vec<_> = tree
            .intersect_rect(Rect::new(0.0, 0.0, 15.0, 15.0), filter)
            .collect();
        assert!(!out_hits.contains(&child));

        // Query inside parent clip should include child
        let in_hits: Vec<_> = tree
            .intersect_rect(Rect::new(25.0, 25.0, 26.0, 26.0), filter)
            .collect();
        assert!(in_hits.contains(&child));
    }
}
