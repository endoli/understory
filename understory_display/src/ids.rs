// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Stable identifiers for semantic provenance.

/// Optional semantic/provenance identifier carried by one display node.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticId(u32);

impl SemanticId {
    /// Creates a semantic identifier from a host-defined numeric value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the underlying host-defined value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}
