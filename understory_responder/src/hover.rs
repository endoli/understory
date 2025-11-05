// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Hover state helper: compute enter/leave transitions from path changes.
//!
//! ## Usage
//!
//! 1) Run the router to produce a dispatch sequence for a pointer move or similar.
//! 2) Extract the root→target path from the dispatch with [`path_from_dispatch`].
//! 3) Call [`HoverState::update_path`] with that path to get `Enter(..)` / `Leave(..)` transitions.
//!
//! ## Minimal example
//!
//! ```
//! use understory_responder::hover::{HoverState, HoverEvent};
//! let mut h: HoverState<u32> = HoverState::new();
//! assert_eq!(h.update_path(&[1, 2]), vec![HoverEvent::Enter(1), HoverEvent::Enter(2)]);
//! assert_eq!(h.update_path(&[1, 3]), vec![HoverEvent::Leave(2), HoverEvent::Enter(3)]);
//! ```
//!
//! ## Example (sketch):
//!
//! ```no_run
//! use understory_responder::hover::{HoverState, path_from_dispatch};
//! use understory_responder::types::{ResolvedHit, DepthKey, Localizer};
//! # use understory_responder::router::Router;
//! # use understory_responder::types::{ParentLookup, WidgetLookup};
//! #
//! # // Minimal types for demonstration
//! # #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
//! # struct Node(u32);
//! #
//! # struct Lookup;
//! # impl WidgetLookup<Node> for Lookup {
//! #     type WidgetId = u32;
//! #     fn widget_of(&self, n: &Node) -> Option<u32> { Some(n.0) }
//! # }
//! #
//! # struct Parents;
//! # impl ParentLookup<Node> for Parents {
//! #     fn parent_of(&self, _n: &Node) -> Option<Node> { None }
//! # }
//! #
//! # let router: Router<Node, Lookup, Parents> = Router::with_parent(Lookup, Parents);
//! # let hits = vec![ResolvedHit {
//! #     node: Node(1),
//! #     path: None,
//! #     depth_key: DepthKey::Z(10),
//! #     localizer: Localizer::default(),
//! #     meta: (),
//! # }];
//! # let seq = router.handle_with_hits::<()>(&hits);
//! #
//! // Derive the root→target path from the dispatch sequence.
//! let path = path_from_dispatch(&seq);
//!
//! // Compute hover transitions from the previous path to the new path.
//! let mut hover = HoverState::new();
//! let transitions = hover.update_path(&path);
//! # let _ = transitions;
//! ```

use alloc::vec::Vec;

use crate::types::{Dispatch, Phase};

/// A simple hover state machine over root→target paths.
///
/// Tracks the current hovered path (root→target) and, when updated with a new
/// path, computes the minimal sequence of leave and enter transitions to move
/// from the old state to the new state.
///
/// Ordering semantics:
/// - Leave events are emitted from inner-most to outer-most.
/// - Enter events are emitted from outer-most to inner-most.
///
/// This mirrors common UI expectations for hover transitions as the pointer
/// moves across siblings and their ancestors.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HoverState<K: Copy + Eq> {
    current: Vec<K>,
}

/// A hover transition event.
///
/// Returned by [`HoverState::update_path`]. Use
/// [`path_from_dispatch`] to derive a root→target path from a router
/// dispatch sequence, then pass that path into [`HoverState::update_path`]
/// to obtain `Enter(..)` / `Leave(..)` transitions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HoverEvent<K> {
    /// Pointer enters the given node (in order from outer→inner).
    Enter(K),
    /// Pointer leaves the given node (in order from inner→outer).
    Leave(K),
}

impl<K: Copy + Eq> HoverState<K> {
    /// Create an empty hover state.
    pub fn new() -> Self {
        Self {
            current: Vec::new(),
        }
    }

    /// Return the current root→target path (if any).
    pub fn current_path(&self) -> &[K] {
        &self.current
    }

    /// Clear the current hover path, returning the corresponding leave events
    /// from inner-most to outer-most.
    pub fn clear(&mut self) -> Vec<HoverEvent<K>> {
        let mut out = Vec::new();
        for &k in self.current.iter().rev() {
            out.push(HoverEvent::Leave(k));
        }
        self.current.clear();
        out
    }

