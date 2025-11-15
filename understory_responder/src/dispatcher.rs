// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dispatcher helper: walk a dispatch sequence and honor stop/consume outcomes.
//!
//! The dispatcher executes handlers for each step in a responder sequence and
//! applies simple propagation rules:
//!
//! - Process entries in order.
//! - Rely on the router to group phases into capture → target → bubble.
//! - [`Outcome::Stop`] aborts propagation immediately (no target/bubble if raised in capture).
//! - [`Outcome::StopAndConsume`] aborts propagation and returns `true`.
//! - Returns `true` if consumed; otherwise `false` (for both `Continue` and `Stop`).
//!
//! Dispatch sequences are typically produced by
//! [`Router::handle_with_hits`](crate::router::Router::handle_with_hits)
//! (pointer routing) or [`Router::dispatch_for`](crate::router::Router::dispatch_for)
//! (focus/keyboard routing).
//!
//! ## Minimal example
//!
//! ```
//! use understory_responder::dispatcher;
//! use understory_responder::types::{Dispatch, Outcome, Phase, Localizer};
//! #[derive(Copy, Clone, Debug)] struct Node(u32);
//!
//! // Build a simple capture → target → bubble sequence.
//! let seq: Vec<Dispatch<Node, (), ()>> = vec![
//!     // Capture from root→target (1 → 2)
//!     Dispatch::capture(Node(1)),
//!     Dispatch::capture(Node(2)),
//!     // Target (only the target node 2)
//!     Dispatch::target(Node(2)).with_localizer(Localizer::default()),
//!     // Bubble from target→root (2 → 1)
//!     Dispatch::bubble(Node(2)),
//!     Dispatch::bubble(Node(1)),
//! ];
//!
//! // Run the dispatcher and record the order of phases.
//! let mut handled: Vec<(Phase, u32)> = Vec::new();
//! let consumed = dispatcher::run(&seq, &mut (), |d, _| {
//!     handled.push((d.phase, d.node.0));
//!     Outcome::Continue
//! });
//!
//! // It should visit all entries and not be consumed.
//! assert!(!consumed);
//! assert_eq!(handled, vec![
//!     (Phase::Capture, 1), (Phase::Capture, 2),
//!     (Phase::Target, 2),
//!     (Phase::Bubble, 2), (Phase::Bubble, 1),
//! ]);
//! ```

use crate::types::{Dispatch, Outcome};

