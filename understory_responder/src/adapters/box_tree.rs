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
