// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Core types for the responder: phases, outcomes, keys, hits, lookups, and dispatch.
//!
//! ## Overview
//!
//! These types describe the responder protocol and its inputs/outputs.
//! They are referenced by the [`router`](crate::router) and used by downstream toolkits.

use alloc::vec::Vec;

/// Phases of event propagation.
///
/// Appears on each [`Dispatch`] item produced by
/// [`Router::handle_with_hits`](crate::router::Router::handle_with_hits).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Phase {
    /// Parent-to-target traversal.
    Capture,
    /// Target node.
    Target,
    /// Target-to-parent traversal.
    Bubble,
}

/// Handler outcome controlling propagation.
///
/// A higher‑level dispatcher (see crate docs) can use this as the return
/// value from per‑node handlers to decide whether to continue within a phase
/// or abort remaining phases.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// Continue within the current phase.
    Continue,
    /// Stop propagation within the current phase.
    Stop,
    /// Stop and mark consumed (for higher-level policies).
    StopAndConsume,
}

/// Policy for breaking ties after equal primary depth.
///
/// Note: The [router](crate::router::Router) does not know how to compare arbitrary node keys `K`.
/// Implementations can supply a custom tie-break outside the router by pre-sorting hits,
/// or future versions may accept an ordering callback.
/// For now, ties are stable with respect to input order, and the router selects the last.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TieBreakPolicy {
    /// Prefer the more recently created identifier when available.
    Newer,
    /// Prefer the less recently created identifier when available.
    Older,
    /// Prefer the smaller identifier when available.
    MinId,
    /// Prefer the larger identifier when available.
    MaxId,
}

/// Primary depth ordering across heterogeneous hits.
///
/// This is carried by [`ResolvedHit`] and used by
/// [`Router::handle_with_hits`](crate::router::Router::handle_with_hits) to rank candidates.
///
/// Precondition: `Distance` should be finite (no NaN) for meaningful ordering.
/// If NaN is encountered, tie-breaking falls back to stable order.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DepthKey {
    /// 2D z-index; higher is nearer to the user.
    Z(i32),
    /// 3D ray distance; lower is nearer to the user.
    Distance(f32),
}

impl Eq for DepthKey {}

impl Ord for DepthKey {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        use core::cmp::Ordering::*;
        match (*self, *other) {
            (Self::Z(a), Self::Z(b)) => a.cmp(&b),
            (Self::Distance(a), Self::Distance(b)) => b.partial_cmp(&a).unwrap_or(Equal),
            // Cross-kind ordering is undefined globally; treat Z as above Distance by default.
            (Self::Z(_), Self::Distance(_)) => Greater,
            (Self::Distance(_), Self::Z(_)) => Less,
        }
    }
}

impl PartialOrd for DepthKey {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(Ord::cmp(self, other))
    }
}

/// Placeholder for world→local transformation and any per-target conversion info.
///
/// Carried by [`ResolvedHit`] and propagated to every [`Dispatch`] entry in the
/// resulting sequence from
/// [`Router::handle_with_hits`](crate::router::Router::handle_with_hits).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Localizer {
    // Future: carry inverse transforms or scroll offsets as needed.
}

/// A resolved hit to be routed.
///
/// Typically obtained from your picker (for example a 2D box tree hit test or a
/// 3D ray cast). It is the input to
/// [`Router::handle_with_hits`](crate::router::Router::handle_with_hits).
#[derive(Clone, Debug)]
pub struct ResolvedHit<K, M = ()> {
    /// Node key associated with the hit.
    pub node: K,
    /// Optional root→target path; if absent, the router may consult [`ParentLookup`] to derive one.
    pub path: Option<Vec<K>>,
    /// Primary depth ordering key used to pick the winning target from candidates.
    pub depth_key: DepthKey,
    /// Transformation context from world space to the target's local coordinates.
    pub localizer: Localizer,
    /// Optional metadata carried alongside the hit (e.g., text or ray-hit details).
    pub meta: M,
}

/// Map nodes to toolkit widget identifiers.
///
/// Implement this trait and supply it to the router so that each [`Dispatch`]
/// can include an optional widget identifier alongside the node key.
pub trait WidgetLookup<K> {
    /// Toolkit widget identifier type associated with a node.
    type WidgetId: Copy + core::fmt::Debug;
    /// Returns a widget identifier for the given node, if any.
    fn widget_of(&self, node: &K) -> Option<Self::WidgetId>;
}

