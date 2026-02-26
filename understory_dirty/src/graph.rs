// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dependency graph for dirty tracking.

use alloc::vec::Vec;
use core::fmt;
use core::hash::Hash;

use hashbrown::HashSet;

use crate::channel::{Channel, ChannelSet};
use crate::drain::DenseKey;
use crate::scratch::TraversalScratch;

/// Maximum number of channels supported (64).
const MAX_CHANNELS: usize = 64;

/// Error returned when a cycle would be created by adding a dependency.
#[derive(Clone, PartialEq, Eq)]
pub struct CycleError<K> {
    /// The key that would depend on another.
    pub from: K,
    /// The key that would be depended upon.
    pub to: K,
    /// The channel where the cycle would occur.
    pub channel: Channel,
}

impl<K: fmt::Debug> fmt::Debug for CycleError<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CycleError {{ from: {:?}, to: {:?}, channel: {:?} }}",
            self.from, self.to, self.channel
        )
    }
}

impl<K: fmt::Debug> fmt::Display for CycleError<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "adding dependency {:?} -> {:?} in {:?} would create a cycle",
            self.from, self.to, self.channel
        )
    }
}

impl<K: fmt::Debug> core::error::Error for CycleError<K> {}

/// How to handle cycle detection when adding dependencies.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum CycleHandling {
    /// Panic in debug builds, silently ignore in release builds.
    ///
    /// This is the default behavior: catches bugs during development with
    /// zero cost in release builds.
    #[default]
    DebugAssert,
    /// Return an error if a cycle would be created.
    Error,
    /// Silently ignore the dependency if it would create a cycle.
    Ignore,
    /// Allow cycles (skip cycle detection entirely).
    ///
    /// This is useful when the caller guarantees no cycles, or when cycles
    /// are intentionally allowed. This has a small performance benefit as
    /// no reachability check is performed.
    Allow,
}

/// Dependency graph: "A depends on B" edges per channel.
///
/// `DirtyGraph` stores bidirectional dependency edges, allowing O(1) queries
/// for both "what does A depend on?" and "what depends on A?". Dependencies
/// are stored per-channel, so layout dependencies can be independent of
/// paint dependencies.
///
/// # Type Parameters
///
/// - `K`: The key type, typically a node identifier. Must be `Copy + Eq + Hash + DenseKey`.
///   If your natural key is owned/structured, see [`intern::Interner`](crate::intern::Interner).
///
/// # Example
///
/// ```
/// use understory_dirty::{Channel, CycleHandling, DirtyGraph};
///
/// const LAYOUT: Channel = Channel::new(0);
///
/// let mut graph = DirtyGraph::<u32>::new();
///
/// // Node 2 depends on node 1 for layout
/// graph.add_dependency(2, 1, LAYOUT, CycleHandling::Error).unwrap();
/// // Node 3 depends on node 2 for layout
/// graph.add_dependency(3, 2, LAYOUT, CycleHandling::Error).unwrap();
///
/// // Query dependencies
/// assert!(graph.dependencies(2, LAYOUT).any(|k| k == 1));
/// assert!(graph.dependents(1, LAYOUT).any(|k| k == 2));
///
/// // Transitive dependents of node 1: [2, 3]
/// let transitive: Vec<_> = graph.transitive_dependents(1, LAYOUT).collect();
/// assert!(transitive.contains(&2));
/// assert!(transitive.contains(&3));
/// ```
///
/// # See Also
///
/// - [`DirtyTracker`](crate::DirtyTracker): Convenience wrapper combining graph + set.
/// - [`CycleHandling`]: Cycle policy used by [`add_dependency`](Self::add_dependency).
/// - [`DrainSorted`](crate::DrainSorted): Drains dirty keys in dependency order.
#[derive(Debug, Clone)]
pub struct DirtyGraph<K>
where
    K: Copy + Eq + Hash + DenseKey,
{
    /// Forward edges: forward[channel][key.index()] -> keys that `key` depends on.
    forward: [Vec<Vec<K>>; MAX_CHANNELS],
    /// Reverse edges: reverse[channel][key.index()] -> keys that depend on `key`.
    reverse: [Vec<Vec<K>>; MAX_CHANNELS],
    /// Cached channels where `key` has any dependencies.
    forward_channels: Vec<ChannelSet>,
    /// Cached channels where `key` has any dependents.
    reverse_channels: Vec<ChannelSet>,
}

