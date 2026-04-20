// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transcript identifier and timestamp types.

/// Stable identifier for one transcript entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryId(pub u64);

/// Host-defined timestamp associated with a transcript entry.
///
/// This crate treats timestamps as opaque monotonic labels rather than wall
/// clock time. Hosts may interpret them as milliseconds, microseconds, ticks,
/// or another stable recorded-at unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub u64);
