// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Adapters to integrate with other Understory crates.
//!
//! This module provides integration helpers for spatial data structures and UI frameworks.
//! Each adapter is gated behind a feature flag to keep the core responder lightweight and `no_std` by default.
//!
//! ## Available Adapters
//!
//! - [`box_tree`] (`box_tree_adapter` feature): Integration with `understory_box_tree` for 2D spatial queries
//!   and UI navigation. Provides:
//!   - [`box_tree::top_hit_for_point`] and [`box_tree::hits_for_rect`]: Convert spatial query results into responder hits
//!   - [`box_tree::navigation`]: Filtered tree traversal for keyboard navigation and focus cycling  
//!   - [`box_tree::ClickAdapter`]: Click state integration with automatic node bounds lookup

#[cfg(feature = "box_tree_adapter")]
pub mod box_tree;
