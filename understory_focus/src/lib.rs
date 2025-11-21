// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Understory Focus: focus navigation primitives.
//!
//! This crate models focus navigation as a combination of:
//! - **Navigation intents** ([`Navigation`]) such as [`Navigation::Next`], [`Navigation::Prev`],
//!   or arrow directions.
//! - **Per-node focus properties** ([`FocusProps`]) such as enabled state, explicit order, and
//!   optional grouping or policy hints.
//! - A **spatial view of candidates** ([`FocusEntry`] / [`FocusSpace`]) that describes where
//!   focusable nodes live in a chosen 2D coordinate space (for example, a surface/world space
//!   or a container-local space).
//! - Pluggable **policies** ([`FocusPolicy`]) that select the next focused node given an
//!   origin, a direction, and a read-only view of focusable candidates.
//!
//! ## Minimal example
//!
//! A simple focus loop over two buttons laid out left-to-right:
//!
//! ```rust
//! use kurbo::Rect;
//! use understory_focus::{
//!     DefaultPolicy, FocusEntry, FocusPolicy, FocusSpace, Navigation, WrapMode,
//! };
//!
//! let entries = vec![
//!     FocusEntry {
//!         id: 1_u32,
//!         rect: Rect::new(0.0, 0.0, 10.0, 10.0),
//!         order: None,
//!         group: None,
//!         enabled: true,
//!         scope_depth: 0,
//!     },
//!     FocusEntry {
//!         id: 2_u32,
//!         rect: Rect::new(20.0, 0.0, 30.0, 10.0),
//!         order: None,
//!         group: None,
//!         enabled: true,
//!         scope_depth: 0,
//!     },
//! ];
//!
//! let space = FocusSpace { nodes: &entries };
//! let policy = DefaultPolicy { wrap: WrapMode::Scope };
//!
//! // Tab moves from the first button to the second…
//! assert_eq!(policy.next(1, Navigation::Next, &space), Some(2));
//! // …and wraps back to the first.
//! assert_eq!(policy.next(2, Navigation::Next, &space), Some(1));
//! ```
//!
//! ## Patterns: groups and policy hints
//!
//! [`FocusSymbol`] is a small, copyable handle you can use to describe
//! higher-level focus intent without baking policy into the geometry layer.
//!
//! - Use [`FocusProps::group`] to keep navigation within a logical cluster
//!   (for example, a grid, toolbar, or inspector section) before jumping
//!   elsewhere.
//! - Use [`FocusProps::policy_hint`] to mark containers that should use a
//!   specific traversal style (for example, reading-order vs. grid-like).
//!
//! ```rust
//! use understory_focus::{FocusProps, FocusSymbol};
//!
//! const GROUP_GRID: FocusSymbol = FocusSymbol(1);
//! const HINT_GRID_POLICY: FocusSymbol = FocusSymbol(10);
//!
//! // A cell inside a grid: share GROUP_GRID so a policy can keep arrows
//! // within the grid until the user explicitly exits the scope.
//! let cell_props = FocusProps {
//!     group: Some(GROUP_GRID),
//!     ..FocusProps::default()
//! };
//!
//! // The grid container itself: mark it with a policy hint so the host
//! // can choose an appropriate FocusPolicy implementation.
//! let grid_props = FocusProps {
//!     policy_hint: Some(HINT_GRID_POLICY),
//!     ..FocusProps::default()
//! };
//! ```
//!
//! The core types are generic over the node identifier `K`, so callers can use any small,
//! copyable handle (for example `understory_box_tree::NodeId` when used with the box tree,
//! or an application-specific id).
//! Geometry is expressed in terms of [`kurbo::Rect`], which matches the rest of the Understory
//! crates and allows directional policies to reason about spatial layout. A [`FocusSpace`]
//! should use a consistent coordinate space for all of its entries (for example, the world
//! space of a box tree or the local space of a focus scope).
//!
//! ## Features
//!
//! - `std` (default): enables `std` support for dependencies such as `kurbo`.
//! - `libm`: enables `no_std` + `alloc` builds that rely on `libm` for floating-point math;
//!   typically used when integrating into embedded or `no_std` environments.
//! - `box_tree_adapter`: enables the [`adapters::box_tree`] module and pulls in
//!   `understory_box_tree` and `understory_index` so you can build a [`FocusSpace`] directly
//!   from an `understory_box_tree::Tree`.
//!
//! This crate is `no_std` and uses `alloc`.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::cmp::Ordering;

