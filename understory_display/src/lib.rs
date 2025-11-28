// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Understory Display List: a POD render tree with stable ids, diffs, and damage integration.
//!
//! This crate defines a small, `no_std` display list that higher layers can build and
//! diff between frames. It is intended to be the common render target for UI widgets,
//! SVG-like content, and other 2D scenes in Understory.
//!
//! ## Overview
//!
//! - Each render operation (`Op`) has a stable [`OpId`], group id, z value, and optional
//!   semantic region id for provenance and a11y mapping.
//! - Ops are collected into a [`DisplayList`], which can be diffed against another list
//!   using [`diff`] to produce a [`Diff`] describing inserts, removes, moves, and
//!   replacements.
//! - A coarse **damage** API accepts dirty rectangles (for example from a box tree) and
//!   returns the subset of ops whose bounds intersect those rects.
//! - A [`ResourceSnapshot`] lists referenced glyph runs and images, allowing backends to
//!   manage their own resource lifetimes.
//!
//! The display list does not perform rendering itself; a backend adapter can consume
//! [`DisplayList`] and [`Diff`] to update GPU resources and draw.
//!
//! ## Why a display list?
//!
//! Higher layers could in principle draw directly into a backend such as Vello, but a
//! recorded, diffable display list provides a few concrete advantages:
//!
//! - **Incremental updates:** Stable [`OpId`]s and [`diff`] let backends update GPU state
//!   or caches incrementally rather than re‑uploading or rebuilding everything every
//!   frame.
//! - **Damage‑driven rendering:** Per‑op [`Rect`] bounds and [`DisplayList::culled_by_damage`]
//!   make it easy to combine geometry damage (for example from a box tree) with repaint
//!   decisions and skip unaffected ops.
//! - **Backend independence:** A single [`DisplayList`] can feed multiple adapters
//!   (Vello, CPU, screenshot renderer, etc.) without callers knowing about concrete GPU
//!   types.
//! - **Debugging and tests:** Recorded ops are easy to log, snapshot, and assert
//!   against in golden tests, which is much harder with “fire‑and‑forget” drawing.
//!
//! If other ecosystems grow their own recording abstractions (for example AnyRender’s
//! recorders, or a future `vello_api` with a recordable scene format), `understory_display`
//! is intended to stay compatible at the conceptual level: a flat sequence of paint ops
//! with ids, bounds, and grouping/clip structure. In practice, diffability and damage
//! integration depend on having stable identifiers, explicit bounds, and visible
//! insert/remove/move/rewrite operations; if an external recorder exposes those
//! invariants, it should be possible to build adapters between that representation and
//! [`DisplayList`]. If not, a thin translation layer or cooperation upstream would be
//! needed to add the missing pieces.

#![no_std]

extern crate alloc;

/// Re-export of Peniko's blend mode type for compositing groups and images.
pub use peniko::BlendMode;

mod diff_impl;
mod ids;
mod list;
mod ops;

pub use crate::diff_impl::*;
pub use crate::ids::*;
pub use crate::list::*;
pub use crate::ops::*;
