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
    /// Node flags controlling visibility, picking, and focus behavior.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct NodeFlags: u8 {
        /// Node is visible (participates in rendering and intersection queries).
        const VISIBLE  = 0b0000_0001;
        /// Node is pickable (participates in hit testing).
        const PICKABLE = 0b0000_0010;
        /// Node is focusable (can receive keyboard focus).
        const FOCUSABLE = 0b0000_0100;
    }
}

impl Default for NodeFlags {
    #[inline(always)]
    fn default() -> Self {
        Self::VISIBLE | Self::PICKABLE
    }
}

/// Local geometry for a node.
#[derive(Clone, Debug)]
pub struct LocalNode {
    /// Local (untransformed) bounds for this node's own content.
    ///
    /// - Expressed in the node's local coordinate space, before `local_transform`.
    /// - Used to derive the node's world-space AABB for spatial indexing and hit-testing.
    /// - Children are **not** constrained by their parent's `local_bounds`; their bounds are
    ///   computed independently from their own `LocalNode`.
    ///
    /// For non-axis-aligned content, use a loose AABB that fully contains what is drawn; it may be
    /// larger than the tight bounding box.
    pub local_bounds: Rect,
    /// Local transform from this node's coordinate space into its parent's.
    ///
    /// - Combined with ancestor transforms to produce `world_transform`.
    /// - Applied to both `local_bounds` and `local_clip` when computing world-space data.
    /// - Order is ancestors * local: the local transform is applied before the ancestor
    ///   transforms to calculate the world transform (`world_transform = ancestors * local`).
    pub local_transform: Affine,
    /// Optional local clip (rounded-rect) applied to this node and its subtree.
    ///
    /// - Expressed in the node's local coordinate space and transformed into world space.
    /// - Combined with any ancestor clip to form an inherited `world_clip`.
    /// - The node's world-space AABB is intersected with this clip for spatial indexing.
    ///
    /// Intuitively:
    /// - Points outside `local_bounds` may still hit children.
    /// - Points outside `local_clip` (once transformed) cannot hit this node or any descendant.
    ///   Backends may still apply more precise clipping during rendering.
    pub local_clip: Option<RoundedRect>,
    /// Z-order within the parent stacking context.
    ///
    /// - Higher values are drawn on top of siblings with lower values.
    /// - Hit testing also compares `z_index` across different parents when nodes overlap;
    ///   depth in the tree and insertion order are used as secondary tie-breakers.
    pub z_index: i32,
    /// Visibility and interaction flags.
    ///
    /// - [`NodeFlags::VISIBLE`] controls participation in visibility queries and hit testing.
    /// - [`NodeFlags::PICKABLE`] is consulted by hit testing.
    /// - [`NodeFlags::FOCUSABLE`] is consulted by focus/navigation layers.
    ///
    /// Flags do not affect layout; they only influence queries and higher-level behavior.
    pub flags: NodeFlags,
}

impl Default for LocalNode {
    #[inline(always)]
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
