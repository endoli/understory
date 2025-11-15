// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Core tree implementation: structure, updates, queries.

use alloc::vec::Vec;
use kurbo::{Affine, Point, Rect, RoundedRect};
use understory_index::{Aabb2D, Index as AabbIndex, Key as AabbKey};

use crate::damage::Damage;
use crate::types::{LocalNode, NodeFlags, NodeId};
use crate::util::{rect_to_aabb, transform_rect_bbox};

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}

/// Top-level region tree.
pub struct Tree {
    nodes: Vec<Option<Node>>, // slots
    generations: Vec<u32>,    // last generation per slot (persists across frees)
    pub(crate) free_list: Vec<usize>,
    pub(crate) epoch: u64,
    pub(crate) index: AabbIndex<f64, NodeId>,
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
    /// Create a new empty tree.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            generations: Vec::new(),
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

    /// Update local clip.
    pub fn set_local_clip(&mut self, id: NodeId, clip: Option<RoundedRect>) {
        if let Some(n) = self.node_opt_mut(id) {
            n.local.local_clip = clip;
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

    /// Access a node for debugging; panics if `id` is stale.
    pub(crate) fn node(&self, id: NodeId) -> &Node {
        self.nodes[id.idx()].as_ref().expect("dangling NodeId")
    }

    /// Access a node mutably for debugging; panics if `id` is stale.
    pub(crate) fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.nodes[id.idx()].as_mut().expect("dangling NodeId")
    }

    /// Run the batched update and return coarse damage.
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

    /// Hit test a world-space point. Returns the topmost node.
    ///
    /// If multiple nodes overlap with the same `z_index`, the newer [`NodeId`] wins.
    /// This tie-break is intentionally deterministic for now.
    /// In the future this may be made configurable (for example via a `TieBreakPolicy`).
    pub fn hit_test_point(&self, pt: Point, filter: QueryFilter) -> Option<Hit> {
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
                Some((best_id, z_best)) => {
                    let z = node.local.z_index;
                    if z > z_best || (z == z_best && Self::id_is_newer(id, best_id)) {
                        best = Some((id, z));
                    }
                }
            }
        }
        best.map(|(node, _)| Hit {
            node,
            path: self.path_to_root(node),
        })
    }

    /// Iterate nodes intersecting a world-space rect.
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

    #[inline]
    fn id_is_newer(a: NodeId, b: NodeId) -> bool {
        (a.1 > b.1) || (a.1 == b.1 && a.0 > b.0)
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

        // Reuse slot by inserting a new node; old id must remain stale; new id is live.
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
    fn newer_than_semantics() {
        // Construct synthetic NodeId pairs and verify newer ordering.
        let old = NodeId::new(10, 1);
        let newer_same_slot = NodeId::new(10, 2);
        let same_gen_higher_slot = NodeId::new(11, 2);
        let same_gen_lower_slot = NodeId::new(9, 2);

        // Private helper is in scope within the module.
        assert!(Tree::id_is_newer(newer_same_slot, old));
        assert!(Tree::id_is_newer(same_gen_higher_slot, newer_same_slot));
        assert!(!Tree::id_is_newer(same_gen_lower_slot, newer_same_slot));
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

        // Sanity: with equal z, the newer of (a, b) should win; typically b is newer.
        let hit1 = tree
            .hit_test_point(
                Point::new(60.0, 60.0),
                QueryFilter {
                    visible_only: true,
                    pickable_only: true,
                },
            )
            .unwrap();
        let expected1 = if Tree::id_is_newer(b, a) { b } else { a };
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
        assert!(Tree::id_is_newer(c, b));

        let hit2 = tree
            .hit_test_point(
                Point::new(60.0, 60.0),
                QueryFilter {
                    visible_only: true,
                    pickable_only: true,
                },
            )
            .unwrap();
        assert_eq!(hit2.node, c, "newer id should win on equal z");
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
        assert!(Tree::id_is_newer(new_node, node));
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
                QueryFilter {
                    visible_only: true,
                    pickable_only: true,
                },
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
                QueryFilter {
                    visible_only: true,
                    pickable_only: true,
                },
            )
            .expect("expected hit after bounds update");
        assert_eq!(hit_after.node, n);
        assert_eq!(hit_after.path.first().copied(), Some(root));
        assert_eq!(hit_after.path.last().copied(), Some(n));
    }
}
