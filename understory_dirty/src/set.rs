// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dirty set: accumulated dirty keys per channel.

use core::hash::Hash;

use hashbrown::HashSet;

use crate::channel::Channel;

/// Maximum number of channels supported (64).
const MAX_CHANNELS: usize = 64;

/// Accumulated dirty keys per channel with generation tracking.
///
/// `DirtySet` maintains a set of dirty keys for each channel, along with a
/// generation counter that increments on every mutation. The generation can
/// be used to detect stale computations or cache invalidation.
///
/// # Type Parameters
///
/// - `K`: The key type, typically a node identifier. Must be `Copy + Eq + Hash`.
///   If your natural key is owned/structured, see [`intern::Interner`](crate::intern::Interner).
///
/// # Example
///
/// ```
/// use understory_dirty::{Channel, DirtySet};
///
/// const LAYOUT: Channel = Channel::new(0);
/// const PAINT: Channel = Channel::new(1);
///
/// let mut dirty = DirtySet::<u32>::new();
///
/// // Mark nodes dirty
/// dirty.mark(1, LAYOUT);
/// dirty.mark(2, LAYOUT);
/// dirty.mark(1, PAINT);
///
/// assert!(dirty.is_dirty(1, LAYOUT));
/// assert!(dirty.is_dirty(2, LAYOUT));
/// assert!(dirty.is_dirty(1, PAINT));
/// assert!(!dirty.is_dirty(2, PAINT));
///
/// // Drain returns and clears dirty keys for a channel
/// let layout_dirty: Vec<_> = dirty.drain(LAYOUT).collect();
/// assert_eq!(layout_dirty.len(), 2);
/// assert!(!dirty.is_dirty(1, LAYOUT));
/// ```
///
/// # See Also
///
/// - [`DirtyTracker`](crate::DirtyTracker): Convenience wrapper combining a [`DirtyGraph`](crate::DirtyGraph)
///   and `DirtySet`.
/// - [`drain_sorted`](crate::drain_sorted): Drain a [`DirtySet`] in topological order given a graph.
/// - [`drain_affected_sorted`](crate::drain_affected_sorted): Drain affected keys (roots + dependents), useful with [`LazyPolicy`](crate::LazyPolicy).
#[derive(Debug)]
pub struct DirtySet<K>
where
    K: Copy + Eq + Hash,
{
    /// Per-channel dirty key sets.
    channels: [HashSet<K>; MAX_CHANNELS],
    /// Generation counter, incremented on each mutation.
    generation: u64,
}

impl<K> Default for DirtySet<K>
where
    K: Copy + Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K> DirtySet<K>