impl<K> Default for DirtyGraph<K>
where
    K: Copy + Eq + Hash + DenseKey,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Ensures `vec` has at least `idx + 1` elements, filling with defaults.
#[inline]
fn grow<T: Default>(vec: &mut Vec<T>, idx: usize) {
    if idx >= vec.len() {
        vec.resize_with(idx + 1, T::default);
    }
}

/// Ensures the channel-set vec has at least `idx + 1` elements.
#[inline]
fn grow_channels(vec: &mut Vec<ChannelSet>, idx: usize) {
    if idx >= vec.len() {
        vec.resize(idx + 1, ChannelSet::EMPTY);
    }
}

impl<K> DirtyGraph<K>
where
    K: Copy + Eq + Hash + DenseKey,
{
    /// Creates a new empty dependency graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            forward: core::array::from_fn(|_| Vec::new()),
            reverse: core::array::from_fn(|_| Vec::new()),
            forward_channels: Vec::new(),
            reverse_channels: Vec::new(),
        }
    }

    /// Returns `true` if the graph has no dependencies.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.forward_channels.iter().all(|cs| cs.is_empty())
    }

    /// Adds a dependency: `from` depends on `to` in the given channel.
    ///
    /// When `to` becomes dirty, `from` should be recomputed (in that channel).
    ///
    /// # Cycle Handling
    ///
    /// The `handling` parameter controls behavior when adding this dependency
    /// would create a cycle:
    ///
    /// - [`CycleHandling::DebugAssert`]: Panics in debug builds, ignores in release.
    /// - [`CycleHandling::Error`]: Returns `Err(CycleError)`.
    /// - [`CycleHandling::Ignore`]: Silently ignores the dependency.
    /// - [`CycleHandling::Allow`]: Skips cycle detection entirely.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if the dependency was newly added.
    /// - `Ok(false)` if the dependency already existed.
    /// - `Err(CycleError)` if a cycle would be created and `handling` is `Error`.
    ///
    /// # See Also
    ///
    /// - [`CycleHandling`]: How cycles are treated.
    /// - [`CycleError`]: Returned when `handling` is [`CycleHandling::Error`].
    pub fn add_dependency(
        &mut self,
        from: K,
        to: K,
        channel: Channel,
        handling: CycleHandling,
    ) -> Result<bool, CycleError<K>> {
        // Self-dependency is a trivial cycle
        if from == to {
            return self.handle_cycle(from, to, channel, handling);
        }

        // Check for cycles unless explicitly allowed
        if handling != CycleHandling::Allow && self.would_create_cycle(from, to, channel) {
            return self.handle_cycle(from, to, channel, handling);
        }

        let ch = channel.index() as usize;
        let from_idx = from.index();
        let to_idx = to.index();

        // Dedup: check if already present
        let fwd = &mut self.forward[ch];
        grow(fwd, from_idx);
        if fwd[from_idx].contains(&to) {
            return Ok(false);
        }

        // Add forward edge
        fwd[from_idx].push(to);

        // Add reverse edge
        let rev = &mut self.reverse[ch];
        grow(rev, to_idx);
        rev[to_idx].push(from);

        // Update channel sets
        grow_channels(&mut self.forward_channels, from_idx);
        self.forward_channels[from_idx].insert(channel);

        grow_channels(&mut self.reverse_channels, to_idx);
        self.reverse_channels[to_idx].insert(channel);

        Ok(true)
    }

    fn handle_cycle(
        &self,
        from: K,
        to: K,
        channel: Channel,
        handling: CycleHandling,
    ) -> Result<bool, CycleError<K>> {
        match handling {
            CycleHandling::DebugAssert => {
                debug_assert!(false, "adding dependency would create a cycle");
                Ok(false)
            }
            CycleHandling::Error => Err(CycleError { from, to, channel }),
            CycleHandling::Ignore | CycleHandling::Allow => Ok(false),
        }
    }

    /// Checks whether adding `from -> to` would create a cycle.
    ///
    /// This performs a DFS from `to` to see if `from` is reachable.
    fn would_create_cycle(&self, from: K, to: K, channel: Channel) -> bool {
        // A cycle would be created if `from` is reachable from `to`
        // (i.e., `to` already transitively depends on `from`)
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        stack.push(to);

        while let Some(current) = stack.pop() {
            if current == from {
                return true;
            }
            if !visited.insert(current) {
                continue;
            }

            // Follow forward edges from current
            let ch = channel.index() as usize;
            let fwd = &self.forward[ch];
            let idx = current.index();
            if idx < fwd.len() {
                stack.extend(fwd[idx].iter().copied());
            }
        }

        false
    }

    /// Removes a dependency: `from` no longer depends on `to` in the given channel.
    ///
    /// Returns `true` if the dependency existed and was removed.
    pub fn remove_dependency(&mut self, from: K, to: K, channel: Channel) -> bool {
        let ch = channel.index() as usize;
        let from_idx = from.index();
        let to_idx = to.index();

        // Remove forward edge
        let fwd = &mut self.forward[ch];
        let removed = if from_idx < fwd.len() {
            if let Some(pos) = fwd[from_idx].iter().position(|&k| k == to) {
                fwd[from_idx].swap_remove(pos);
                true
            } else {
                false
            }
        } else {
            false
        };

        if !removed {
            return false;
        }

        // Remove reverse edge
        let rev = &mut self.reverse[ch];
        if to_idx < rev.len()
            && let Some(pos) = rev[to_idx].iter().position(|&k| k == from)
        {
            rev[to_idx].swap_remove(pos);
        }

        // Update channel sets if the adjacency list became empty
        if fwd[from_idx].is_empty()
            && from_idx < self.forward_channels.len()
        {
            self.forward_channels[from_idx].remove(channel);
        }
        if to_idx < rev.len() && rev[to_idx].is_empty()
            && to_idx < self.reverse_channels.len()
        {
            self.reverse_channels[to_idx].remove(channel);
        }

        true
    }

    /// Replaces all direct dependencies of `from` in `channel`.
    ///
    /// This is a batch convenience for the common "set all deps" workflow.
    ///
    /// - Dependencies present in the old set but missing from `to` are removed.
    /// - Dependencies present in `to` but missing from the old set are added.
    /// - Dependencies present in both sets are left unchanged.
    /// - Duplicate keys in `to` are ignored.
    ///
    /// # Cycle Handling
    ///
    /// Cycle handling is applied while adding new dependencies. If adding a
    /// dependency returns `Err(CycleError)`, this method rolls back to the
    /// previous dependency set and returns the error.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if the dependency set changed.
    /// - `Ok(false)` if the dependency set was already equal to `to`.
    /// - `Err(CycleError)` if a cycle would be created and `handling` is `Error`
    ///   (in which case no changes are retained).
    pub fn replace_dependencies(
        &mut self,
        from: K,
        channel: Channel,
        to: impl IntoIterator<Item = K>,
        handling: CycleHandling,
    ) -> Result<bool, CycleError<K>> {
        // Collect new deps, deduplicating.
        let mut new_set: Vec<K> = Vec::new();
        for k in to {
            if !new_set.contains(&k) {
                new_set.push(k);
            }
        }

        let ch = channel.index() as usize;
        let from_idx = from.index();
        let fwd = &self.forward[ch];
        let old = if from_idx < fwd.len() {
            fwd[from_idx].as_slice()
        } else {
            &[]
        };

        let unchanged =
            old.len() == new_set.len() && old.iter().all(|dep| new_set.contains(dep));
        if unchanged {
            return Ok(false);
        }

        let mut to_remove: Vec<K> = Vec::new();
        for &dep in old {
            if !new_set.contains(&dep) {
                to_remove.push(dep);
            }
        }

        let mut to_add: Vec<K> = Vec::new();
        for &dep in &new_set {
            if !old.contains(&dep) {
                to_add.push(dep);
            }
        }

        // Remove stale edges first so cycle checks for additions observe the post-update graph.
        let mut removed: Vec<K> = Vec::new();
        for dep in to_remove.iter().copied() {
            if self.remove_dependency(from, dep, channel) {
                removed.push(dep);
            }
        }

        // Add new edges and roll back diff mutations on error.
        let mut added: Vec<K> = Vec::new();
        for dep in to_add.iter().copied() {
            match self.add_dependency(from, dep, channel, handling) {
                Ok(true) => added.push(dep),
                Ok(false) => {}
                Err(e) => {
                    for d in added {
                        let _ = self.remove_dependency(from, d, channel);
                    }
                    for d in removed {
                        let _ = self.add_dependency(from, d, channel, CycleHandling::Allow);
                    }
                    return Err(e);
                }
            }
        }

        Ok(true)
    }

    /// Removes a key entirely from the graph.
    ///
    /// This removes all dependencies involving `key`, both as a dependent
    /// and as a dependency.
    pub fn remove_key(&mut self, key: K) {
        let key_idx = key.index();

        // Remove forward edges: for each channel where key has dependencies
        let fwd_channels = if key_idx < self.forward_channels.len() {
            self.forward_channels[key_idx]
        } else {
            ChannelSet::EMPTY
        };
        for channel in fwd_channels {
            let ch = channel.index() as usize;

            // Collect deps before mutating
            let deps: Vec<K> = if key_idx < self.forward[ch].len() {
                core::mem::take(&mut self.forward[ch][key_idx])
            } else {
                Vec::new()
            };

            // Remove reverse entries for each dep
            for dep in deps {
                let dep_idx = dep.index();
                let rev = &mut self.reverse[ch];
                if dep_idx < rev.len() {
                    if let Some(pos) = rev[dep_idx].iter().position(|&k| k == key) {
                        rev[dep_idx].swap_remove(pos);
                    }
                    if rev[dep_idx].is_empty() && dep_idx < self.reverse_channels.len() {
                        self.reverse_channels[dep_idx].remove(channel);
                    }
                }
            }
        }
        if key_idx < self.forward_channels.len() {
            self.forward_channels[key_idx] = ChannelSet::EMPTY;
        }

        // Remove reverse edges: for each channel where key has dependents
        let rev_channels = if key_idx < self.reverse_channels.len() {
            self.reverse_channels[key_idx]
        } else {
            ChannelSet::EMPTY
        };
        for channel in rev_channels {
            let ch = channel.index() as usize;

            // Collect dependents before mutating
            let dependents: Vec<K> = if key_idx < self.reverse[ch].len() {
                core::mem::take(&mut self.reverse[ch][key_idx])
            } else {
                Vec::new()
            };

            // Remove forward entries for each dependent
            for dependent in dependents {
                let dep_idx = dependent.index();
                let fwd = &mut self.forward[ch];
                if dep_idx < fwd.len() {
                    if let Some(pos) = fwd[dep_idx].iter().position(|&k| k == key) {
                        fwd[dep_idx].swap_remove(pos);
                    }
                    if fwd[dep_idx].is_empty() && dep_idx < self.forward_channels.len() {
                        self.forward_channels[dep_idx].remove(channel);
                    }
                }
            }
        }
        if key_idx < self.reverse_channels.len() {
            self.reverse_channels[key_idx] = ChannelSet::EMPTY;
        }
    }

    /// Returns an iterator over the direct dependencies of `key` in the given channel.
    ///
    /// These are the keys that `key` depends on (i.e., if they become dirty,
    /// `key` should be recomputed).
    ///
    /// The iteration order is not specified and may vary across runs or platforms.
    #[inline]
    pub fn dependencies(&self, key: K, channel: Channel) -> impl Iterator<Item = K> + '_ {
        let ch = channel.index() as usize;
        let fwd = &self.forward[ch];
        let idx = key.index();
        let slice = if idx < fwd.len() {
            fwd[idx].as_slice()
        } else {
            &[]
        };
        slice.iter().copied()
    }

    /// Returns an iterator over the direct dependents of `key` in the given channel.
    ///
    /// These are the keys that depend on `key` (i.e., if `key` becomes dirty,
    /// they should be recomputed).
    ///
    /// The iteration order is not specified and may vary across runs or platforms.
    #[inline]
    pub fn dependents(&self, key: K, channel: Channel) -> impl Iterator<Item = K> + '_ {
        let ch = channel.index() as usize;
        let rev = &self.reverse[ch];
        let idx = key.index();
        let slice = if idx < rev.len() {
            rev[idx].as_slice()
        } else {
            &[]
        };
        slice.iter().copied()
    }

    /// Returns an iterator over all transitive dependents of `key` in the given channel.
    ///
    /// This performs a DFS traversal and yields all keys that directly or
    /// indirectly depend on `key`.
    ///
    /// The iteration order is not specified and may vary across runs or platforms.
    pub fn transitive_dependents(&self, key: K, channel: Channel) -> impl Iterator<Item = K> + '_ {
        TransitiveDependentsIter::new(self, key, channel)
    }

    /// Calls `f` for each transitive dependent of `key`, using reusable scratch buffers.
    ///
    /// This is equivalent to iterating [`transitive_dependents`](Self::transitive_dependents),
    /// but allows the caller to reuse allocations across traversals.
    ///
    /// The iteration order is not specified and may vary across runs or platforms.
    ///
    /// # See Also
    ///
    /// - [`TraversalScratch`]: Reusable storage for this traversal.
    /// - [`EagerPolicy::propagate_with_scratch`](crate::EagerPolicy::propagate_with_scratch): Uses this helper.
    pub fn for_each_transitive_dependent(
        &self,
        key: K,
        channel: Channel,
        scratch: &mut TraversalScratch<K>,
        mut f: impl FnMut(K),
    ) {
        scratch.reset();
        scratch.stack.extend(self.dependents(key, channel));

        while let Some(next) = scratch.stack.pop() {
            if scratch.visited.insert(next) {
                f(next);
                scratch.stack.extend(self.dependents(next, channel));
            }
        }
    }

    /// Returns the set of channels in which `key` has any dependencies.
    #[must_use]
    pub fn dependency_channels(&self, key: K) -> ChannelSet {
        let idx = key.index();
        if idx < self.forward_channels.len() {
            self.forward_channels[idx]
        } else {
            ChannelSet::EMPTY
        }
    }

    /// Returns the set of channels in which `key` has any dependents.
    #[must_use]
    pub fn dependent_channels(&self, key: K) -> ChannelSet {
        let idx = key.index();
        if idx < self.reverse_channels.len() {
            self.reverse_channels[idx]
        } else {
            ChannelSet::EMPTY
        }
    }

    /// Returns `true` if `key` has any dependencies in the given channel.
    #[inline]
    #[must_use]
    pub fn has_dependencies(&self, key: K, channel: Channel) -> bool {
        let ch = channel.index() as usize;
        let fwd = &self.forward[ch];
        let idx = key.index();
        idx < fwd.len() && !fwd[idx].is_empty()
    }

    /// Returns `true` if `key` has any dependents in the given channel.
    #[must_use]
    pub fn has_dependents(&self, key: K, channel: Channel) -> bool {
        let ch = channel.index() as usize;
        let rev = &self.reverse[ch];
        let idx = key.index();
        idx < rev.len() && !rev[idx].is_empty()
    }

    /// Returns the in-degree of `key` in the given channel.
    ///
    /// The in-degree is the number of keys that `key` depends on.
    #[must_use]
    pub fn in_degree(&self, key: K, channel: Channel) -> usize {
        let ch = channel.index() as usize;
        let fwd = &self.forward[ch];
        let idx = key.index();
        if idx < fwd.len() { fwd[idx].len() } else { 0 }
    }

    /// Returns the out-degree of `key` in the given channel.
    ///
    /// The out-degree is the number of keys that depend on `key`.
    #[must_use]
    pub fn out_degree(&self, key: K, channel: Channel) -> usize {
        let ch = channel.index() as usize;
        let rev = &self.reverse[ch];
        let idx = key.index();
        if idx < rev.len() { rev[idx].len() } else { 0 }
    }

    /// Returns an iterator over all unique keys that have dependencies or dependents.
    ///
    /// Each key is yielded at most once, even if it appears in both the forward
    /// and reverse edge maps.
    ///
    /// The iteration order is not specified and may vary across runs or platforms.
    pub fn keys(&self) -> impl Iterator<Item = K> + '_ {
        let mut seen = HashSet::new();
        let max_len = self.forward_channels.len().max(self.reverse_channels.len());
        // Iterate both channel vecs looking for non-empty entries.
        // We need to reconstruct keys from indices — but DenseKey only provides
        // index(), not from_index(). Instead, walk all adjacency lists.
        let mut all_keys: Vec<K> = Vec::new();
        for ch in 0..MAX_CHANNELS {
            for inner in &self.forward[ch] {
                for &k in inner {
                    if seen.insert(k) {
                        all_keys.push(k);
                    }
                }
            }
            for inner in &self.reverse[ch] {
                for &k in inner {
                    if seen.insert(k) {
                        all_keys.push(k);
                    }
                }
            }
        }
        // Also include keys that appear as "from" or "to" — they are in the
        // adjacency lists of their counterparts, so the above already covers them.
        // But keys with forward entries need to include themselves too.
        // Actually, keys only appear in adjacency lists of other keys, not
        // themselves. A key with forward edges appears as "from" — we need to
        // find it. The forward_channels/reverse_channels vecs tell us which
        // indices have entries, but we can't reconstruct K from an index.
        //
        // However, we can collect all K values that appear anywhere in the
        // adjacency lists. A key with only forward edges (from) appears in
        // the reverse list of its dependencies, and vice versa. So iterating
        // all adjacency entries covers all keys.
        let _ = max_len;
        all_keys.into_iter()
    }

    /// Collects [`keys`](Self::keys) into a `Vec`.
    ///
    /// The order is not specified and may vary across runs or platforms.
    #[must_use]
    pub fn keys_vec(&self) -> Vec<K> {
        self.keys().collect()
    }
}

