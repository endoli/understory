// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Public types for the box tree: node identifiers, flags, and local geometry.

use kurbo::{Affine, Rect, RoundedRect};

/// Identifier for a node in the tree.
///
/// This is a small, copyable handle that stays stable across updates but becomes
/// invalid when the underlying slot is reused.
/// It consists of a slot index and a generation counter.
///
/// ## Semantics
///
/// - On insert, a fresh slot is allocated with generation `1`.
/// - On remove, the slot is freed; any existing `NodeId` that pointed to that slot is now stale.
/// - On reuse of a freed slot, its generation is incremented, producing a new, distinct `NodeId`.
///
/// ### Newer
///
/// A `NodeId` is considered newer than another when it has a higher generation.
/// If generations are equal, the one with the higher slot index is considered newer.
/// This total order is used only for deterministic tie-breaks in
/// [hit testing](crate::Tree::hit_test_point).
///
/// ### Liveness
///
/// Use [`Tree::is_alive`](crate::Tree::is_alive) to check whether a `NodeId` still refers to a live node.
/// Stale `NodeId`s never alias a different live node because the generation must match.
///
/// ### Notes
///
/// - The generation increments on slot reuse and never decreases.
/// - `u32` is ample for practical lifetimes; behavior on generation overflow is unspecified.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct NodeId(pub(crate) u32, pub(crate) u32);

impl NodeId {
    pub(crate) const fn new(idx: u32, generation: u32) -> Self {
        Self(idx, generation)
    }

    pub(crate) const fn idx(self) -> usize {
        self.0 as usize
    }
}

bitflags::bitflags! {
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