/// Run a handler over a dispatch sequence and honor stop/consume outcomes.
///
/// ## Usage
///
/// - Inputs:
///   - `seq`: a responder sequence typically produced by
///     [`Router::handle_with_hits`](crate::router::Router::handle_with_hits) (pointer routing)
///     or [`Router::dispatch_for`](crate::router::Router::dispatch_for) (focus/keyboard).
///     If you build a sequence by hand, it should follow the same capture → target → bubble
///     ordering that the router emits; `run` assumes this when applying `Stop`/`StopAndConsume`.
///   - `event`: a mutable event payload carried across handler calls; you own its shape.
///   - `handler`: your per‑entry callback; return an [`Outcome`] to control propagation.
/// - Semantics:
///   - [`Outcome::Continue`]: keep going.
///   - [`Outcome::Stop`]: abort propagation immediately (no later phases).
///   - [`Outcome::StopAndConsume`]: abort propagation and return `true`.
/// - Return:
///   - `true` if consumed (via `StopAndConsume`), otherwise `false`.
///
/// ## Tips
///
/// - Multiple listeners per node/phase: proxy them inside your `handler`; short‑circuit to emulate
///   “stopImmediatePropagation”.
/// - Default prevention: add a `default_prevented: bool` flag to your event and set it in `handler`.
///   After `run`, check the flag to decide whether to execute a default action.
///
/// ## Examples
///
/// ### prevent default while continuing propagation
///
/// ```
/// use understory_responder::dispatcher::run;
/// use understory_responder::types::{Dispatch, Outcome, Phase, Localizer};
/// #[derive(Copy, Clone, Debug)] struct Node(u32);
/// #[derive(Default)] struct Ev { default_prevented: bool, seen: Vec<(Phase, u32)> }
/// // Target handler sets default_prevented, but propagation continues.
/// let seq: Vec<Dispatch<Node, (), ()>> = vec![
///     Dispatch::capture(Node(1)),
///     Dispatch::capture(Node(2)),
///     Dispatch::target(Node(2)),
///     Dispatch::bubble(Node(2)),
///     Dispatch::bubble(Node(1)),
/// ];
///
/// let mut ev = Ev::default();
/// let consumed = run(&seq, &mut ev, |d, e| {
///     e.seen.push((d.phase, d.node.0));
///     if matches!(d.phase, Phase::Target) { e.default_prevented = true; }
///     Outcome::Continue
/// });
///
/// // Dispatcher is not consumed; default prevention is recorded on the event.
/// assert!(!consumed);
/// assert!(ev.default_prevented);
/// assert_eq!(ev.seen, vec![
///   (Phase::Capture, 1), (Phase::Capture, 2),
///   (Phase::Target, 2),
///   (Phase::Bubble, 2), (Phase::Bubble, 1),
/// ]);
/// ```
///
/// ### stop propagation in capture (no target/bubble)
///
/// ```
/// use understory_responder::dispatcher::run;
/// use understory_responder::types::{Dispatch, Outcome, Phase, Localizer};
/// #[derive(Copy, Clone, Debug)] struct Node(u32);
/// // Stop in the first capture entry; target/bubble are skipped.
/// let seq: Vec<Dispatch<Node, (), ()>> = vec![
///     Dispatch::capture(Node(1)),
///     Dispatch::capture(Node(2)),
///     Dispatch::target(Node(2)),
/// ];
///
/// let mut seen: Vec<(Phase, u32)> = Vec::new();
/// let consumed = run(&seq, &mut (), |d, _| {
///     seen.push((d.phase, d.node.0));
///     if d.phase == Phase::Capture && d.node.0 == 1 { Outcome::Stop } else { Outcome::Continue }
/// });
///
/// // Not consumed; propagation aborted after the first capture.
/// assert!(!consumed);
/// assert_eq!(seen, vec![(Phase::Capture, 1)]);
/// ```
pub fn run<K, W, M, E>(
    seq: &[Dispatch<K, W, M>],
    event: &mut E,
    mut handler: impl FnMut(&Dispatch<K, W, M>, &mut E) -> Outcome,
) -> bool {
    // The router already emits dispatch entries in capture → target → bubble
    // order, grouped by phase. We simply walk them in sequence and apply the
    // outcome rules.
    for d in seq {
        match handler(d, event) {
            Outcome::Continue => {}
            // Abort propagation immediately (spec-aligned: no target/bubble if raised in capture).
            Outcome::Stop => return false,
            Outcome::StopAndConsume => return true,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Phase;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    struct Node(u32);

    fn mk_seq() -> Vec<Dispatch<Node, (), ()>> {
        vec![
            Dispatch::capture(Node(1)),
            Dispatch::capture(Node(2)),
            Dispatch::target(Node(2)),
            Dispatch::bubble(Node(2)),
            Dispatch::bubble(Node(1)),
        ]
    }

    #[test]
    fn continue_through_all() {
        let seq = mk_seq();
        let mut seen: Vec<(Phase, u32)> = Vec::new();
        let consumed = run(&seq, &mut (), |d, _| {
            seen.push((d.phase, d.node.0));
            Outcome::Continue
        });
        assert!(!consumed);
        assert_eq!(seen.len(), seq.len());
    }

    #[test]
    fn default_prevention_pattern_sets_flag_at_target() {
        #[derive(Default)]
        struct Ev {
            default_prevented: bool,
            seen: Vec<(Phase, u32)>,
        }

        let seq = mk_seq();
        let mut ev = Ev::default();
        let consumed = run(&seq, &mut ev, |d, e| {
            e.seen.push((d.phase, d.node.0));
            if matches!(d.phase, Phase::Target) {
                e.default_prevented = true;
            }
            Outcome::Continue
        });

        assert!(!consumed);
        assert!(ev.default_prevented);
        assert_eq!(
            ev.seen,
            vec![
                (Phase::Capture, 1),
                (Phase::Capture, 2),
                (Phase::Target, 2),
                (Phase::Bubble, 2),
                (Phase::Bubble, 1),
            ]
        );
    }

    #[test]
    fn stop_aborts_propagation() {
        let seq = mk_seq();
        let mut seen: Vec<(Phase, u32)> = Vec::new();
        let consumed = run(&seq, &mut (), |d, _| {
            seen.push((d.phase, d.node.0));
            if d.phase == Phase::Capture && d.node.0 == 1 {
                Outcome::Stop
            } else {
                Outcome::Continue
            }
        });
        assert!(!consumed);
        // Stop during first capture aborts propagation immediately.
        assert_eq!(seen, vec![(Phase::Capture, 1)]);
    }

    #[test]
    fn stop_in_target_aborts_bubble_phase() {
        let seq = mk_seq();
        let mut seen: Vec<(Phase, u32)> = Vec::new();
        let consumed = run(&seq, &mut (), |d, _| {
            seen.push((d.phase, d.node.0));
            if matches!(d.phase, Phase::Target) {
                Outcome::Stop
            } else {
                Outcome::Continue
            }
        });
        assert!(!consumed);
        assert_eq!(
            seen,
            vec![(Phase::Capture, 1), (Phase::Capture, 2), (Phase::Target, 2),]
        );
    }

    #[test]
    fn stop_in_bubble_aborts_remaining_bubble_entries() {
        let seq = mk_seq();
        let mut seen: Vec<(Phase, u32)> = Vec::new();
        let consumed = run(&seq, &mut (), |d, _| {
            seen.push((d.phase, d.node.0));
            if d.phase == Phase::Bubble && d.node.0 == 2 {
                Outcome::Stop
            } else {
                Outcome::Continue
            }
        });
        assert!(!consumed);
        assert_eq!(
            seen,
            vec![
                (Phase::Capture, 1),
                (Phase::Capture, 2),
                (Phase::Target, 2),
                (Phase::Bubble, 2),
            ]
        );
    }

    #[test]
    fn stop_and_consume_aborts_remaining_phases() {
        let seq = mk_seq();
        let mut seen: Vec<(Phase, u32)> = Vec::new();
        let consumed = run(&seq, &mut (), |d, _| {
            seen.push((d.phase, d.node.0));
            if d.phase == Phase::Target {
                Outcome::StopAndConsume
            } else {
                Outcome::Continue
            }
        });
        assert!(consumed);
        // Should include both capture entries and the target; bubbles are skipped.
        assert_eq!(
            seen,
            vec![(Phase::Capture, 1), (Phase::Capture, 2), (Phase::Target, 2),]
        );
    }
}