    /// Update the hover path and return the enter/leave events required to
    /// transition from the previous path to `new_path`.
    ///
    /// Leaves are emitted from inner-most to outer-most, then enters from
    /// outer-most to inner-most (matching common UI expectations).
    pub fn update_path(&mut self, new_path: &[K]) -> Vec<HoverEvent<K>> {
        // Compute the length of the common prefix (the shared ancestry)
        // which corresponds to the lowest common ancestor (LCA) depth.
        let mut lca = 0;
        while lca < self.current.len() && lca < new_path.len() && self.current[lca] == new_path[lca]
        {
            lca += 1;
        }

        let mut out = Vec::new();
        // Leaves: from old tail back to the LCA (exclusive), inner→outer.
        for &k in self.current[lca..].iter().rev() {
            out.push(HoverEvent::Leave(k));
        }

        // Enters: from LCA down to new tail, outer→inner.
        for &k in &new_path[lca..] {
            out.push(HoverEvent::Enter(k));
        }

        self.current.clear();
        self.current.extend_from_slice(new_path);
        out
    }
}

/// Extract a root→target path from a router dispatch sequence.
///
/// Assumes the sequence begins with all [`Capture`](crate::types::Phase::Capture)
/// events for the path, followed by the [`Target`](crate::types::Phase::Target)
/// and [`Bubble`](crate::types::Phase::Bubble) phases (as produced by the
/// router in this crate). Pass the returned path to
/// [`HoverState::update_path`] to compute hover transitions.
pub fn path_from_dispatch<K: Copy, W, M>(seq: &[Dispatch<K, W, M>]) -> Vec<K> {
    let mut path = Vec::new();
    for d in seq {
        match d.phase {
            Phase::Capture => path.push(d.node),
            Phase::Target | Phase::Bubble => break,
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    // Fresh path: expect outer→inner enters.
    #[test]
    fn hover_enter_on_fresh_path() {
        let mut h: HoverState<u32> = HoverState::new();
        let ev = h.update_path(&[1, 2, 3]);
        assert_eq!(
            ev,
            vec![
                HoverEvent::Enter(1),
                HoverEvent::Enter(2),
                HoverEvent::Enter(3)
            ]
        );
        assert_eq!(h.current_path(), &[1, 2, 3]);
    }

    // Clearing path: expect inner→outer leaves.
    #[test]
    fn hover_leave_to_empty() {
        let mut h: HoverState<u32> = HoverState::new();
        let _ = h.update_path(&[1, 2]);
        let ev = h.clear();
        assert_eq!(ev, vec![HoverEvent::Leave(2), HoverEvent::Leave(1)]);
        assert!(h.current_path().is_empty());
    }

    // Branch change with shallow LCA (depth 1): leave inner tail, then enter new branch.
    #[test]
    fn hover_branch_change() {
        let mut h: HoverState<u32> = HoverState::new();
        let _ = h.update_path(&[1, 2, 3]);
        let ev = h.update_path(&[1, 4]);
        assert_eq!(
            ev,
            vec![
                HoverEvent::Leave(3),
                HoverEvent::Leave(2),
                HoverEvent::Enter(4)
            ]
        );
        assert_eq!(h.current_path(), &[1, 4]);
    }

    // Disjoint paths: no common prefix — leave entire old path, enter entire new path.
    #[test]
    fn hover_disjoint_paths() {
        let mut h: HoverState<u32> = HoverState::new();
        let _ = h.update_path(&[1, 2, 3]);
        // No common prefix between old and new.
        let ev = h.update_path(&[4, 5]);
        assert_eq!(
            ev,
            vec![
                HoverEvent::Leave(3),
                HoverEvent::Leave(2),
                HoverEvent::Leave(1),
                HoverEvent::Enter(4),
                HoverEvent::Enter(5),
            ]
        );
        assert_eq!(h.current_path(), &[4, 5]);
    }

    // Deep LCA: shared prefix [1,2,3], tails [4,5] → [9,10].
    // Expect leaves 5,4 (inner→outer), then enters 9,10 (outer→inner).
    #[test]
    fn hover_deep_lca() {
        let mut h: HoverState<u32> = HoverState::new();
        let _ = h.update_path(&[1, 2, 3, 4, 5]);
        let ev = h.update_path(&[1, 2, 3, 9, 10]);
        assert_eq!(
            ev,
            vec![
                HoverEvent::Leave(5),
                HoverEvent::Leave(4),
                HoverEvent::Enter(9),
                HoverEvent::Enter(10),
            ]
        );
        assert_eq!(h.current_path(), &[1, 2, 3, 9, 10]);
    }

    // Same path repeated: no transitions.
    #[test]
    fn hover_same_path_no_events() {
        let mut h: HoverState<u32> = HoverState::new();
        let first = h.update_path(&[7, 8]);
        assert_eq!(first, vec![HoverEvent::Enter(7), HoverEvent::Enter(8)]);
        let second = h.update_path(&[7, 8]);
        assert!(second.is_empty());
        assert_eq!(h.current_path(), &[7, 8]);
    }
}
