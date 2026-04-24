// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![no_std]

//! Reusable tree surface integration for Overstory.
//!
//! This crate owns the generic Overstory-side realization for tree rows,
//! including row focus/selection state and composite navigation policy, while
//! leaving domain-specific outline/inspection controllers in other crates.

extern crate alloc;

mod tree;

pub use tree::{
    TreeKeyboardAction, TreeRowAction, TreeRowIds, TreeRowPresentation, TreeViewController,
    TreeViewRealizedRow, TreeViewStyle,
};
