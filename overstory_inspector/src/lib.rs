// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![no_std]

//! Inspector and property-grid surfaces for Overstory.
//!
//! This crate keeps repeated Overstory wiring for inspector-like surfaces out
//! of app examples while leaving domain-specific tree and property models with
//! the host application.

extern crate alloc;

mod property_grid;
mod tree;

pub use property_grid::{
    PropertyBadge, PropertyGridController, PropertyGridRealizedRow, PropertyGridRow,
    PropertyGridRowIds, PropertyGridStyle, PropertyValue,
};
pub use tree::{
    InspectorTreeClick, InspectorTreeController, InspectorTreeRealizedRow, InspectorTreeStyle,
};
