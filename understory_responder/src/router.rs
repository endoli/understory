// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Router implementation.
//!
//! ## Overview
//!
//! Orders hits, reconstructs paths, and emits dispatch steps.
//! Produces a capture → target → bubble sequence for the selected target.
//!
//! ## Target Selection
//!
//! - Ranks candidates by [`DepthKey`](crate::types::DepthKey).
//! - In 2D, `Z` higher is nearer.
//! - In 3D, `Distance` lower is nearer.
//! - When kinds differ, `Z` outranks `Distance`.
//! - Picks exactly one winning candidate, the last after ordering.
//!
//! ## Ties and Policies
//!
//! - Equal‑depth ties are stable and the last wins.
//! - Use [`TieBreakPolicy`] to document intent or pre‑order your input when you have a stronger ordering.
//! - `set_scope` filters candidates before ranking.
//! - `capture` overrides selection entirely until released.
//!
//! ## See Also
//!
//! [`hover`](crate::hover) for hover transitions derived from the dispatch sequence.

use alloc::vec::Vec;

use crate::types::{
    Dispatch, Localizer, NoParent, ParentLookup, Phase, ResolvedHit, TieBreakPolicy, WidgetLookup,
};

/// Deterministic responder chain router.
///
/// ## Usage
///
/// - Construct with [`Router::new`] when callers always provide a full path in
///   [`crate::types::ResolvedHit`], or with [`Router::with_parent`] to enable
///   path reconstruction via a [`crate::types::ParentLookup`].
/// - Optionally configure policies:
///   - [`Router::set_default_tie_break`] to document equal‑depth intent.
///   - [`Router::set_scope`] to filter candidates (e.g., visibility/pickability).
///   - [`Router::capture`] to override target selection until released.
/// - Call [`Router::handle_with_hits`] each input event to select the winning
///   candidate and produce a capture → target → bubble dispatch sequence.
///
/// ## See Also
///
/// [`crate::hover`] for deriving hover enter/leave transitions from
/// the returned dispatch sequence.
pub struct Router<K, L: WidgetLookup<K>, P: ParentLookup<K> = NoParent> {
    pub(crate) lookup: L,
    pub(crate) parent: P,
    pub(crate) default_tie_break: TieBreakPolicy,
    pub(crate) scope: Option<fn(&K) -> bool>,
    pub(crate) focus: Option<K>,
    // Minimal capture for skeleton; production would be per-pointer id.
    pub(crate) capture: Option<K>,
    pub(crate) _phantom: core::marker::PhantomData<fn() -> K>,
}

impl<K: Copy + Eq, L: WidgetLookup<K>, P: ParentLookup<K>> core::fmt::Debug for Router<K, L, P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Router")
            .field("default_tie_break", &self.default_tie_break)
            .finish_non_exhaustive()
    }
}

