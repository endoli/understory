// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Channel and channel set types for identifying dirty domains.

use core::fmt;
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

/// Identifies a dirty domain (layout, paint, accessibility, etc.).
///
/// A channel is a lightweight handle (a single `u8`) that represents a specific
/// invalidation domain. Dependencies and dirty state are tracked per-channel,
/// allowing independent invalidation of different concerns.
///
/// # Example
///
/// ```
/// use understory_dirty::Channel;
///
/// // Define your own channels as constants
/// const LAYOUT: Channel = Channel::new(0);
/// const PAINT: Channel = Channel::new(1);
/// const A11Y: Channel = Channel::new(2);
/// const STYLE: Channel = Channel::new(3);
/// ```
///
/// # See Also
///
/// - [`ChannelSet`]: A compact set of channels.
/// - [`DirtySet`](crate::DirtySet): Tracks dirty keys per channel.
/// - [`DirtyGraph`](crate::DirtyGraph): Stores dependencies per channel.
/// - [`DirtyTracker`](crate::DirtyTracker): Convenience wrapper combining graph + set.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Channel(u8);

impl Channel {
    /// Creates a new channel with the given index.
    ///
    /// # Panics
    ///
    /// Panics if `index >= 64`, as [`ChannelSet`] only supports 64 channels.
    #[must_use]
    pub const fn new(index: u8) -> Self {
        assert!(index < 64, "Channel index must be less than 64");
        Self(index)
    }

    /// Returns the index of this channel.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }

    /// Converts this channel into a single-element [`ChannelSet`].
    #[must_use]
    pub const fn into_set(self) -> ChannelSet {
        ChannelSet(1_u64 << self.0)
    }
}

impl fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Channel").field(&self.0).finish()
    }
}

/// A compact bitfield representing a set of up to 64 channels.
///
/// `ChannelSet` is useful for operations that affect multiple channels at once,
/// such as marking a node dirty in several domains simultaneously.
///
/// # Example
///
/// ```
/// use understory_dirty::{Channel, ChannelSet};
///
/// const LAYOUT: Channel = Channel::new(0);
/// const PAINT: Channel = Channel::new(1);
/// const A11Y: Channel = Channel::new(2);
///
/// let mut set = ChannelSet::empty();
/// set.insert(LAYOUT);
/// set.insert(PAINT);
///
/// assert!(set.contains(LAYOUT));
/// assert!(set.contains(PAINT));
/// assert!(!set.contains(A11Y));
///
/// // Combine sets with bitwise OR
/// let combined = LAYOUT.into_set() | A11Y.into_set();
/// assert!(combined.contains(LAYOUT));
/// assert!(combined.contains(A11Y));
/// ```
///
/// # See Also
///
/// - [`Channel`]: The single-channel identifier stored in this set.
/// - [`DirtySet`](crate::DirtySet): Uses channels to partition dirty keys.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct ChannelSet(u64);

impl ChannelSet {
    /// An empty channel set.
    pub const EMPTY: Self = Self(0);

    /// A channel set containing all 64 possible channels.
    pub const ALL: Self = Self(u64::MAX);

    /// Creates an empty channel set.
    #[must_use]
    pub const fn empty() -> Self {
        Self::EMPTY
    }

    /// Creates a channel set containing all 64 possible channels.
    #[must_use]
    pub const fn all() -> Self {
        Self::ALL
    }

    /// Returns `true` if this set contains no channels.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if this set contains the given channel.
    #[must_use]
    pub const fn contains(self, channel: Channel) -> bool {
        (self.0 & (1_u64 << channel.0)) != 0
    }

    /// Inserts a channel into the set.
    pub fn insert(&mut self, channel: Channel) {
        self.0 |= 1_u64 << channel.0;
    }

    /// Removes a channel from the set.
    pub fn remove(&mut self, channel: Channel) {
        self.0 &= !(1_u64 << channel.0);
    }

    /// Returns the number of channels in the set.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// Returns an iterator over the channels in this set.
    #[must_use]
    pub const fn iter(self) -> ChannelSetIter {
        ChannelSetIter { bits: self.0 }
    }
}

impl fmt::Debug for ChannelSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl BitOr for ChannelSet {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for ChannelSet {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for ChannelSet {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for ChannelSet {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl Not for ChannelSet {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl From<Channel> for ChannelSet {
    fn from(channel: Channel) -> Self {
        channel.into_set()
    }
}

impl IntoIterator for ChannelSet {
    type Item = Channel;
    type IntoIter = ChannelSetIter;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over the channels in a [`ChannelSet`].
#[derive(Clone, Debug)]
pub struct ChannelSetIter {
    bits: u64,
}

impl Iterator for ChannelSetIter {
    type Item = Channel;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bits == 0 {
            return None;
        }
        // SAFETY: trailing_zeros of a u64 is at most 63, which fits in u8.
        #[expect(clippy::cast_possible_truncation, reason = "trailing_zeros <= 63")]
        let index = self.bits.trailing_zeros() as u8;
        self.bits &= self.bits - 1; // Clear the lowest set bit
        Some(Channel(index))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let count = self.bits.count_ones() as usize;
        (count, Some(count))
    }
}

impl ExactSizeIterator for ChannelSetIter {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    const LAYOUT: Channel = Channel::new(0);
    const PAINT: Channel = Channel::new(1);
    const A11Y: Channel = Channel::new(2);

    #[test]
    fn channel_new_valid() {
        let ch = Channel::new(42);
        assert_eq!(ch.index(), 42);
    }

    #[test]
    #[should_panic(expected = "Channel index must be less than 64")]
    fn channel_new_invalid() {
        let _ = Channel::new(64);
    }

    #[test]
    fn channel_set_operations() {
        let mut set = ChannelSet::empty();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);

        set.insert(LAYOUT);
        assert!(!set.is_empty());
        assert!(set.contains(LAYOUT));
        assert!(!set.contains(PAINT));
        assert_eq!(set.len(), 1);

        set.insert(PAINT);
        assert!(set.contains(PAINT));
        assert_eq!(set.len(), 2);

        set.remove(LAYOUT);
        assert!(!set.contains(LAYOUT));
        assert!(set.contains(PAINT));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn channel_set_bitwise() {
        let a = LAYOUT.into_set();
        let b = PAINT.into_set();
        let c = a | b;

        assert!(c.contains(LAYOUT));
        assert!(c.contains(PAINT));
        assert!(!c.contains(A11Y));

        let d = c & a;
        assert!(d.contains(LAYOUT));
        assert!(!d.contains(PAINT));

        let e = !a;
        assert!(!e.contains(LAYOUT));
        assert!(e.contains(PAINT));
    }

    #[test]
    fn channel_set_iter() {
        let set = LAYOUT.into_set() | A11Y.into_set();
        let channels: Vec<_> = set.iter().collect();

        assert_eq!(channels.len(), 2);
        assert!(channels.contains(&LAYOUT));
        assert!(channels.contains(&A11Y));
    }

    #[test]
    fn channel_set_iter_exact_size() {
        let set = LAYOUT.into_set() | PAINT.into_set() | A11Y.into_set();
        let iter = set.iter();
        assert_eq!(iter.len(), 3);
    }
}