use kurbo::Rect;

#[cfg(feature = "box_tree_adapter")]
pub mod adapters;

/// Direction of focus navigation.
///
/// These values represent high-level navigation intents such as Tab/Shift+Tab and
/// arrow-key movement. Concrete policies interpret them according to their own rules.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Navigation {
    /// Move to the next candidate in the container's forward order (for example, Tab).
    Next,
    /// Move to the previous candidate in the container's backward order (for example, Shift+Tab).
    Prev,
    /// Move in the up direction relative to the current focus.
    Up,
    /// Move in the down direction relative to the current focus.
    Down,
    /// Move in the left direction relative to the current focus.
    Left,
    /// Move in the right direction relative to the current focus.
    Right,
    /// Enter a child scope (for example, when Tab enters a composite widget or grid).
    EnterScope,
    /// Exit the current scope (for example, Escape returning to the parent scope).
    ExitScope,
}

/// Symbol-like identifier used for grouping and policy hints.
///
/// This is a small, copyable handle that callers can use to partition focusable
/// elements into groups or to select a traversal policy. The host is responsible
/// for managing the meaning and lifecycle of individual symbols (for example via
/// an interned string table, enum-to-symbol mapping, or static constants).
///
/// Typical uses:
/// - As [`FocusProps::group`] to keep navigation within a logical cluster (for example,
///   a grid or toolbar) before jumping elsewhere.
/// - As [`FocusProps::policy_hint`] to indicate that a container prefers a specific
///   traversal style (for example, reading-order vs. grid-like movement).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FocusSymbol(pub u64);

/// Per-node focus properties provided by the host.
///
/// These properties are layered on top of the underlying scene or box tree:
/// they do not affect spatial indexing or hit testing directly, but are
/// consulted by focus traversal policies.
#[derive(Clone, Debug)]
pub struct FocusProps {
    /// Whether this node can be targeted by focus.
    ///
    /// Disabled nodes are skipped during focus traversal but may remain in the
    /// spatial index for hit testing or layout purposes.
    pub enabled: bool,
    /// Optional explicit ordering key.
    ///
    /// When present, policies that support ordered traversal may sort candidates
    /// by this value in addition to their geometric ordering.
    pub order: Option<i32>,
    /// Optional group identifier.
    ///
    /// Groups allow policies to partition focusable elements (for example, to
    /// keep navigation within a grid or logical cluster).
    pub group: Option<FocusSymbol>,
    /// Whether this node should be considered as an initial focus candidate
    /// when its containing scope is first activated.
    pub autofocus: bool,
    /// Optional policy hint.
    ///
    /// Callers can use this to indicate a preferred traversal policy for a
    /// container or subtree (for example, "reading order", "grid", or "directional").
    /// Interpretation is left to the host and the active policy implementation.
    pub policy_hint: Option<FocusSymbol>,
}

impl Default for FocusProps {
    fn default() -> Self {
        Self {
            enabled: true,
            order: None,
            group: None,
            autofocus: false,
            policy_hint: None,
        }
    }
}

/// A single focusable candidate within a [`FocusSpace`].
///
/// `FocusEntry` bundles the node identifier with its spatial bounds and
/// effective focus-related properties. Policies operate over collections of
/// these entries.
#[derive(Clone, Debug)]
pub struct FocusEntry<K> {
    /// Identifier for this focusable node.
    pub id: K,
    /// Bounds in the coordinate space of the surrounding [`FocusSpace`].
    ///
    /// Callers are free to choose the coordinate space (for example, the
    /// surface/world space of a box tree or a container-local space), but all
    /// entries within a given [`FocusSpace`] should use the same space so that
    /// directional policies can compare positions meaningfully.
    pub rect: Rect,
    /// Optional explicit ordering key.
    pub order: Option<i32>,
    /// Optional group identifier for partitioning.
    pub group: Option<FocusSymbol>,
    /// Whether this node is enabled for focus.
    pub enabled: bool,
    /// Depth of this node within the current focus scope.
    ///
    /// Policies can use this to refine ordering (for example, preferring
    /// shallower nodes when multiple candidates overlap).
    pub scope_depth: u8,
}

