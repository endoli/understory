// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![no_std]

//! Reusable linear list surface integration for Overstory.
//!
//! This crate owns the generic Overstory-side realization for list rows,
//! including row focus/selection state and composite navigation policy, while
//! leaving domain-specific row semantics in other crates.

extern crate alloc;

mod list;

pub use list::{
    ListKeyboardAction, ListRowAction, ListRowIds, ListRowPresentation, ListViewController,
    ListViewRealizedRow, ListViewStyle,
};
