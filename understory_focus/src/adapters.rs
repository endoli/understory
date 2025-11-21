// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integration helpers for other Understory crates.
//!
//! Modules in this file are behind feature flags so `understory_focus` can
//! remain usable in contexts that do not depend on those crates.
//!
//! - [`box_tree`] (`box_tree_adapter` feature): build [`crate::FocusSpace`]
//!   views from an [`understory_box_tree::Tree`].

#[cfg(feature = "box_tree_adapter")]
pub mod box_tree;