impl<K: Copy + Eq, L: WidgetLookup<K>, P: ParentLookup<K> + Default> Router<K, L, P> {
    /// Create a router with default policies and a default parent lookup.
    pub fn new(lookup: L) -> Self {
        Self {
            lookup,
            parent: P::default(),
            default_tie_break: TieBreakPolicy::Newer,
            scope: None,
            focus: None,
            capture: None,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<K: Copy + Eq, L: WidgetLookup<K>, P: ParentLookup<K>> Router<K, L, P> {
    /// Create a router with an explicit parent lookup provider.
    pub fn with_parent(lookup: L, parent: P) -> Self {
        Self {
            lookup,
            parent,
            default_tie_break: TieBreakPolicy::Newer,
            scope: None,
            focus: None,
            capture: None,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Set the default tie-break policy when multiple hits share the same primary depth.
    pub fn set_default_tie_break(&mut self, p: TieBreakPolicy) {
        self.default_tie_break = p;
    }

    /// Set an optional scope filter; only nodes that satisfy the predicate are considered.
    pub fn set_scope(&mut self, scope: Option<fn(&K) -> bool>) {
        self.scope = scope;
    }

    /// Set the focused node (reserved for higher-level policies; currently not used in routing).
    pub fn set_focus(&mut self, node: Option<K>) {
        self.focus = node;
    }

    /// Set the captured node for pointer events (reserved; currently not used in routing).
    pub fn capture(&mut self, node: Option<K>) {
        self.capture = node;
    }

    /// Handle a pre-resolved sequence of hits and produce a propagation sequence.
    pub fn handle_with_hits<M>(
        &self,
        hits: &[ResolvedHit<K, M>],
    ) -> Vec<Dispatch<K, L::WidgetId, M>>
    where
        M: Clone,
    {
        // Capture override: when set, route to the captured node regardless of
        // current hit ranking. Use the hit's path if available, otherwise try to
        // reconstruct via parent lookup, and finally fall back to a singleton path.
        if let Some(cap) = self.capture {
            // Find any hit for the captured node (prefer the last if multiple exist).
            let cap_hit = hits.iter().rev().find(|h| h.node == cap);
            let (path, localizer, meta) = match cap_hit {
                Some(h) if h.path.is_some() => (
                    h.path.clone().unwrap(),
                    h.localizer.clone(),
                    Some(h.meta.clone()),
                ),
                Some(h) => (
                    Self::reconstruct_path(cap, &self.parent),
                    h.localizer.clone(),
                    Some(h.meta.clone()),
                ),
                None => (
                    Self::reconstruct_path(cap, &self.parent),
                    Localizer::default(),
                    None,
                ),
            };
            return self.emit_path(path, localizer, meta);
        }

        // Single-pass selection without allocation/sort. Equal-depth ties are
        // resolved by the tie-break policy, and if still equal we prefer the
        // last candidate (stable last-wins behavior).
        let mut best_idx: Option<usize> = None;
        for (i, h) in hits.iter().enumerate() {
            if let Some(f) = self.scope
                && !f(&h.node)
            {
                continue;
            }
            match best_idx {
                None => best_idx = Some(i),
                Some(j) => {
                    let a = &hits[j];
                    use core::cmp::Ordering::*;
                    let better = match a.depth_key.cmp(&h.depth_key) {
                        Less => true,     // h nearer than a
                        Greater => false, // a nearer than h
                        Equal => match self.tiebreak(&a.node, &h.node) {
                            Less => true,     // h preferred by policy
                            Greater => false, // a preferred by policy
                            Equal => true,    // stable last wins
                        },
                    };
                    if better {
                        best_idx = Some(i);
                    }
                }
            }
        }

        let Some(i) = best_idx else {
            return Vec::new();
        };
        let best = &hits[i];

        // Derive path if not provided.
        let path: Vec<K> = if let Some(p) = &best.path {
            p.clone()
        } else {
            Self::reconstruct_path(best.node, &self.parent)
        };

        self.emit_path(path, best.localizer.clone(), Some(best.meta.clone()))
    }

    fn make_dispatch<M: Clone>(
        &self,
        phase: Phase,
        node: K,
        localizer: Localizer,
        meta: Option<M>,
    ) -> Dispatch<K, L::WidgetId, M> {
        let widget = self.lookup.widget_of(&node);
        Dispatch {
            phase,
            node,
            widget,
            localizer,
            meta,
        }
    }

    fn reconstruct_path(target: K, parent_lookup: &impl ParentLookup<K>) -> Vec<K> {
        let mut out = Vec::new();
        let mut cur = target;
        // Collect to root; caller ensures acyclic ancestry.
        loop {
            out.push(cur);
            match parent_lookup.parent_of(&cur) {
                Some(p) => cur = p,
                None => break,
            }
        }
        out.reverse();
        out
    }

    fn emit_path<M: Clone>(
        &self,
        path: Vec<K>,
        localizer: Localizer,
        meta: Option<M>,
    ) -> Vec<Dispatch<K, L::WidgetId, M>> {
        let mut out = Vec::new();
        // Capture: root→target
        for &n in &path {
            out.push(self.make_dispatch(Phase::Capture, n, localizer.clone(), meta.clone()));
        }
        // Target
        let target = *path.last().unwrap();
        out.push(self.make_dispatch(Phase::Target, target, localizer.clone(), meta.clone()));
        // Bubble: target→root
        for &n in path.iter().rev() {
            out.push(self.make_dispatch(Phase::Bubble, n, localizer.clone(), meta.clone()));
        }
        out
    }

    fn tiebreak(&self, a: &K, b: &K) -> core::cmp::Ordering {
        use core::cmp::Ordering::*;
        match self.default_tie_break {
            TieBreakPolicy::Newer => {
                if Self::id_is_newer(a, b) {
                    Greater
                } else if Self::id_is_newer(b, a) {
                    Less
                } else {
                    Equal
                }
            }
            TieBreakPolicy::Older => {
                if Self::id_is_newer(b, a) {
                    Greater
                } else if Self::id_is_newer(a, b) {
                    Less
                } else {
                    Equal
                }
            }
            // Fallbacks when no inherent ordering is known for K.
            TieBreakPolicy::MinId => Self::id_cmp(a, b).reverse(),
            TieBreakPolicy::MaxId => Self::id_cmp(a, b),
        }
    }

    // Default id comparisons assume K is comparable by address or value if desired; we provide fallbacks.
    // TODO: Implement meaningful tie-breaking by allowing injected comparators or a trait.
    // Consider:
    // - `set_is_newer(fn: fn(&K, &K) -> bool)` and `set_id_cmp(fn: fn(&K, &K) -> Ordering)`;
    // - Or a generic `IdOrder<K>` trait with a default stable-last-wins implementation;
    // - Provide a NodeId-specific comparator in the box-tree adapter (generation, then slot).
    fn id_is_newer(_a: &K, _b: &K) -> bool {
        // Without generational ids in K, default to false (stable).
        false
    }

    // TODO: As above, use an injected comparator or trait to define ordering for K.
    // Until then, return Equal so stable last-wins applies after Equal depth.
    fn id_cmp(_a: &K, _b: &K) -> core::cmp::Ordering {
        core::cmp::Ordering::Equal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use alloc::vec;

    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
    struct Node(u32);

    struct Lookup;
    impl WidgetLookup<Node> for Lookup {
        type WidgetId = u32;
        fn widget_of(&self, node: &Node) -> Option<Self::WidgetId> {
            Some(node.0)
        }
    }

    // The rest of the tests mirror the ones in the prior lib.rs, ensuring
    // behavior parity after the module split.

    #[test]
    fn capture_overrides_selection_and_reconstructs_path() {
        struct Parents;
        impl ParentLookup<Node> for Parents {
            fn parent_of(&self, node: &Node) -> Option<Node> {
                match node.0 {
                    3 => Some(Node(2)),
                    2 => Some(Node(1)),
                    _ => None,
                }
            }
        }

        let lookup = Lookup;
        let mut router: Router<Node, Lookup, Parents> = Router::with_parent(lookup, Parents);
        router.capture(Some(Node(3)));
        // Competing hit with higher Z for a different node.
        let hits = vec![ResolvedHit {
            node: Node(9),
            path: Some(vec![Node(9)]),
            depth_key: DepthKey::Z(999),
            localizer: Localizer::default(),
            meta: (),
        }];
        let out = router.handle_with_hits::<()>(&hits);
        let phases: Vec<(Phase, u32)> = out.iter().map(|d| (d.phase, d.node.0)).collect();
        assert_eq!(
            phases,
            vec![
                (Phase::Capture, 1),
                (Phase::Capture, 2),
                (Phase::Capture, 3),
                (Phase::Target, 3),
                (Phase::Bubble, 3),
                (Phase::Bubble, 2),
                (Phase::Bubble, 1),
            ]
        );
    }

    #[test]
    fn capture_prefers_hit_metadata_when_available() {
        let lookup = Lookup;
        let mut router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        router.capture(Some(Node(7)));
        #[derive(Clone, Debug, PartialEq)]
        struct Meta(&'static str);
        let hits = vec![ResolvedHit {
            node: Node(7),
            path: Some(vec![Node(1), Node(7)]),
            depth_key: DepthKey::Z(0),
            localizer: Localizer::default(),
            meta: Meta("captured"),
        }];
        let out = router.handle_with_hits::<Meta>(&hits);
        let phases: Vec<(Phase, u32)> = out.iter().map(|d| (d.phase, d.node.0)).collect();
        assert_eq!(
            phases,
            vec![
                (Phase::Capture, 1),
                (Phase::Capture, 7),
                (Phase::Target, 7),
                (Phase::Bubble, 7),
                (Phase::Bubble, 1),
            ]
        );
        assert!(
            out.iter()
                .all(|d| matches!(d.meta.as_ref(), Some(Meta("captured"))))
        );
    }

    #[test]
    fn capture_bypasses_scope_filter() {
        let lookup = Lookup;
        let mut router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        router.capture(Some(Node(3))); // odd
        router.set_scope(Some(|n: &Node| (n.0 & 1) == 0)); // even only
        let hits = vec![ResolvedHit {
            node: Node(2),
            path: Some(vec![Node(2)]),
            depth_key: DepthKey::Z(100),
            localizer: Localizer::default(),
            meta: (),
        }];
        let out = router.handle_with_hits::<()>(&hits);
        let tgt = out
            .iter()
            .find(|d| matches!(d.phase, Phase::Target))
            .unwrap();
        assert_eq!(tgt.node.0, 3);
    }

    #[test]
    fn simple_path_dispatch() {
        let lookup = Lookup;
        let router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        let hits = vec![ResolvedHit {
            node: Node(3),
            path: Some(vec![Node(1), Node(2), Node(3)]),
            depth_key: DepthKey::Z(10),
            localizer: Localizer::default(),
            meta: (),
        }];
        let out = router.handle_with_hits::<()>(&hits);
        assert_eq!(out.len(), 7);
        assert!(matches!(out[0].phase, Phase::Capture));
        assert_eq!(out[0].node.0, 1);
        assert!(matches!(out[3].phase, Phase::Target));
        assert_eq!(out[3].node.0, 3);
        assert!(matches!(out[6].phase, Phase::Bubble));
        assert_eq!(out[6].node.0, 1);
    }

    #[test]
    fn scope_filter_selects_allowed_hit() {
        let lookup = Lookup;
        let mut router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        router.set_scope(Some(|n: &Node| (n.0 & 1) == 0));
        let hits = vec![
            ResolvedHit {
                node: Node(1),
                path: Some(vec![Node(1)]),
                depth_key: DepthKey::Z(100),
                localizer: Localizer::default(),
                meta: (),
            },
            ResolvedHit {
                node: Node(2),
                path: Some(vec![Node(2)]),
                depth_key: DepthKey::Z(50),
                localizer: Localizer::default(),
                meta: (),
            },
        ];
        let out = router.handle_with_hits::<()>(&hits);
        assert_eq!(
            out.iter()
                .filter(|d| matches!(d.phase, Phase::Target))
                .count(),
            1
        );
        let tgt = out
            .iter()
            .find(|d| matches!(d.phase, Phase::Target))
            .unwrap();
        assert_eq!(tgt.node.0, 2);
    }

    #[test]
    fn parent_of_reconstructs_path() {
        struct Parents;
        impl ParentLookup<Node> for Parents {
            fn parent_of(&self, node: &Node) -> Option<Node> {
                match node.0 {
                    3 => Some(Node(2)),
                    2 => Some(Node(1)),
                    _ => None,
                }
            }
        }

        let lookup = Lookup;
        let router: Router<Node, Lookup, Parents> = Router::with_parent(lookup, Parents);
        let hits = vec![ResolvedHit {
            node: Node(3),
            path: None,
            depth_key: DepthKey::Z(10),
            localizer: Localizer::default(),
            meta: (),
        }];
        let out = router.handle_with_hits::<()>(&hits);
        let phases: Vec<(Phase, u32)> = out.iter().map(|d| (d.phase, d.node.0)).collect();
        assert_eq!(
            phases,
            vec![
                (Phase::Capture, 1),
                (Phase::Capture, 2),
                (Phase::Capture, 3),
                (Phase::Target, 3),
                (Phase::Bubble, 3),
                (Phase::Bubble, 2),
                (Phase::Bubble, 1),
            ]
        );
    }

    #[test]
    fn mixed_depthkey_z_beats_distance() {
        let lookup = Lookup;
        let router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        let hits = vec![
            ResolvedHit {
                node: Node(10),
                path: Some(vec![Node(10)]),
                depth_key: DepthKey::Distance(0.1),
                localizer: Localizer::default(),
                meta: (),
            },
            ResolvedHit {
                node: Node(20),
                path: Some(vec![Node(20)]),
                depth_key: DepthKey::Z(0),
                localizer: Localizer::default(),
                meta: (),
            },
        ];
        let out = router.handle_with_hits::<()>(&hits);
        let tgt = out
            .iter()
            .find(|d| matches!(d.phase, Phase::Target))
            .unwrap();
        assert_eq!(tgt.node.0, 20);
    }

    #[test]
    fn tie_break_is_stable_last_wins_on_equal_depth() {
        let lookup = Lookup;
        let router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        let hits = vec![
            ResolvedHit {
                node: Node(1),
                path: Some(vec![Node(1)]),
                depth_key: DepthKey::Z(5),
                localizer: Localizer::default(),
                meta: (),
            },
            ResolvedHit {
                node: Node(2),
                path: Some(vec![Node(2)]),
                depth_key: DepthKey::Z(5),
                localizer: Localizer::default(),
                meta: (),
            },
        ];
        let out = router.handle_with_hits::<()>(&hits);
        let tgt = out
            .iter()
            .find(|d| matches!(d.phase, Phase::Target))
            .unwrap();
        assert_eq!(tgt.node.0, 2);
    }

    #[test]
    fn meta_and_localizer_passthrough() {
        #[derive(Clone, Debug, PartialEq)]
        struct Meta(&'static str);
        let lookup = Lookup;
        let router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        let hits = vec![ResolvedHit {
            node: Node(7),
            path: Some(vec![Node(7)]),
            depth_key: DepthKey::Z(1),
            localizer: Localizer::default(),
            meta: Meta("hello"),
        }];
        let out = router.handle_with_hits::<Meta>(&hits);
        assert!(out.iter().all(|d| d.meta.as_ref().is_some()));
        assert!(out.iter().all(|d| d.localizer == Localizer::default()));
        assert!(
            out.iter()
                .all(|d| matches!(d.meta.as_ref(), Some(Meta("hello"))))
        );
    }

    #[test]
    fn widget_id_is_mapped_for_each_dispatch() {
        let lookup = Lookup;
        let router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        let hits = vec![ResolvedHit {
            node: Node(42),
            path: Some(vec![Node(1), Node(42)]),
            depth_key: DepthKey::Z(10),
            localizer: Localizer::default(),
            meta: (),
        }];
        let out = router.handle_with_hits::<()>(&hits);
        assert!(!out.is_empty());
        for d in &out {
            assert_eq!(d.widget, Some(d.node.0));
        }
    }

    #[test]
    fn same_node_higher_z_wins() {
        let lookup = Lookup;
        let router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        let hits = vec![
            ResolvedHit {
                node: Node(5),
                path: Some(vec![Node(5)]),
                depth_key: DepthKey::Z(1),
                localizer: Localizer::default(),
                meta: (),
            },
            ResolvedHit {
                node: Node(5),
                path: Some(vec![Node(5)]),
                depth_key: DepthKey::Z(10),
                localizer: Localizer::default(),
                meta: (),
            },
        ];
        let out = router.handle_with_hits::<()>(&hits);
        let tgt = out
            .iter()
            .find(|d| matches!(d.phase, Phase::Target))
            .unwrap();
        assert_eq!(tgt.node.0, 5);
        assert_eq!(
            out.iter()
                .filter(|d| matches!(d.phase, Phase::Target))
                .count(),
            1
        );
    }

    #[test]
    fn capture_can_be_released() {
        let lookup = Lookup;
        let mut router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        router.capture(Some(Node(1)));
        router.capture(None);
        let hits = vec![
            ResolvedHit {
                node: Node(2),
                path: Some(vec![Node(2)]),
                depth_key: DepthKey::Z(1),
                localizer: Localizer::default(),
                meta: (),
            },
            ResolvedHit {
                node: Node(3),
                path: Some(vec![Node(3)]),
                depth_key: DepthKey::Z(10),
                localizer: Localizer::default(),
                meta: (),
            },
        ];
        let out = router.handle_with_hits::<()>(&hits);
        let tgt = out
            .iter()
            .find(|d| matches!(d.phase, Phase::Target))
            .unwrap();
        assert_eq!(tgt.node.0, 3);
    }

    #[test]
    fn capture_prefers_last_matching_hit() {
        let lookup = Lookup;
        let mut router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        router.capture(Some(Node(7)));
        #[derive(Clone, Debug, PartialEq)]
        struct Meta(&'static str);
        let hits = vec![
            ResolvedHit {
                node: Node(7),
                path: Some(vec![Node(7)]),
                depth_key: DepthKey::Z(1),
                localizer: Localizer::default(),
                meta: Meta("first"),
            },
            ResolvedHit {
                node: Node(7),
                path: Some(vec![Node(1), Node(7)]),
                depth_key: DepthKey::Z(2),
                localizer: Localizer::default(),
                meta: Meta("second"),
            },
        ];
        let out = router.handle_with_hits::<Meta>(&hits);
        let phases: Vec<(Phase, u32)> = out.iter().map(|d| (d.phase, d.node.0)).collect();
        assert_eq!(
            phases,
            vec![
                (Phase::Capture, 1),
                (Phase::Capture, 7),
                (Phase::Target, 7),
                (Phase::Bubble, 7),
                (Phase::Bubble, 1),
            ]
        );
        assert!(
            out.iter()
                .all(|d| matches!(d.meta.as_ref(), Some(Meta("second"))))
        );
    }

    #[test]
    fn distance_ordering_and_tie_break() {
        let lookup = Lookup;
        let router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        let hits = vec![
            ResolvedHit {
                node: Node(1),
                path: Some(vec![Node(1)]),
                depth_key: DepthKey::Distance(0.25),
                localizer: Localizer::default(),
                meta: (),
            },
            ResolvedHit {
                node: Node(2),
                path: Some(vec![Node(2)]),
                depth_key: DepthKey::Distance(0.25),
                localizer: Localizer::default(),
                meta: (),
            },
            ResolvedHit {
                node: Node(3),
                path: Some(vec![Node(3)]),
                depth_key: DepthKey::Distance(0.10),
                localizer: Localizer::default(),
                meta: (),
            },
        ];
        let out = router.handle_with_hits::<()>(&hits);
        let tgt = out
            .iter()
            .find(|d| matches!(d.phase, Phase::Target))
            .unwrap();
        assert_eq!(tgt.node.0, 3);
        let out2 = router.handle_with_hits::<()>(&hits[..2]);
        let tgt2 = out2
            .iter()
            .find(|d| matches!(d.phase, Phase::Target))
            .unwrap();
        assert_eq!(tgt2.node.0, 2);
    }

    #[test]
    fn fallback_singleton_path_without_parent_or_path() {
        let lookup = Lookup;
        let router: Router<Node, Lookup, NoParent> = Router::new(lookup);
        let hits = vec![ResolvedHit {
            node: Node(9),
            path: None,
            depth_key: DepthKey::Z(0),
            localizer: Localizer::default(),
            meta: (),
        }];
        let out = router.handle_with_hits::<()>(&hits);
        let phases: Vec<(Phase, u32)> = out.iter().map(|d| (d.phase, d.node.0)).collect();
        assert_eq!(
            phases,
            vec![(Phase::Capture, 9), (Phase::Target, 9), (Phase::Bubble, 9),]
        );
    }
}
