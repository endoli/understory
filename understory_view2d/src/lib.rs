// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_view2d --heading-base-level=0

//! Understory View 2D: 1D and 2D view/viewport primitives.
//!
//! This crate provides small, headless models of world‑space views where the
//! view extents are typically expressed in device pixels. It focuses on:
//! - Camera / viewport state (pan + zoom).
//! - Coordinate conversion between world and view/device (pixel) space.
//! - View fitting and centering/alignment helpers.
//! - Simple zoom / pan constraints.
//!
//! It does **not** own any scene graph or rendering backend. Callers are
//! expected to:
//! - Maintain their own scene or display tree.
//! - Use [`Viewport2D`] / [`Viewport1D`] to derive transforms and
//!   visible‑region bounds.
//! - Wire input events (for example, from `ui-events`) into pan/zoom
//!   operations at a higher layer.
//! - Optionally combine `world_units_per_pixel` helpers with display DPI and
//!   external unit libraries (for example `joto_constants`) to reason about
//!   physical sizes.
//!
//! ## Minimal 2D example
//!
//! ```rust
//! use kurbo::{Point, Rect};
//! use understory_view2d::Viewport2D;
//!
//! // Device/view rect: 800x600 window.
//! let view_rect = Rect::new(0.0, 0.0, 800.0, 600.0);
//! let mut view = Viewport2D::new(view_rect);
//!
//! // Optional world bounds for fitting/clamping.
//! view.set_world_bounds(Some(Rect::new(-100.0, -100.0, 100.0, 100.0)));
//! view.fit_world();
//!
//! // Convert a device-space point into world space (for hit testing, etc.).
//! let device_pt = Point::new(400.0, 300.0);
//! let world_pt = view.view_to_world_point(device_pt);
//! ```
//!
//! ## Minimal 1D example (timeline/axis)
//!
//! ```rust
//! use understory_view2d::Viewport1D;
//!
//! // 0..800 view span in pixels.
//! let span = 0.0..800.0;
//! let mut view = Viewport1D::new(span);
//!
//! // World bounds in \"time\" units.
//! view.set_world_bounds(Some(0.0..120.0));
//! view.fit_world();
//!
//! // Convert a device-space X coordinate into world-space time.
//! let device_x = 400.0;
//! let world_t = view.view_to_world_x(device_x);
//! ```
//!
//! ## Design notes
//!
//! - Cameras are axis‑aligned with a **uniform** zoom factor.
//! - Panning operates in view space; zooming is expressed as a scalar.
//! - Rotation is intentionally left out of the initial design and can be
//!   added later as a backwards‑compatible extension.
//! - Controllers that interpret `ui-events` and more complex behaviors such
//!   as inertia are expected to live in higher‑level crates built on top of
//!   this one.
//!
//! ## Culling example
//!
//! `Viewport2D` can be used to compute a visible world rectangle for culling.
//! For example, given a list of world‑space rectangles, you can retain only
//! those that intersect the current view:
//!
//! ```rust
//! use kurbo::Rect;
//! use understory_view2d::Viewport2D;
//!
//! let view_rect = Rect::new(0.0, 0.0, 800.0, 600.0);
//! let view = Viewport2D::new(view_rect);
//!
//! let visible_world = view.visible_world_rect();
//! let world_items: &[Rect] = &[
//!     Rect::new(-10.0, -10.0, 10.0, 10.0),
//!     Rect::new(1_000.0, 1_000.0, 1_100.0, 1_100.0),
//! ];
//!
//! let visible_items: Vec<Rect> = world_items
//!     .iter()
//!     .copied()
//!     .filter(|r| r.intersect(visible_world).area() > 0.0)
//!     .collect();
//! assert!(!visible_items.is_empty());
//! ```
//!
//! This crate is `no_std`.

#![no_std]

mod modes;
mod viewport1d;
mod viewport2d;

pub use modes::{ClampMode, FitMode};
pub use viewport1d::{Viewport1D, Viewport1DDebugInfo};
pub use viewport2d::{Viewport2D, Viewport2DDebugInfo};
