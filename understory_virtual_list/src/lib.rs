// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_virtual_list --heading-base-level=0

//! Understory Virtual List: core 1D virtualization primitives.
//!
//! This crate provides a small, renderer-agnostic core for virtualizing a dense strip
//! of items indexed `0..len`. It is intended to be shared across different UI stacks
//! and list/stack implementations.
//!
//! The core concepts are:
//!
//! - [`Scalar`]: a small abstraction over `f32`/`f64` used for extents, offsets,
//!   and scroll positions.
//! - [`ExtentModel`]: a trait describing a 1D strip of items with per-item extents
//!   and prefix-sum-style queries.
//! - [`compute_visible_strip`]: a helper that, given a scroll offset, viewport
//!   extent, and asymmetric overscan distances, returns which indices should be
//!   realized plus how much padding exists before and after them.
//! - [`VirtualList`]: a small controller that wraps an [`ExtentModel`] implementation,
//!   scroll state, viewport extent, and overscan, and caches the most recent
//!   [`VisibleStrip`]. It also provides index-based scrolling via [`ScrollAlign`]
//!   and convenience methods for visibility queries and scroll clamping.
//! - [`GridTrackModel`]: an adapter that maps a per-track [`ExtentModel`] onto a
//!   per-cell view for grid-like layouts (tracks × cells).
//!
//! This crate deliberately does **not** know about widgets, display trees, or any
//! particular UI framework. Host frameworks are responsible for:
//!
//! - Owning the actual data and view/widget instances.
//! - Calling [`VirtualList::visible_strip`] when scroll or viewport changes.
//! - Diffing the returned `[start, end)` index range to create/destroy children.
//! - Feeding measured item sizes back into an [`ExtentModel`] (for example via
//!   [`PrefixSumExtentModel`]).
//!
//! ## Minimal example
//!
//! A very simple fixed-height list:
//!
//! ```rust
//! use understory_virtual_list::{FixedExtentModel, VirtualList};
//!
//! // 100 items, each 20 logical pixels tall.
//! let model = FixedExtentModel::new(100, 20.0);
//! let mut list = VirtualList::new(model, 200.0, 40.0);
//!
//! // Scroll to 100px from the start.
//! list.set_scroll_offset(100.0);
//!
//! let strip = list.visible_strip();
//! assert!(strip.start < strip.end);
//! assert!(strip.content_extent > 0.0);
//!
//! // Host frameworks would now instantiate views for indices `start..end`
//! // and position them after `before_extent` worth of spacer.
//! ```
//!
//! For non-uniform item sizes, use [`PrefixSumExtentModel`] and feed measured
//! extents back into it after layout. A typical pattern is:
//! - start with a rough estimate for all items,
//! - measure actual extents after layout and call [`PrefixSumExtentModel::set_extent`]
//!   or [`PrefixSumExtentModel::rebuild`],
//! - and use [`PrefixSumExtentModel::total_extent_for_len`] and
//!   [`PrefixSumExtentModel::index_at_offset_for_len`] to keep scroll behavior
//!   stable as measurements refine.
//!
//! All extents and offsets live in a caller-chosen 1D coordinate space
//! (typically logical pixels) and are expected to be finite and non-negative.
//!
//! ## Grid-like example with tracks and cells
//!
//! For grids, use [`GridTrackModel`] to adapt a per-track model to per-cell
//! indices. In a vertical grid, tracks typically correspond to rows and cells
//! to columns:
//!
//! ```rust
//! use core::num::NonZeroUsize;
//! use understory_virtual_list::{FixedExtentModel, GridTrackModel, VirtualList};
//!
//! // Four tracks (rows), each 20 logical pixels tall.
//! let row_model = FixedExtentModel::new(4, 20.0);
//! // A 3-column grid over 12 cells (4 rows × 3 columns).
//! let columns = NonZeroUsize::new(3).unwrap();
//! let grid_model = GridTrackModel::new(row_model, columns, 12);
//! let mut list = VirtualList::new(grid_model, 40.0, 0.0);
//!
//! let strip = list.visible_strip();
//! assert!(strip.start < strip.end);
//! // Host code can map each visible cell index `i` to:
//! //   let track = list.model().track_of_cell(i);
//! //   let cell_in_track = list.model().cell_in_track(i);
//! ```
//!
//! This crate is `no_std` and uses `alloc`.

#![no_std]

extern crate alloc;

mod fixed;
mod grid_track;
mod model;
mod prefix_sum;
mod scalar;
mod virtual_list;

pub use fixed::FixedExtentModel;
pub use grid_track::GridTrackModel;
pub use model::{ExtentModel, ResizableExtentModel, VisibleStrip, compute_visible_strip};
pub use prefix_sum::PrefixSumExtentModel;
pub use scalar::Scalar;
pub use virtual_list::{ScrollAlign, VirtualList};