/// Look up the parent of a node to reconstruct a root→target path for propagation.
///
/// The [router](crate::router::Router) consults this when a [`ResolvedHit::path`] is absent, if you
/// construct it via [`Router::with_parent`](crate::router::Router::with_parent).
pub trait ParentLookup<K> {
    /// Returns the parent of `node`, or `None` if `node` is a root.
    fn parent_of(&self, node: &K) -> Option<K>;
}

/// A no‑op parent provider used by default when no parent lookup is needed.
///
/// Used by [`Router::new`](crate::router::Router::new). All calls to
/// [`ParentLookup::parent_of`] return `None`.
#[derive(Copy, Clone, Debug, Default)]
pub struct NoParent;

impl<K> ParentLookup<K> for NoParent {
    #[inline]
    fn parent_of(&self, _node: &K) -> Option<K> {
        None
    }
}

/// A single dispatch item.
///
/// Produced by [`Router::handle_with_hits`](crate::router::Router::handle_with_hits), and typically fed
/// into a higher‑level dispatcher that invokes handlers in [`Capture`](Phase::Capture), then
/// [`Target`](Phase::Target), then [`Bubble`](Phase::Bubble) phases.
#[derive(Clone, Debug)]
pub struct Dispatch<K, W, M = ()> {
    /// Propagation phase for this step (capture, target, or bubble).
    pub phase: Phase,
    /// Node associated with this dispatch step.
    pub node: K,
    /// Optional widget id corresponding to the node.
    pub widget: Option<W>,
    /// Transformation context for local event coordinates.
    pub localizer: Localizer,
    /// Optional metadata (cloned from the winning hit).
    pub meta: Option<M>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depthkey_z_ordering() {
        assert!(DepthKey::Z(10) > DepthKey::Z(5));
        assert!(DepthKey::Z(-1) < DepthKey::Z(0));
        assert_eq!(
            DepthKey::Z(7).cmp(&DepthKey::Z(7)),
            core::cmp::Ordering::Equal
        );
    }

    #[test]
    fn depthkey_distance_ordering() {
        // Smaller distance is considered nearer and thus greater in ordering.
        assert!(DepthKey::Distance(0.1) > DepthKey::Distance(0.2));
        assert!(DepthKey::Distance(1.0) < DepthKey::Distance(0.5));
        assert_eq!(
            DepthKey::Distance(0.25).cmp(&DepthKey::Distance(0.25)),
            core::cmp::Ordering::Equal
        );
    }

    #[test]
    fn depthkey_mixed_ordering() {
        // Z is always considered greater than Distance when kinds differ.
        assert!(DepthKey::Z(0) > DepthKey::Distance(0.0));
        assert!(DepthKey::Z(-100) > DepthKey::Distance(1000.0));
        assert_eq!(
            DepthKey::Z(1).cmp(&DepthKey::Distance(1.0)),
            core::cmp::Ordering::Greater
        );
        assert_eq!(
            DepthKey::Distance(1.0).cmp(&DepthKey::Z(1)),
            core::cmp::Ordering::Less
        );
    }

    #[test]
    fn depthkey_partialord_matches_ord() {
        let a = DepthKey::Z(3);
        let b = DepthKey::Z(7);
        assert_eq!(a.partial_cmp(&b), Some(a.cmp(&b)));

        let c = DepthKey::Distance(0.5);
        let d = DepthKey::Distance(0.25);
        assert_eq!(c.partial_cmp(&d), Some(c.cmp(&d)));
    }

    #[test]
    fn depthkey_distance_nan_is_equal() {
        // NaN comparisons fall back to Equal by design to keep sort stable.
        let nan = f32::NAN;
        let a = DepthKey::Distance(nan);
        let b = DepthKey::Distance(0.0);
        assert_eq!(a.cmp(&b), core::cmp::Ordering::Equal);
        assert_eq!(b.cmp(&a), core::cmp::Ordering::Equal);
        assert_eq!(a.partial_cmp(&b), Some(core::cmp::Ordering::Equal));
    }
}
