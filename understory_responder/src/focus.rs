// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Focus state helper: compute enter/leave transitions from focus path changes.
//!
//! ## Usage
//! 1) Decide when focus changes (e.g., after a routed pointer-up).
//! 2) Build a root→target path for the new focus (via a router, parent lookup, or stored path).
//! 3) Call [`FocusState::update_path`] with that path to get `Enter(..)` / `Leave(..)` transitions.
//!
//! ## Minimal example
//! ```
//! use understory_responder::focus::{FocusState, FocusEvent};
//! let mut f: FocusState<u32> = FocusState::new();
//! assert_eq!(f.update_path(&[1, 2]), vec![FocusEvent::Enter(1), FocusEvent::Enter(2)]);
//! assert_eq!(f.update_path(&[1, 3]), vec![FocusEvent::Leave(2), FocusEvent::Enter(3)]);
//! ```
//!
//! ## With a spatial focus policy (conceptual)
//!
//! The focus state pairs naturally with a spatial policy (for example from the
//! `understory_focus` crate):
//!
//! ```ignore
//! use understory_focus::{DefaultPolicy, FocusEntry, FocusPolicy, FocusSpace, Navigation, WrapMode};
//! use understory_responder::focus::{FocusEvent, FocusState};
//!
//! # type NodeId = u32;
//! # let root: NodeId = 1;
//! # let current: NodeId = 2;
//! # let next: NodeId = 3;
//! # let entries: Vec<FocusEntry<NodeId>> = Vec::new();
//! // 1. Build a FocusSpace from your geometry layer (e.g., box tree).
//! let space = FocusSpace { nodes: &entries };
//! let policy = DefaultPolicy { wrap: WrapMode::Scope };
//!
//! // 2. Choose the next focused node based on navigation intent.
//! let next = policy.next(current, Navigation::Right, &space).unwrap_or(current);
//!
//! // 3. Resolve the node id to a root→target path and feed it into FocusState.
//! let mut focus_state: FocusState<NodeId> = FocusState::new();
//! let new_path: Vec<NodeId> = vec![root, next];
//! let events: Vec<FocusEvent<NodeId>> = focus_state.update_path(&new_path);
//! # let _ = events;
//! ```
//!
//! In a real application, this path would typically come from a parent lookup
//! or a precomputed path table rather than a hard-coded vector.

use alloc::vec::Vec;

/// A simple focus state machine over root→target paths.
///
/// Tracks the current focused path and, when updated, computes the minimal
/// sequence of leave and enter transitions to move from the old focus to the new.
///
/// Ordering semantics:
/// - Leave events are emitted from inner-most to outer-most.
/// - Enter events are emitted from outer-most to inner-most.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusState<K: Copy + Eq> {
    current: Vec<K>,
}

/// A focus transition event.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FocusEvent<K> {
    /// Focus enters the given node (outer→inner).
    Enter(K),
    /// Focus leaves the given node (inner→outer).
    Leave(K),
}

impl<K: Copy + Eq> FocusState<K> {
    /// Create an empty focus state.
    pub fn new() -> Self {
        Self {
            current: Vec::new(),
        }
    }

    /// Return the current root→target focus path (if any).
    pub fn current_path(&self) -> &[K] {
        &self.current
    }

    /// Clear the current focus path, returning leave events (inner→outer).
    pub fn clear(&mut self) -> Vec<FocusEvent<K>> {
        let mut out = Vec::new();
        for &k in self.current.iter().rev() {
            out.push(FocusEvent::Leave(k));
        }
        self.current.clear();
        out
    }

    /// Update focus to a new path and return enter/leave transitions.
    pub fn update_path(&mut self, new_path: &[K]) -> Vec<FocusEvent<K>> {
        // Compute shared prefix length (LCA depth).
        let mut lca = 0;
        while lca < self.current.len() && lca < new_path.len() && self.current[lca] == new_path[lca]
        {
            lca += 1;
        }

        let mut out = Vec::new();
        // Leaves: old tail back to LCA (exclusive), inner→outer.
        for &k in self.current[lca..].iter().rev() {
            out.push(FocusEvent::Leave(k));
        }
        // Enters: LCA down to new tail, outer→inner.
        for &k in &new_path[lca..] {
            out.push(FocusEvent::Enter(k));
        }
        self.current.clear();
        self.current.extend_from_slice(new_path);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    // Fresh focus: expect outer→inner enters.
    #[test]
    fn focus_enter_on_fresh_path() {
        let mut f: FocusState<u32> = FocusState::new();
        let ev = f.update_path(&[1, 2, 3]);
        assert_eq!(
            ev,
            vec![
                FocusEvent::Enter(1),
                FocusEvent::Enter(2),
                FocusEvent::Enter(3)
            ]
        );
        assert_eq!(f.current_path(), &[1, 2, 3]);
    }

    // Clearing focus: inner→outer leaves.
    #[test]
    fn focus_leave_to_empty() {
        let mut f: FocusState<u32> = FocusState::new();
        let _ = f.update_path(&[1, 2]);
        let ev = f.clear();
        assert_eq!(ev, vec![FocusEvent::Leave(2), FocusEvent::Leave(1)]);
        assert!(f.current_path().is_empty());
    }

    // Branch change with shallow LCA.
    #[test]
    fn focus_branch_change() {
        let mut f: FocusState<u32> = FocusState::new();
        let _ = f.update_path(&[1, 2, 3]);
        let ev = f.update_path(&[1, 4]);
        assert_eq!(
            ev,
            vec![
                FocusEvent::Leave(3),
                FocusEvent::Leave(2),
                FocusEvent::Enter(4)
            ]
        );
        assert_eq!(f.current_path(), &[1, 4]);
    }

    // Disjoint paths: leave entire old, enter entire new.
    #[test]
    fn focus_disjoint_paths() {
        let mut f: FocusState<u32> = FocusState::new();
        let _ = f.update_path(&[1, 2, 3]);
        let ev = f.update_path(&[4, 5]);
        assert_eq!(
            ev,
            vec![
                FocusEvent::Leave(3),
                FocusEvent::Leave(2),
                FocusEvent::Leave(1),
                FocusEvent::Enter(4),
                FocusEvent::Enter(5),
            ]
        );
        assert_eq!(f.current_path(), &[4, 5]);
    }

    // Deep LCA: shared prefix [1,2,3].
    #[test]
    fn focus_deep_lca() {
        let mut f: FocusState<u32> = FocusState::new();
        let _ = f.update_path(&[1, 2, 3, 4, 5]);
        let ev = f.update_path(&[1, 2, 3, 9, 10]);
        assert_eq!(
            ev,
            vec![
                FocusEvent::Leave(5),
                FocusEvent::Leave(4),
                FocusEvent::Enter(9),
                FocusEvent::Enter(10),
            ]
        );
        assert_eq!(f.current_path(), &[1, 2, 3, 9, 10]);
    }

    // Same path repeated: no transitions.
    #[test]
    fn focus_same_path_no_events() {
        let mut f: FocusState<u32> = FocusState::new();
        let first = f.update_path(&[7, 8]);
        assert_eq!(first, vec![FocusEvent::Enter(7), FocusEvent::Enter(8)]);
        let second = f.update_path(&[7, 8]);
        assert!(second.is_empty());
        assert_eq!(f.current_path(), &[7, 8]);
    }
}
