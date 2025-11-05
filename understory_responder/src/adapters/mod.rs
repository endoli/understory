// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Adapters to integrate with other Understory crates.
//!
//! Enabled via feature flags to keep the core small and `no_std` by default.

#[cfg(feature = "box_tree_adapter")]
pub mod box_tree;
