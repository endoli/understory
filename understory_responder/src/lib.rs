// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_responder --heading-base-level=0

//! Understory Responder: a deterministic, `no_std` router for UI events.
//!
//! ## Overview
//!
//! This crate builds the responder chain sequence — capture → target → bubble — from pre‑resolved hits.
//! It does not perform hit testing.
//! Instead, feed it [`ResolvedHit`](crate::types::ResolvedHit) items (for example from a box tree or a 3D ray cast), and it emits a deterministic propagation sequence you can dispatch.
//!
//! ## Inputs
//!
//! Provide one or more [`ResolvedHit`](crate::types::ResolvedHit) values for candidate targets.
//! A [`ResolvedHit`](crate::types::ResolvedHit) contains the node key, an optional root→target `path`, a [`DepthKey`](crate::types::DepthKey) used for ordering,
//! a [`Localizer`](crate::types::Localizer) for coordinate conversion, and an optional `meta` payload (e.g., text or ray‑hit details).
//! You may also provide a [`ParentLookup`](crate::types::ParentLookup) source to reconstruct a path when `path` is absent.
//!
//! ## Ordering
//!
//! Candidates are ranked by [`DepthKey`](crate::types::DepthKey).
//! For `Z`, higher is nearer. For `Distance`, lower is nearer. When kinds differ, `Z` ranks above `Distance` by default.
//! Equal‑depth ties are stable and the router selects the last.
//!
//! ## Pointer capture
//!
//! If capture is set, the router routes to the captured node regardless of fresh hits.
//! It uses the matching hit’s path and `meta` if present, otherwise reconstructs a path with [`ParentLookup`](crate::types::ParentLookup) or falls back to a singleton path.
//! Capture bypasses scope filtering.
//!
//! ## Layering
//!
//! The router only computes the traversal order. A higher‑level dispatcher can execute handlers, honor cancelation, and apply toolkit policies.
//!
//! ## Workflow
//!
//! 1) Pick candidates — e.g., from a 2D box tree or a 3D ray cast — and build
//!    one or more [`ResolvedHit`](crate::types::ResolvedHit) values (with optional root→target paths).
//! 2) Route — [`Router`](crate::router::Router) ranks candidates by [`DepthKey`](crate::types::DepthKey) and selects
//!    exactly one target. It emits a capture→target→bubble sequence for that target’s path.
//!    - Overlapping siblings: only the topmost/nearest candidate is selected; siblings do not receive the target.
//!    - Equal‑depth ties: deterministic and stable; the last candidate wins unless you pre‑order your hits or set a policy.
//!    - Pointer capture: overrides selection until released.
//! 3) Hover — derive the path from the dispatch via [`path_from_dispatch`](crate::hover::path_from_dispatch)
//!    and feed it to [`HoverState`](crate::hover::HoverState). `HoverState` emits leave (inner→outer)
//!    and enter (outer→inner) events for the minimal transition between old and new paths.
//!
//! ## Dispatcher sketch
//!
//! The snippet below shows how a higher‑level layer could walk the router’s sequence and honor stop/cancel rules.
//! It groups contiguous entries by phase and allows a handler to stop within a phase or stop‑and‑consume the event entirely.
//!
//! ```no_run
//! use understory_responder::types::{Dispatch, Outcome, Phase};
//!
//! /// Deliver a single dispatch item to your toolkit and return
//! /// whether to continue propagation or stop.
//! fn deliver<K, W, M>(_d: &Dispatch<K, W, M>) -> Outcome {
//!     Outcome::Continue
//! }
//!
//! /// Walk the dispatch sequence produced by the router.
//! /// Returns true if the event was consumed (e.g., default prevented).
//! fn run_dispatch<K, W, M>(seq: &[Dispatch<K, W, M>]) -> bool {
//!     let mut consumed = false;
//!     let mut i = 0;
//!     while i < seq.len() {
//!         let phase = seq[i].phase;
//!         // Process contiguous entries for the same phase.
//!         while i < seq.len() && seq[i].phase == phase {
//!             match deliver(&seq[i]) {
//!                 Outcome::Continue => {}
//!                 Outcome::Stop => {
//!                     // Skip remaining entries in this phase.
//!                     while i + 1 < seq.len() && seq[i + 1].phase == phase {
//!                         i += 1;
//!                     }
//!                 }
//!                 Outcome::StopAndConsume => {
//!                     consumed = true;
//!                     // Abort remaining phases.
//!                     return consumed;
//!                 }
//!             }
//!             i += 1;
//!         }
//!     }
//!     consumed
//! }
//!
//! # // Example: invoking with a dummy sequence
//! # fn _example<K, W, M>(seq: &[Dispatch<K, W, M>]) { let _ = run_dispatch(seq); }
//! ```
//!
//! This crate is `no_std` and uses `alloc`.
//!
//!

#![no_std]

extern crate alloc;

pub mod adapters;
pub mod hover;
pub mod router;
pub mod types;
