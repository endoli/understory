// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Public types for the box tree: node identifiers, flags, and local geometry.

use kurbo::{Affine, Rect, RoundedRect};
use understory_index::{SubtreeFilter, SubtreeSummary};

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

/// Bitmask describing which kinds of input a node (or its subtree) is interested in.
///
/// This is a behavioral hint used for pruning; it does not affect correctness.
/// Nodes that never handle a given kind of event can leave the corresponding bit unset
/// so that higher layers can cheaply skip them for those queries.
///
/// Typical usage:
/// - Pointer move / hover routing.
/// - Pointer down/up / drag gestures.
/// - Wheel events.
///
/// The mask is intentionally open-ended. Callers may define their own bits using
/// [`InterestMask::from_bits`] in addition to the built-in constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InterestMask(pub(crate) u64);

impl InterestMask {
    /// An empty mask (no interest).
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Construct a mask from raw bits.
    ///
    /// This is intentionally lenient: all bits are accepted. Use this to define
    /// application-specific interests in addition to the built-in constants.
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Return the underlying bit representation.
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Returns true if no bits are set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Pointer move / hover interest.
    pub const POINTER_MOVE: Self = Self(1 << 0);
    /// Pointer down (press) interest.
    pub const POINTER_DOWN: Self = Self(1 << 1);
    /// Pointer up (release) interest.
    pub const POINTER_UP: Self = Self(1 << 2);
    /// Wheel / scroll interest.
    pub const WHEEL: Self = Self(1 << 3);
    /// Keyboard interest (typically routed via focus).
    pub const KEY: Self = Self(1 << 4);
    /// Text input interest.
    pub const TEXT_INPUT: Self = Self(1 << 5);
    /// IME composition interest.
    pub const IME: Self = Self(1 << 6);
    /// Drag / gesture interest.
    pub const DRAG: Self = Self(1 << 7);
    // 8..63 reserved for future expansion.
}

impl core::ops::BitOr for InterestMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for InterestMask {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl core::ops::BitAnd for InterestMask {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl core::ops::BitAndAssign for InterestMask {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl SubtreeSummary for InterestMask {
    #[inline]
    fn empty() -> Self {
        Self::empty()
    }

    #[inline]
    fn combine(left: Self, right: Self) -> Self {
        left | right
    }
}

impl SubtreeFilter<InterestMask, InterestMask> for () {
    #[inline]
    fn may_contain(&self, summary: &InterestMask, query: &InterestMask) -> bool {
        !(*summary & *query).is_empty()
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
