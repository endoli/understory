// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! R-tree backend generic over scalar `T: Scalar` with SAH-like split.

use alloc::borrow::ToOwned;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Debug;

use crate::backend::Backend;
use crate::types::{Aabb2D, Scalar, area, union_aabb};

/// R-tree backend using SAH-like splits and widened accumulator metrics.
pub struct RTree<T: Scalar, P: Copy + Debug> {
    max_children: usize,
    min_children: usize,
    root: Option<NodeIdx>,
    arena: Vec<RNode<T, P>>,
    slots: Vec<Option<Aabb2D<T>>>,
}

#[derive(Clone)]
struct RNode<T: Scalar, P: Copy + Debug> {
    bbox: Aabb2D<T>,
    leaf: bool,
    children: Vec<RChild<T, P>>,
}

#[derive(Clone)]
enum RChild<T: Scalar, P: Copy + Debug> {
    Node(NodeIdx),
    Item {
        slot: usize,
        bbox: Aabb2D<T>,
        _p: core::marker::PhantomData<P>,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct NodeIdx(usize);

impl NodeIdx {
    const fn new(i: usize) -> Self {
        Self(i)
    }

    const fn get(self) -> usize {
        self.0
    }
}

impl<T: Scalar, P: Copy + Debug> Default for RTree<T, P> {
    fn default() -> Self {
        Self {
            max_children: 8,
            min_children: 4,
            root: None,
            arena: Vec::new(),
            slots: Vec::new(),
        }
    }
}

// Reduce clippy::type_complexity noise for local helpers.
type RChildren<TS, PS> = Vec<RChild<TS, PS>>;
type RBestSplit<TS, PS> = Option<(
    crate::types::ScalarAcc<TS>,
    RChildren<TS, PS>,
    RChildren<TS, PS>,
)>;

impl<T: Scalar, P: Copy + Debug> RTree<T, P> {
    fn ensure_slot(&mut self, slot: usize, bbox: Aabb2D<T>) {
        if self.slots.len() <= slot {
            self.slots.resize_with(slot + 1, || None);
        }
        self.slots[slot] = Some(bbox);
    }

    #[inline]
    fn ceil_div(a: usize, b: usize) -> usize {
        a.div_ceil(b)
    }

    fn centroid_x_of_aabb(a: &Aabb2D<T>) -> T {
        Scalar::mid(a.min_x, a.max_x)
    }

    fn centroid_y_of_aabb(a: &Aabb2D<T>) -> T {
        Scalar::mid(a.min_y, a.max_y)
    }

    /// STR-like bulk builder: creates a packed tree from items in one pass into `arena`.
    fn bulk_build_nodes(
        arena: &mut Vec<RNode<T, P>>,
        items: &mut [(usize, Aabb2D<T>)],
        max_children: usize,
    ) -> Option<NodeIdx> {
        if items.is_empty() {
            return None;
        }

        // Build leaf level (as node indices in the arena)
        let n = items.len();
        let num_leaves = Self::ceil_div(n, max_children);
        let mut gx = 1_usize;
        while gx * gx < num_leaves {
            gx += 1;
        }
        items.sort_by(|a, b| {
            Self::centroid_x_of_aabb(&a.1)
                .partial_cmp(&Self::centroid_x_of_aabb(&b.1))
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        let slice_size = Self::ceil_div(n, gx);
        let mut leaves: Vec<usize> = Vec::new();
        for slice in items.chunks_mut(slice_size) {
            slice.sort_by(|a, b| {
                Self::centroid_y_of_aabb(&a.1)
                    .partial_cmp(&Self::centroid_y_of_aabb(&b.1))
                    .unwrap_or(core::cmp::Ordering::Equal)
            });
            for chunk in slice.chunks(max_children) {
                let mut children: Vec<RChild<T, P>> = Vec::with_capacity(chunk.len());
                for (slot, bbox) in chunk.iter().copied() {
                    children.push(RChild::Item {
                        slot,
                        bbox,
                        _p: core::marker::PhantomData,
                    });
                }
                let bbox = Self::node_bbox(arena, &children);
                let idx = arena.len();
                arena.push(RNode {
                    bbox,
                    leaf: true,
                    children,
                });
                leaves.push(idx);
            }
        }

        // Promote until a single root remains
        let mut level: Vec<usize> = leaves;
        while level.len() > max_children {
            let n_nodes = level.len();
            let num_parents = Self::ceil_div(n_nodes, max_children);
            let mut gx = 1_usize;
            while gx * gx < num_parents {
                gx += 1;
            }
            level.sort_by(|&a, &b| {
                Self::centroid_x_of_aabb(&arena[a].bbox)
                    .partial_cmp(&Self::centroid_x_of_aabb(&arena[b].bbox))
                    .unwrap_or(core::cmp::Ordering::Equal)
            });
            let slice_size = Self::ceil_div(n_nodes, gx);
            let mut next: Vec<usize> = Vec::new();
            for slice in level.chunks_mut(slice_size) {
                slice.sort_by(|&a, &b| {
                    Self::centroid_y_of_aabb(&arena[a].bbox)
                        .partial_cmp(&Self::centroid_y_of_aabb(&arena[b].bbox))
                        .unwrap_or(core::cmp::Ordering::Equal)
                });
                let mut i = 0;
                while i < slice.len() {
                    let end = core::cmp::min(i + max_children, slice.len());
                    let chunk = &mut slice[i..end];
                    let mut children: Vec<RChild<T, P>> = Vec::with_capacity(chunk.len());
                    for child_idx in chunk.iter_mut() {
                        let ch_idx = *child_idx;
                        children.push(RChild::Node(NodeIdx::new(ch_idx)));
                    }
                    let bbox = Self::node_bbox(arena, &children);
                    let idx = arena.len();
                    arena.push(RNode {
                        bbox,
                        leaf: false,
                        children,
                    });
                    next.push(idx);
                    i = end;
                }
            }
            level = next;
        }

        // Create root
        if level.len() == 1 {
            Some(NodeIdx::new(level[0]))
        } else {
            // Pack remaining nodes under a new root
            let mut children: Vec<RChild<T, P>> = Vec::with_capacity(level.len());
            for idx in level.into_iter() {
                children.push(RChild::Node(NodeIdx::new(idx)));
            }
            let bbox = Self::node_bbox(arena, &children);
            let root_idx = arena.len();
            arena.push(RNode {
                bbox,
                leaf: false,
                children,
            });
            Some(NodeIdx::new(root_idx))
        }
    }

    /// Build an `RTree` from a set of (slot, bbox) pairs using a packed layout.
    pub fn bulk_build_default(pairs: &[(usize, Aabb2D<T>)]) -> Self {
        let max_children = 8; // default matches Self::default
        let mut items = pairs.to_vec();
        let mut arena: Vec<RNode<T, P>> = Vec::new();
        let root = Self::bulk_build_nodes(&mut arena, &mut items[..], max_children);
        let mut slots: Vec<Option<Aabb2D<T>>> = Vec::new();
        for (slot, bbox) in pairs.iter().copied() {
            if slots.len() <= slot {
                slots.resize_with(slot + 1, || None);
            }
            slots[slot] = Some(bbox);
        }
        Self {
            max_children,
            min_children: 4,
            root,
            arena,
            slots,
        }
    }

    fn node_bbox(arena: &[RNode<T, P>], children: &[RChild<T, P>]) -> Aabb2D<T> {
        let mut it = children.iter();
        let first = match it.next() {
            Some(RChild::Node(i)) => arena[i.get()].bbox,
            Some(RChild::Item { bbox, .. }) => *bbox,
            None => Aabb2D::new(T::zero(), T::zero(), T::zero(), T::zero()),
        };
        it.fold(first, |acc, c| match c {
            RChild::Node(i) => union_aabb(acc, arena[i.get()].bbox),
            RChild::Item { bbox, .. } => union_aabb(acc, *bbox),
        })
    }

    fn enlarge_cost(a: &Aabb2D<T>, b: &Aabb2D<T>) -> T::Acc {
        let u = union_aabb(*a, *b);
        area(&u) - area(a)
    }

    fn choose_child(arena: &[RNode<T, P>], children: &[RChild<T, P>], bbox: &Aabb2D<T>) -> usize {
        let mut best_idx = 0_usize;
        let mut best_cost: Option<T::Acc> = None;
        for (i, c) in children.iter().enumerate() {
            let cb = match c {
                RChild::Node(idx) => arena[idx.get()].bbox,
                RChild::Item { bbox, .. } => *bbox,
            };
            let cost = Self::enlarge_cost(&cb, bbox);
            if best_cost.map(|bc| cost < bc).unwrap_or(true) {
                best_cost = Some(cost);
                best_idx = i;
            }
        }
        best_idx
    }

    /// SAH-like split: sort along an axis, precompute prefix/suffix AABBs, and
    /// choose `k` that minimizes `area(LB_k) * k + area(RB_k) * (n - k)`.
    fn split_children_with<F>(
        children: &mut [RChild<T, P>],
        _max_children: usize,
        min_children: usize,
        mut bbox_of: F,
    ) -> (RChildren<T, P>, RChildren<T, P>)
    where
        F: FnMut(&RChild<T, P>) -> Aabb2D<T>,
    {
        fn centroid_x<T: Scalar>(b: &Aabb2D<T>) -> T {
            Scalar::mid(b.min_x, b.max_x)
        }
        fn centroid_y<T: Scalar>(b: &Aabb2D<T>) -> T {
            Scalar::mid(b.min_y, b.max_y)
        }
        let n = children.len();
        let mut best: RBestSplit<T, P> = None;
        for axis in 0..2 {
            let mut v = children.to_owned();
            if axis == 0 {
                v.sort_by(|a, b| {
                    centroid_x::<T>(&bbox_of(a))
                        .partial_cmp(&centroid_x::<T>(&bbox_of(b)))
                        .unwrap_or(core::cmp::Ordering::Equal)
                });
            } else {
                v.sort_by(|a, b| {
                    centroid_y::<T>(&bbox_of(a))
                        .partial_cmp(&centroid_y::<T>(&bbox_of(b)))
                        .unwrap_or(core::cmp::Ordering::Equal)
                });
            }

            // Precompute prefix and suffix bounding boxes to evaluate costs in O(1) per split.
            let mut prefix: Vec<Aabb2D<T>> = Vec::with_capacity(n);
            for (i, c) in v.iter().enumerate() {
                let bb = bbox_of(c);
                if i == 0 {
                    prefix.push(bb);
                } else {
                    let prev = *prefix.last().unwrap();
                    prefix.push(union_aabb(prev, bb));
                }
            }
            let mut suffix: Vec<Aabb2D<T>> = Vec::with_capacity(n);
            for (i, c) in v.iter().enumerate().rev() {
                let bb = bbox_of(c);
                if i == n - 1 {
                    suffix.push(bb);
                } else {
                    let prev = *suffix.last().unwrap();
                    suffix.push(union_aabb(bb, prev));
                }
            }
            suffix.reverse();

            for k in min_children..=(n - min_children) {
                let lb = prefix[k - 1];
                let rb = suffix[k];
                let c = area(&lb) * T::acc_from_usize(k) + area(&rb) * T::acc_from_usize(n - k);
                if best.as_ref().map(|(bc, _, _)| c < *bc).unwrap_or(true) {
                    let left = v[..k].to_vec();
                    let right = v[k..].to_vec();
                    best = Some((c, left, right));
                }
            }
        }
        let (_, l, r) = best.expect("split requires overflow");
        (l, r)
    }

    fn insert_node(
        arena: &mut Vec<RNode<T, P>>,
        node_idx: usize,
        slot: usize,
        bbox: Aabb2D<T>,
        max_children: usize,
        min_children: usize,
    ) -> Option<usize> {
        if arena[node_idx].leaf {
            // Safe separate block to minimize mutable borrows
            {
                let node = &mut arena[node_idx];
                node.children.push(RChild::Item {
                    slot,
                    bbox,
                    _p: core::marker::PhantomData,
                });
                node.bbox = union_aabb(node.bbox, bbox);
                if node.children.len() <= max_children {
                    return None;
                }
            }
            // Overflow split for a leaf: children are Items
            let (left, right, l_bbox, r_bbox) =
                {
                    let mut items = core::mem::take(&mut arena[node_idx].children);
                    let (left, right) =
                        Self::split_children_with(&mut items, max_children, min_children, |c| {
                            match c {
                                RChild::Item { bbox, .. } => *bbox,
                                RChild::Node(_) => unreachable!(),
                            }
                        });
                    let l_bbox = Self::node_bbox(arena, &left);
                    let r_bbox = Self::node_bbox(arena, &right);
                    (left, right, l_bbox, r_bbox)
                };
            {
                let node = &mut arena[node_idx];
                node.leaf = true;
                node.children = left;
                node.bbox = l_bbox;
            }
            let r_idx = arena.len();
            arena.push(RNode {
                bbox: r_bbox,
                leaf: true,
                children: right,
            });
            Some(r_idx)
        } else {
            // Choose child without holding &mut to the node across arena borrows
            let idx = {
                let children = &arena[node_idx].children;
                Self::choose_child(arena, children, &bbox)
            };
            let split = match arena[node_idx].children[idx] {
                RChild::Node(child_idx) => Self::insert_node(
                    arena,
                    child_idx.get(),
                    slot,
                    bbox,
                    max_children,
                    min_children,
                ),
                RChild::Item { .. } => None,
            };
            // update node bbox
            arena[node_idx].bbox = union_aabb(arena[node_idx].bbox, bbox);
            if let Some(new_right_idx) = split {
                // Insert new right sibling and handle possible overflow
                arena[node_idx]
                    .children
                    .insert(idx + 1, RChild::Node(NodeIdx::new(new_right_idx)));
                if arena[node_idx].children.len() > max_children {
                    let (left, right, l_bbox, r_bbox) = {
                        let mut ch = core::mem::take(&mut arena[node_idx].children);
                        let (left, right) =
                            Self::split_children_with(&mut ch, max_children, min_children, |c| {
                                match c {
                                    RChild::Item { bbox, .. } => *bbox,
                                    RChild::Node(i) => arena[i.get()].bbox,
                                }
                            });
                        let l_bbox = Self::node_bbox(arena, &left);
                        let r_bbox = Self::node_bbox(arena, &right);
                        (left, right, l_bbox, r_bbox)
                    };
                    arena[node_idx].leaf = false;
                    arena[node_idx].children = left;
                    arena[node_idx].bbox = l_bbox;
                    let r_idx = arena.len();
                    arena.push(RNode {
                        bbox: r_bbox,
                        leaf: false,
                        children: right,
                    });
                    return Some(r_idx);
                }
            }
            None
        }
    }

    fn search_remove(
        arena: &mut Vec<RNode<T, P>>,
        node_idx: usize,
        slot: usize,
        old: &Aabb2D<T>,
    ) -> bool {
        let node_bbox = arena[node_idx].bbox;
        if node_bbox.intersect(old).is_empty() {
            return false;
        }
        if arena[node_idx].leaf {
            let before = arena[node_idx].children.len();
            arena[node_idx].children.retain(|c| match c {
                RChild::Item { slot: s, .. } => *s != slot,
                _ => true,
            });
            if arena[node_idx].children.len() != before {
                let bb = Self::node_bbox(arena, &arena[node_idx].children);
                arena[node_idx].bbox = bb;
                return true;
            }
            false
        } else {
            let mut removed = false;
            // Recurse into child nodes
            let child_indices: Vec<NodeIdx> = arena[node_idx]
                .children
                .iter()
                .filter_map(|c| {
                    if let RChild::Node(i) = c {
                        Some(*i)
                    } else {
                        None
                    }
                })
                .collect();
            for ci in child_indices {
                if Self::search_remove(arena, ci.get(), slot, old) {
                    removed = true;
                }
            }
            if removed {
                let new_children = {
                    let old_children = core::mem::take(&mut arena[node_idx].children);
                    old_children
                        .into_iter()
                        .filter(|c| match c {
                            RChild::Node(i) => !arena[i.get()].children.is_empty(),
                            _ => true,
                        })
                        .collect::<Vec<_>>()
                };
                arena[node_idx].children = new_children;
                if !arena[node_idx].children.is_empty() {
                    let bb = Self::node_bbox(arena, &arena[node_idx].children);
                    arena[node_idx].bbox = bb;
                }
            }
            removed
        }
    }

    /// Attempt to update an item's AABB in-place without remove+insert.
    /// Returns true if the item was found and updated; recomputes ancestor bboxes on the path.
    fn update_in_place(
        arena: &mut Vec<RNode<T, P>>,
        node_idx: usize,
        slot: usize,
        old: Aabb2D<T>,
        new: Aabb2D<T>,
    ) -> bool {
        let interest = union_aabb(old, new);
        if arena[node_idx].bbox.intersect(&interest).is_empty() {
            return false;
        }
        if arena[node_idx].leaf {
            let mut found = false;
            for c in &mut arena[node_idx].children {
                if let RChild::Item { slot: s, bbox, .. } = c
                    && *s == slot
                {
                    *bbox = new;
                    found = true;
                    break;
                }
            }
            if found {
                let bb = Self::node_bbox(arena, &arena[node_idx].children);
                arena[node_idx].bbox = bb;
            }
            found
        } else {
            let child_indices: Vec<NodeIdx> = arena[node_idx]
                .children
                .iter()
                .filter_map(|c| {
                    if let RChild::Node(i) = c {
                        Some(*i)
                    } else {
                        None
                    }
                })
                .collect();
            let mut updated = false;
            for ci in child_indices {
                if Self::update_in_place(arena, ci.get(), slot, old, new) {
                    updated = true;
                    break;
                }
            }
            if updated {
                let bb = Self::node_bbox(arena, &arena[node_idx].children);
                arena[node_idx].bbox = bb;
            }
            updated
        }
    }
}

impl<T: Scalar, P: Copy + Debug> Backend<T, P> for RTree<T, P> {
    fn insert(&mut self, slot: usize, aabb: Aabb2D<T>) {
        self.ensure_slot(slot, aabb);
        match self.root {
            None => {
                let mut leaf = RNode::<T, P> {
                    bbox: aabb,
                    leaf: true,
                    children: Vec::new(),
                };
                leaf.children.push(RChild::Item {
                    slot,
                    bbox: aabb,
                    _p: core::marker::PhantomData,
                });
                let idx = self.arena.len();
                self.arena.push(leaf);
                self.root = Some(NodeIdx::new(idx));
            }
            Some(root_idx) => {
                let split = Self::insert_node(
                    &mut self.arena,
                    root_idx.get(),
                    slot,
                    aabb,
                    self.max_children,
                    self.min_children,
                );
                if let Some(right_idx) = split {
                    // Create a new root combining old root and new right child
                    let left_bb = self.arena[root_idx.get()].bbox;
                    let right_bb = self.arena[right_idx].bbox;
                    let new_bb = union_aabb(left_bb, right_bb);
                    let children = vec![
                        RChild::Node(root_idx),
                        RChild::Node(NodeIdx::new(right_idx)),
                    ];
                    let idx = self.arena.len();
                    self.arena.push(RNode {
                        bbox: new_bb,
                        leaf: false,
                        children,
                    });
                    self.root = Some(NodeIdx::new(idx));
                }
            }
        }
    }

    fn update(&mut self, slot: usize, aabb: Aabb2D<T>) {
        if let Some(old) = self.slots.get(slot).and_then(|x| *x)
            && let Some(root_idx) = self.root
        {
            if Self::update_in_place(&mut self.arena, root_idx.get(), slot, old, aabb) {
                if let Some(s) = self.slots.get_mut(slot) {
                    *s = Some(aabb);
                }
                return;
            }
            let _ = Self::search_remove(&mut self.arena, root_idx.get(), slot, &old);
        }
        self.insert(slot, aabb);
    }

    fn remove(&mut self, slot: usize) {
        if let Some(old) = self.slots.get(slot).and_then(|x| *x) {
            if let Some(root_idx) = self.root {
                let _ = Self::search_remove(&mut self.arena, root_idx.get(), slot, &old);
            }
            if let Some(s) = self.slots.get_mut(slot) {
                *s = None;
            }
        }
    }

    fn clear(&mut self) {
        self.root = None;
        self.arena.clear();
        self.slots.clear();
    }

    fn visit_point<F: FnMut(usize)>(&self, x: T, y: T, mut f: F) {
        let Some(root_idx) = self.root else {
            return;
        };
        let p = Aabb2D::new(x, y, x, y);
        let mut stack = vec![root_idx];
        while let Some(i) = stack.pop() {
            let n = &self.arena[i.get()];
            if n.bbox.intersect(&p).is_empty() {
                continue;
            }
            if n.leaf {
                for c in &n.children {
                    if let RChild::Item { slot, bbox, .. } = c
                        && !bbox.intersect(&p).is_empty()
                    {
                        f(*slot);
                    }
                }
            } else {
                for c in &n.children {
                    if let RChild::Node(ci) = c {
                        stack.push(*ci);
                    }
                }
            }
        }
    }

    fn visit_rect<F: FnMut(usize)>(&self, rect: Aabb2D<T>, mut f: F) {
        let Some(root_idx) = self.root else {
            return;
        };
        let mut stack = vec![root_idx];
        while let Some(i) = stack.pop() {
            let n = &self.arena[i.get()];
            if n.bbox.intersect(&rect).is_empty() {
                continue;
            }
            if n.leaf {
                for c in &n.children {
                    if let RChild::Item { slot, bbox, .. } = c
                        && !bbox.intersect(&rect).is_empty()
                    {
                        f(*slot);
                    }
                }
            } else {
                for c in &n.children {
                    if let RChild::Node(ci) = c {
                        stack.push(*ci);
                    }
                }
            }
        }
    }
}

impl<T: Scalar, P: Copy + Debug> Debug for RTree<T, P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let total = self.slots.len();
        let alive = self.slots.iter().filter(|e| e.is_some()).count();
        let has_root = self.root.is_some();
        f.debug_struct("RTree")
            .field("max_children", &self.max_children)
            .field("min_children", &self.min_children)
            .field("arena_nodes", &self.arena.len())
            .field("total_slots", &total)
            .field("alive", &alive)
            .field("has_root", &has_root)
            .finish_non_exhaustive()
    }
}

/// Convenience type aliases.
/// R-tree with i64 coordinates and i128 metrics.
pub type RTreeI64<P> = RTree<i64, P>;

/// R-tree with f32 coordinates and f64 metrics.
pub type RTreeF32<P> = RTree<f32, P>;

/// R-tree with f64 coordinates and f64 metrics.
pub type RTreeF64<P> = RTree<f64, P>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Index;

    #[test]
    fn rtree_i64_basic_insert_query() {
        let mut idx = Index::<i64, u32>::with_rtree();
        let _k1 = idx.insert(Aabb2D::new(0, 0, 10, 10), 1);
        let _k2 = idx.insert(Aabb2D::new(5, 5, 15, 15), 2);
        let _ = idx.commit();
        let hits: Vec<_> = idx.query_point(6, 6).collect();
        assert_eq!(hits.len(), 2);
        let payloads: Vec<_> = hits.into_iter().map(|(_, p)| p).collect();
        assert!(payloads.contains(&1) && payloads.contains(&2));
        let q: Vec<_> = idx.query_rect(Aabb2D::new(12, 12, 20, 20)).collect();
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn rtree_i64_update_remove() {
        let mut idx = Index::<i64, u32>::with_rtree();
        let k = idx.insert(Aabb2D::new(0, 0, 10, 10), 1);
        let _ = idx.commit();
        // Move far away (in-place update should still keep structure consistent)
        idx.update(k, Aabb2D::new(100, 100, 110, 110));
        let _ = idx.commit();
        assert_eq!(idx.query_point(1, 1).count(), 0);
        assert_eq!(idx.query_point(105, 105).count(), 1);
        idx.remove(k);
        let _ = idx.commit();
        assert_eq!(idx.query_point(105, 105).count(), 0);
    }

    #[test]
    fn rtree_update_in_place_correctness() {
        // Use backend directly to inspect structure.
        let mut b: RTree<i64, u8> = RTree::default();
        // Insert a couple of items into a single leaf.
        b.insert(0, Aabb2D::new(0, 0, 10, 10));
        b.insert(1, Aabb2D::new(12, 0, 22, 10));
        let arena_before = b.arena.len();
        let root_before_is_leaf = b.root.map(|ri| b.arena[ri.get()].leaf).unwrap_or(false);

        // Update slot 0 to a far-away location; our in-place path should update bbox
        // and maintain a valid tree without adding nodes.
        b.update(0, Aabb2D::new(100, 100, 110, 110));

        // Structure sanity: arena size shouldn't grow from an update.
        assert_eq!(b.arena.len(), arena_before);
        // Root should remain a node (likely still leaf for tiny set).
        assert_eq!(
            b.root.map(|ri| b.arena[ri.get()].leaf).unwrap_or(false),
            root_before_is_leaf
        );

        // Query correctness: old spot no longer hits; new spot hits slot 0.
        let v_old: Vec<_> = b.query_point(5, 5).collect();
        assert!(v_old.is_empty());
        let v_new: Vec<_> = b.query_point(105, 105).collect();
        assert_eq!(v_new, vec![0]);
        // Neighbor still present.
        let v_neighbor: Vec<_> = b.query_point(15, 5).collect();
        assert_eq!(v_neighbor, vec![1]);
    }
}
