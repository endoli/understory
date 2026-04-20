// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Entry lifecycle states.

/// Lifecycle state for a transcript entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryStatus {
    /// The entry is still being produced or updated.
    InProgress,
    /// The entry completed successfully.
    Complete,
    /// The entry ended in failure.
    Failed,
    /// The entry was cancelled before completing.
    Cancelled,
}