where
    K: Copy + Eq + Hash,
{
    /// Creates a new empty dirty set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            channels: core::array::from_fn(|_| HashSet::new()),
            generation: 0,
        }
    }

    /// Returns the current generation.
    ///
    /// The generation is incremented on every mutation (mark, clear, drain).
    /// This can be used to detect whether the dirty set has changed since a
    /// previous observation.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Marks a key as dirty in the given channel.
    ///
    /// Returns `true` if the key was newly inserted, `false` if it was already dirty.
    pub fn mark(&mut self, key: K, channel: Channel) -> bool {
        self.generation = self.generation.wrapping_add(1);
        self.channels[channel.index() as usize].insert(key)
    }

    /// Returns `true` if the key is dirty in the given channel.
    #[must_use]
    pub fn is_dirty(&self, key: K, channel: Channel) -> bool {
        self.channels[channel.index() as usize].contains(&key)
    }

    /// Returns `true` if there are any dirty keys in the given channel.
    #[must_use]
    pub fn has_dirty(&self, channel: Channel) -> bool {
        !self.channels[channel.index() as usize].is_empty()
    }

    /// Returns `true` if there are no dirty keys in any channel.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.channels.iter().all(HashSet::is_empty)
    }

    /// Returns the number of dirty keys in the given channel.
    #[must_use]
    pub fn len(&self, channel: Channel) -> usize {
        self.channels[channel.index() as usize].len()
    }

    /// Returns an iterator over the dirty keys in the given channel.
    ///
    /// This does not clear the dirty state. Use [`drain`](Self::drain) to
    /// consume and clear.
    pub fn iter(&self, channel: Channel) -> impl Iterator<Item = K> + '_ {
        self.channels[channel.index() as usize].iter().copied()
    }

    /// Drains and returns the dirty keys for the given channel.
    ///
    /// After this call, the channel will have no dirty keys.
    pub fn drain(&mut self, channel: Channel) -> impl Iterator<Item = K> + '_ {
        self.generation = self.generation.wrapping_add(1);
        self.channels[channel.index() as usize].drain()
    }

    /// Clears all dirty keys in the given channel.
    pub fn clear(&mut self, channel: Channel) {
        self.generation = self.generation.wrapping_add(1);
        self.channels[channel.index() as usize].clear();
    }

    /// Clears all dirty keys in all channels.
    pub fn clear_all(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        for set in &mut self.channels {
            set.clear();
        }
    }

    /// Removes a specific key from all channels.
    ///
    /// This is useful when a node is removed from the tree entirely.
    pub fn remove_key(&mut self, key: K) {
        let mut removed = false;
        for set in &mut self.channels {
            removed |= set.remove(&key);
        }
        if removed {
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

impl<K> Clone for DirtySet<K>
where
    K: Copy + Eq + Hash,
{
    fn clone(&self) -> Self {
        Self {
            channels: core::array::from_fn(|i| self.channels[i].clone()),
            generation: self.generation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    const LAYOUT: Channel = Channel::new(0);
    const PAINT: Channel = Channel::new(1);

    #[test]
    fn mark_and_query() {
        let mut dirty = DirtySet::<u32>::new();

        assert!(!dirty.is_dirty(1, LAYOUT));
        assert!(dirty.is_empty());

        let inserted = dirty.mark(1, LAYOUT);
        assert!(inserted);
        assert!(dirty.is_dirty(1, LAYOUT));
        assert!(!dirty.is_empty());
        assert!(dirty.has_dirty(LAYOUT));

        // Marking again returns false
        let inserted_again = dirty.mark(1, LAYOUT);
        assert!(!inserted_again);
    }

    #[test]
    fn channel_independence() {
        let mut dirty = DirtySet::<u32>::new();

        dirty.mark(1, LAYOUT);
        dirty.mark(2, PAINT);

        assert!(dirty.is_dirty(1, LAYOUT));
        assert!(!dirty.is_dirty(1, PAINT));
        assert!(!dirty.is_dirty(2, LAYOUT));
        assert!(dirty.is_dirty(2, PAINT));
    }

    #[test]
    fn drain_clears_channel() {
        let mut dirty = DirtySet::<u32>::new();

        dirty.mark(1, LAYOUT);
        dirty.mark(2, LAYOUT);
        dirty.mark(1, PAINT);

        let drained: Vec<_> = dirty.drain(LAYOUT).collect();
        assert_eq!(drained.len(), 2);
        assert!(!dirty.has_dirty(LAYOUT));
        assert!(dirty.has_dirty(PAINT));
    }

    #[test]
    fn clear_specific_channel() {
        let mut dirty = DirtySet::<u32>::new();

        dirty.mark(1, LAYOUT);
        dirty.mark(1, PAINT);

        dirty.clear(LAYOUT);
        assert!(!dirty.has_dirty(LAYOUT));
        assert!(dirty.has_dirty(PAINT));
    }

    #[test]
    fn clear_all() {
        let mut dirty = DirtySet::<u32>::new();

        dirty.mark(1, LAYOUT);
        dirty.mark(2, PAINT);

        dirty.clear_all();
        assert!(dirty.is_empty());
    }

    #[test]
    fn remove_key_from_all_channels() {
        let mut dirty = DirtySet::<u32>::new();

        dirty.mark(1, LAYOUT);
        dirty.mark(1, PAINT);
        dirty.mark(2, LAYOUT);

        dirty.remove_key(1);
        assert!(!dirty.is_dirty(1, LAYOUT));
        assert!(!dirty.is_dirty(1, PAINT));
        assert!(dirty.is_dirty(2, LAYOUT));
    }

    #[test]
    fn generation_increments() {
        let mut dirty = DirtySet::<u32>::new();
        let initial = dirty.generation();

        dirty.mark(1, LAYOUT);
        assert_eq!(dirty.generation(), initial + 1);

        dirty.mark(2, LAYOUT);
        assert_eq!(dirty.generation(), initial + 2);

        let _ = dirty.drain(LAYOUT).count();
        assert_eq!(dirty.generation(), initial + 3);

        dirty.clear(PAINT);
        assert_eq!(dirty.generation(), initial + 4);
    }

    #[test]
    fn len_and_iter() {
        let mut dirty = DirtySet::<u32>::new();

        dirty.mark(1, LAYOUT);
        dirty.mark(2, LAYOUT);
        dirty.mark(3, LAYOUT);

        assert_eq!(dirty.len(LAYOUT), 3);
        assert_eq!(dirty.len(PAINT), 0);

        let keys: Vec<_> = dirty.iter(LAYOUT).collect();
        assert_eq!(keys.len(), 3);
    }
}