/// A read-only view of focusable candidates.
///
/// A `FocusSpace` is typically built by a higher-level adapter (for example,
/// from an `understory_box_tree::Tree` plus an application-provided map of
/// `FocusProps`). Policies should treat it as an immutable snapshot.
#[derive(Clone, Debug)]
pub struct FocusSpace<'a, K> {
    /// Focusable candidates visible to the current scope and policy.
    pub nodes: &'a [FocusEntry<K>],
}

/// Wrap mode configuration for focus traversal.
///
/// Policies may consult this to decide whether navigation should wrap around
/// within a scope or stop at the edges.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum WrapMode {
    /// Do not wrap; reaching the end of the sequence yields no next candidate.
    Never,
    /// Wrap within the current focus scope.
    Scope,
    /// Wrap globally across all visible scopes (policy-defined).
    Global,
}

/// Trait for focus traversal policies.
///
/// A policy receives a navigation intent, the current origin node, and a
/// read-only view of focusable candidates, and returns the next focused node
/// if any. Implementations are free to use spatial reasoning, ordering keys,
/// grouping, or scope depth as needed.
pub trait FocusPolicy<K>
where
    K: Copy + Eq,
{
    /// Compute the next focus target given an origin, navigation intent, and focus space.
    fn next(&self, origin: K, direction: Navigation, space: &FocusSpace<'_, K>) -> Option<K>;
}

/// Default focus traversal policy placeholder.
///
/// This type captures common configuration such as wrap mode; its behavior is
/// intentionally minimal for now and may evolve as the focus system matures.
#[derive(Copy, Clone, Debug)]
pub struct DefaultPolicy {
    /// Wrap behavior when traversing focusable candidates.
    pub wrap: WrapMode,
}

impl Default for DefaultPolicy {
    fn default() -> Self {
        Self {
            wrap: WrapMode::Scope,
        }
    }
}

impl<K> FocusPolicy<K> for DefaultPolicy
where
    K: Copy + Eq,
{
    fn next(&self, origin: K, direction: Navigation, space: &FocusSpace<'_, K>) -> Option<K> {
        match direction {
            Navigation::Next => next_linear(origin, space, self.wrap, Step::Forward),
            Navigation::Prev => next_linear(origin, space, self.wrap, Step::Backward),
            Navigation::Up | Navigation::Down | Navigation::Left | Navigation::Right => {
                next_directional(origin, direction, space).or_else(|| {
                    // Fallback to linear traversal if no directional candidate is found.
                    let step = match direction {
                        Navigation::Up | Navigation::Left => Step::Backward,
                        Navigation::Down | Navigation::Right => Step::Forward,
                        _ => Step::Forward,
                    };
                    next_linear(origin, space, self.wrap, step)
                })
            }
            // Scope enter/exit are higher-level intents; the default policy does
            // not change focus in response to them.
            Navigation::EnterScope | Navigation::ExitScope => None,
        }
    }
}

#[derive(Copy, Clone)]
enum Step {
    Forward,
    Backward,
}

fn next_linear<K>(origin: K, space: &FocusSpace<'_, K>, wrap: WrapMode, step: Step) -> Option<K>
where
    K: Copy + Eq,
{
    let nodes = space.nodes;
    if nodes.is_empty() {
        return None;
    }

    // Collect enabled candidates and sort them by explicit order and
    // reading order (y, then x).
    let mut indices: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.enabled.then_some(i))
        .collect();
    if indices.is_empty() {
        return None;
    }

    indices.sort_by(|&ia, &ib| compare_linear(&nodes[ia], &nodes[ib]));

    // Locate the origin within the sorted candidates, if present.
    let origin_pos = indices.iter().position(|&i| nodes[i].id == origin);

    match step {
        Step::Forward => match origin_pos {
            Some(pos) => {
                if pos + 1 < indices.len() {
                    Some(nodes[indices[pos + 1]].id)
                } else if matches!(wrap, WrapMode::Scope | WrapMode::Global) {
                    Some(nodes[indices[0]].id)
                } else {
                    None
                }
            }
            None => Some(nodes[indices[0]].id),
        },
        Step::Backward => match origin_pos {
            Some(pos) => {
                if pos > 0 {
                    Some(nodes[indices[pos - 1]].id)
                } else if matches!(wrap, WrapMode::Scope | WrapMode::Global) {
                    Some(nodes[indices[indices.len() - 1]].id)
                } else {
                    None
                }
            }
            None => Some(nodes[indices[indices.len() - 1]].id),
        },
    }
}

fn compare_linear<K>(a: &FocusEntry<K>, b: &FocusEntry<K>) -> Ordering {
    // First, honor explicit order when present.
    match (a.order, b.order) {
        (Some(ao), Some(bo)) => ao
            .cmp(&bo)
            .then_with(|| compare_rect_reading(&a.rect, &b.rect)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => compare_rect_reading(&a.rect, &b.rect),
    }
}

fn compare_rect_reading(a: &Rect, b: &Rect) -> Ordering {
    const EPS: f64 = 1e-6;
    let ay = a.y0;
    let by = b.y0;
    if (ay - by).abs() > EPS {
        return ay.partial_cmp(&by).unwrap_or(Ordering::Equal);
    }
    let ax = a.x0;
    let bx = b.x0;
    ax.partial_cmp(&bx).unwrap_or(Ordering::Equal)
}

fn next_directional<K>(origin: K, direction: Navigation, space: &FocusSpace<'_, K>) -> Option<K>
where
    K: Copy + Eq,
{
    let nodes = space.nodes;
    if nodes.is_empty() {
        return None;
    }

    let origin_entry = nodes.iter().find(|e| e.id == origin && e.enabled)?;
    let oc = origin_entry.rect.center();

    let mut best_idx: Option<usize> = None;
    let mut best_score: f64 = f64::INFINITY;

    for (i, candidate) in nodes.iter().enumerate() {
        if !candidate.enabled || candidate.id == origin {
            continue;
        }
        let cc = candidate.rect.center();
        let dx = cc.x - oc.x;
        let dy = cc.y - oc.y;

        let (primary, secondary, forward_sign) = match direction {
            Navigation::Right => (dx, dy, 1.0),
            Navigation::Left => (dx, dy, -1.0),
            Navigation::Down => (dy, dx, 1.0),
            Navigation::Up => (dy, dx, -1.0),
            _ => continue,
        };

        // Restrict to the forward hemiplane.
        if forward_sign * primary <= 0.0 {
            continue;
        }

        let abs_primary = primary.abs();
        let abs_secondary = secondary.abs();
        // Favor closer candidates and penalize off-axis motion.
        let score = abs_primary + 4.0 * abs_secondary;

        if !score.is_finite() {
            continue;
        }

        if score < best_score {
            best_score = score;
            best_idx = Some(i);
        }
    }

    best_idx.map(|i| nodes[i].id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn linear_next_prev_with_wrap() {
        let entries = vec![
            FocusEntry {
                id: 1_u32,
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
            FocusEntry {
                id: 2_u32,
                rect: Rect::new(20.0, 0.0, 30.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
        ];
        let space = FocusSpace { nodes: &entries };
        let policy = DefaultPolicy {
            wrap: WrapMode::Scope,
        };

        assert_eq!(policy.next(1, Navigation::Next, &space), Some(2));
        assert_eq!(policy.next(2, Navigation::Next, &space), Some(1));
        assert_eq!(policy.next(1, Navigation::Prev, &space), Some(2));
    }

    #[test]
    fn directional_prefers_forward_candidates() {
        let entries = vec![
            FocusEntry {
                id: 1_u32,
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
            // Right of origin.
            FocusEntry {
                id: 2_u32,
                rect: Rect::new(20.0, 0.0, 30.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
            // Left of origin.
            FocusEntry {
                id: 3_u32,
                rect: Rect::new(-30.0, 0.0, -20.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
        ];
        let space = FocusSpace { nodes: &entries };
        let policy = DefaultPolicy::default();

        assert_eq!(policy.next(1, Navigation::Right, &space), Some(2));
        assert_eq!(policy.next(1, Navigation::Left, &space), Some(3));
    }

    #[test]
    fn linear_respects_explicit_order() {
        let entries = vec![
            FocusEntry {
                id: 1_u32,
                rect: Rect::new(20.0, 0.0, 30.0, 10.0),
                order: Some(2),
                group: None,
                enabled: true,
                scope_depth: 0,
            },
            FocusEntry {
                id: 2_u32,
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                order: Some(1),
                group: None,
                enabled: true,
                scope_depth: 0,
            },
        ];
        let space = FocusSpace { nodes: &entries };
        let policy = DefaultPolicy {
            wrap: WrapMode::Scope,
        };

        // Despite the reading-order geometry, explicit order should win.
        assert_eq!(policy.next(2, Navigation::Next, &space), Some(1));
        assert_eq!(policy.next(1, Navigation::Prev, &space), Some(2));
    }

    #[test]
    fn linear_skips_disabled_entries() {
        let entries = vec![
            FocusEntry {
                id: 1_u32,
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
            FocusEntry {
                id: 2_u32,
                rect: Rect::new(20.0, 0.0, 30.0, 10.0),
                order: None,
                group: None,
                enabled: false,
                scope_depth: 0,
            },
            FocusEntry {
                id: 3_u32,
                rect: Rect::new(40.0, 0.0, 50.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
        ];
        let space = FocusSpace { nodes: &entries };
        let policy = DefaultPolicy {
            wrap: WrapMode::Scope,
        };

        // Next from 1 should skip disabled 2 and go to 3.
        assert_eq!(policy.next(1, Navigation::Next, &space), Some(3));
        // Prev from 3 should skip disabled 2 and go back to 1.
        assert_eq!(policy.next(3, Navigation::Prev, &space), Some(1));
    }

    #[test]
    fn linear_no_wrap_stops_at_edges() {
        let entries = vec![
            FocusEntry {
                id: 1_u32,
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
            FocusEntry {
                id: 2_u32,
                rect: Rect::new(20.0, 0.0, 30.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
        ];
        let space = FocusSpace { nodes: &entries };
        let policy = DefaultPolicy {
            wrap: WrapMode::Never,
        };

        assert_eq!(policy.next(2, Navigation::Next, &space), None);
        assert_eq!(policy.next(1, Navigation::Prev, &space), None);
    }

    #[test]
    fn directional_skips_disabled_and_self() {
        let entries = vec![
            FocusEntry {
                id: 1_u32,
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
            // Right but disabled.
            FocusEntry {
                id: 2_u32,
                rect: Rect::new(20.0, 0.0, 30.0, 10.0),
                order: None,
                group: None,
                enabled: false,
                scope_depth: 0,
            },
            // Further right and enabled.
            FocusEntry {
                id: 3_u32,
                rect: Rect::new(40.0, 0.0, 50.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
        ];
        let space = FocusSpace { nodes: &entries };
        let policy = DefaultPolicy::default();

        // Right from 1 should skip disabled 2 and pick 3.
        assert_eq!(policy.next(1, Navigation::Right, &space), Some(3));
        // Left from 1 has no directional candidate, so it falls back to
        // linear backward traversal with wrap, which selects 3.
        assert_eq!(policy.next(1, Navigation::Left, &space), Some(3));
    }

    #[test]
    fn directional_falls_back_to_linear_when_blocked() {
        let entries = vec![
            FocusEntry {
                id: 1_u32,
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
            // All candidates lie to the left of the origin.
            FocusEntry {
                id: 2_u32,
                rect: Rect::new(-30.0, 0.0, -20.0, 10.0),
                order: None,
                group: None,
                enabled: true,
                scope_depth: 0,
            },
        ];
        let space = FocusSpace { nodes: &entries };
        let policy = DefaultPolicy {
            wrap: WrapMode::Scope,
        };

        // Right finds no directional candidate, so it should fall back to
        // linear "next", which wraps to id 2 in this two-element space.
        assert_eq!(policy.next(1, Navigation::Right, &space), Some(2));
    }
}
