// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Cascade origin ordering.

/// The origin/strength of a style source within the Style layer.
///
/// Higher origins win over lower ones regardless of selector specificity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StyleOrigin {
    /// Low-precedence base styling (e.g. control defaults).
    Base = 0,
    /// Rule-based styling (stylesheets).
    Sheet = 1,
    /// High-precedence overrides (e.g. explicit style assignment).
    Override = 2,
}