/// Iterator over transitive dependents using DFS.
struct TransitiveDependentsIter<'a, K>
where
    K: Copy + Eq + Hash + DenseKey,
{
    graph: &'a DirtyGraph<K>,
    channel: Channel,
    visited: HashSet<K>,
    stack: Vec<K>,
}

impl<'a, K> TransitiveDependentsIter<'a, K>
where
    K: Copy + Eq + Hash + DenseKey,
{
    fn new(graph: &'a DirtyGraph<K>, start: K, channel: Channel) -> Self {
        let mut iter = Self {
            graph,
            channel,
            visited: HashSet::new(),
            stack: Vec::new(),
        };
        // Initialize with direct dependents
        iter.stack.extend(graph.dependents(start, channel));
        iter
    }
}

impl<K> Iterator for TransitiveDependentsIter<'_, K>
where
    K: Copy + Eq + Hash + DenseKey,
{
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(key) = self.stack.pop() {
            if self.visited.insert(key) {
                // Push dependents of this key
                self.stack.extend(self.graph.dependents(key, self.channel));
                return Some(key);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    const LAYOUT: Channel = Channel::new(0);
    const PAINT: Channel = Channel::new(1);
    const A11Y: Channel = Channel::new(2);

    #[test]
    fn add_and_query_dependencies() {
        let mut graph = DirtyGraph::<u32>::new();

        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();

        // Node 2 depends on node 1
        assert!(graph.dependencies(2, LAYOUT).any(|k| k == 1));
        // Node 1 has dependent node 2
        assert!(graph.dependents(1, LAYOUT).any(|k| k == 2));
        // Node 2 has dependent node 3
        assert!(graph.dependents(2, LAYOUT).any(|k| k == 3));
    }

    #[test]
    fn replace_dependencies_updates_in_place() {
        let mut graph = DirtyGraph::<u32>::new();
        graph
            .add_dependency(10, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(10, 2, LAYOUT, CycleHandling::Error)
            .unwrap();

        let changed = graph
            .replace_dependencies(10, LAYOUT, [3, 4], CycleHandling::Error)
            .unwrap();
        assert!(changed);

        let deps: Vec<_> = graph.dependencies(10, LAYOUT).collect();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&3));
        assert!(deps.contains(&4));
        assert!(!deps.contains(&1));
        assert!(!deps.contains(&2));
    }

    #[test]
    fn replace_dependencies_rolls_back_on_cycle_error() {
        let mut graph = DirtyGraph::<u32>::new();
        // 2 depends on 1.
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        // 1 depends on 3 (old dependency set for 1).
        graph
            .add_dependency(1, 3, LAYOUT, CycleHandling::Error)
            .unwrap();

        // Replacing deps for 1 with [2] would create a 1 <-> 2 cycle.
        let err = graph
            .replace_dependencies(1, LAYOUT, [2], CycleHandling::Error)
            .unwrap_err();
        assert_eq!(err.from, 1);
        assert_eq!(err.to, 2);

        // Old deps of 1 are restored.
        let deps: Vec<_> = graph.dependencies(1, LAYOUT).collect();
        assert_eq!(deps, vec![3]);
        assert!(!graph.dependencies(1, LAYOUT).any(|k| k == 2));

        // Unrelated edges are unchanged.
        assert!(graph.dependencies(2, LAYOUT).any(|k| k == 1));
    }

    #[test]
    fn replace_dependencies_noop_when_set_unchanged_returns_false() {
        let mut graph = DirtyGraph::<u32>::new();
        graph
            .add_dependency(10, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(10, 2, LAYOUT, CycleHandling::Error)
            .unwrap();

        // Duplicates and ordering differences should not count as a change.
        let changed = graph
            .replace_dependencies(10, LAYOUT, [2, 1, 2], CycleHandling::Error)
            .unwrap();
        assert!(!changed);

        let deps: Vec<_> = graph.dependencies(10, LAYOUT).collect();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&1));
        assert!(deps.contains(&2));
    }

    #[test]
    fn replace_dependencies_rolls_back_mixed_delta_on_cycle_error() {
        let mut graph = DirtyGraph::<u32>::new();
        // Make adding 1 -> 2 a cycle by first adding 2 -> 1.
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        // Old deps for 1 are {3, 4}.
        graph
            .add_dependency(1, 3, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(1, 4, LAYOUT, CycleHandling::Error)
            .unwrap();

        // New set would be {4, 2}: remove 3, keep 4, add 2 (cycle).
        let err = graph
            .replace_dependencies(1, LAYOUT, [4, 2], CycleHandling::Error)
            .unwrap_err();
        assert_eq!(err.from, 1);
        assert_eq!(err.to, 2);

        // Old set is restored after failed mixed-delta update.
        let deps: Vec<_> = graph.dependencies(1, LAYOUT).collect();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&3));
        assert!(deps.contains(&4));
        assert!(!deps.contains(&2));
    }

    #[test]
    fn replace_dependencies_is_channel_scoped() {
        let mut graph = DirtyGraph::<u32>::new();
        graph
            .add_dependency(7, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(7, 9, PAINT, CycleHandling::Error)
            .unwrap();

        graph
            .replace_dependencies(7, LAYOUT, [2, 3], CycleHandling::Error)
            .unwrap();

        let layout: Vec<_> = graph.dependencies(7, LAYOUT).collect();
        assert_eq!(layout.len(), 2);
        assert!(layout.contains(&2));
        assert!(layout.contains(&3));
        assert!(!layout.contains(&1));

        // PAINT dependencies are unchanged.
        let paint: Vec<_> = graph.dependencies(7, PAINT).collect();
        assert_eq!(paint, vec![9]);
    }

    #[test]
    fn cycle_detection_error() {
        let mut graph = DirtyGraph::<u32>::new();

        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();

        // Adding 1 -> 3 would create a cycle: 1 -> 3 -> 2 -> 1
        let result = graph.add_dependency(1, 3, LAYOUT, CycleHandling::Error);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.from, 1);
        assert_eq!(err.to, 3);
    }

    #[test]
    fn self_dependency_is_cycle() {
        let mut graph = DirtyGraph::<u32>::new();

        let result = graph.add_dependency(1, 1, LAYOUT, CycleHandling::Error);
        assert!(result.is_err());
    }

    #[test]
    fn cycle_ignore() {
        let mut graph = DirtyGraph::<u32>::new();

        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Ignore)
            .unwrap();

        // Self-cycle is silently ignored
        let result = graph.add_dependency(1, 1, LAYOUT, CycleHandling::Ignore);
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Returns false because nothing was added
    }

    #[test]
    fn cycle_allow() {
        let mut graph = DirtyGraph::<u32>::new();

        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Allow)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Allow)
            .unwrap();

        // Cycle is allowed
        let result = graph.add_dependency(1, 3, LAYOUT, CycleHandling::Allow);
        assert!(result.is_ok());
        assert!(result.unwrap()); // Edge was added
    }

    #[test]
    fn remove_dependency() {
        let mut graph = DirtyGraph::<u32>::new();

        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        assert!(graph.dependencies(2, LAYOUT).any(|k| k == 1));

        let removed = graph.remove_dependency(2, 1, LAYOUT);
        assert!(removed);
        assert!(!graph.dependencies(2, LAYOUT).any(|k| k == 1));

        // Removing again returns false
        assert!(!graph.remove_dependency(2, 1, LAYOUT));
    }

    #[test]
    fn remove_key() {
        let mut graph = DirtyGraph::<u32>::new();

        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(2, 1, PAINT, CycleHandling::Error)
            .unwrap();

        graph.remove_key(2);

        // Node 2's dependencies are gone
        assert!(!graph.dependencies(2, LAYOUT).any(|_| true));
        // Node 1's dependents are gone
        assert!(!graph.dependents(1, LAYOUT).any(|_| true));
        // Node 3's dependencies are gone
        assert!(!graph.dependencies(3, LAYOUT).any(|_| true));
    }

    #[test]
    fn transitive_dependents() {
        let mut graph = DirtyGraph::<u32>::new();

        // 1 <- 2 <- 3
        //      ^
        //      |
        //      4
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(4, 2, LAYOUT, CycleHandling::Error)
            .unwrap();

        let transitive: Vec<_> = graph.transitive_dependents(1, LAYOUT).collect();
        assert_eq!(transitive.len(), 3);
        assert!(transitive.contains(&2));
        assert!(transitive.contains(&3));
        assert!(transitive.contains(&4));
    }

    #[test]
    fn channel_independence() {
        let mut graph = DirtyGraph::<u32>::new();

        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();

        assert!(graph.has_dependencies(2, LAYOUT));
        assert!(!graph.has_dependencies(2, PAINT));
        assert!(graph.has_dependents(1, LAYOUT));
        assert!(!graph.has_dependents(1, PAINT));
    }

    #[test]
    fn in_out_degree() {
        let mut graph = DirtyGraph::<u32>::new();

        // 3 depends on 1 and 2
        graph
            .add_dependency(3, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();

        // Node 3 has in-degree 2 (depends on 2 nodes)
        assert_eq!(graph.in_degree(3, LAYOUT), 2);
        // Node 1 has out-degree 1 (1 node depends on it)
        assert_eq!(graph.out_degree(1, LAYOUT), 1);
    }

    #[test]
    fn dependency_channels() {
        let mut graph = DirtyGraph::<u32>::new();

        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(2, 1, PAINT, CycleHandling::Error)
            .unwrap();

        let channels = graph.dependency_channels(2);
        assert!(channels.contains(LAYOUT));
        assert!(channels.contains(PAINT));
        assert!(!channels.contains(A11Y));
    }

    #[test]
    fn keys_and_keys_vec_are_unique() {
        let mut graph = DirtyGraph::<u32>::new();
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();

        let keys: Vec<_> = graph.keys().collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&1));
        assert!(keys.contains(&2));

        let keys_vec = graph.keys_vec();
        assert_eq!(keys_vec.len(), 2);
        assert!(keys_vec.contains(&1));
        assert!(keys_vec.contains(&2));
    }
}
