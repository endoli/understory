// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dependency graph for dirty tracking.

use alloc::vec::Vec;
use core::fmt;
use core::hash::Hash;

use hashbrown::{HashMap, HashSet};

use crate::channel::{Channel, ChannelSet};
use crate::scratch::TraversalScratch;

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
/// - `K`: The key type, typically a node identifier. Must be `Copy + Eq + Hash`.
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
    K: Copy + Eq + Hash,
{
    /// Forward edges: (from, channel) -> set of keys `from` depends on.
    forward: HashMap<(K, Channel), HashSet<K>>,
    /// Reverse edges: (to, channel) -> set of keys that depend on `to`.
    reverse: HashMap<(K, Channel), HashSet<K>>,
    /// Cached channels where `key` has any dependencies.
    forward_channels: HashMap<K, ChannelSet>,
    /// Cached channels where `key` has any dependents.
    reverse_channels: HashMap<K, ChannelSet>,
}

impl<K> Default for DirtyGraph<K>
where
    K: Copy + Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K> DirtyGraph<K>
where
    K: Copy + Eq + Hash,
{
    /// Creates a new empty dependency graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            forward: HashMap::new(),
            reverse: HashMap::new(),
            forward_channels: HashMap::new(),
            reverse_channels: HashMap::new(),
        }
    }

    /// Returns `true` if the graph has no dependencies.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
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

        // Add forward edge: (from, channel) -> to
        let inserted = self.forward.entry((from, channel)).or_default().insert(to);

        if inserted {
            // Add reverse edge: (to, channel) <- from
            self.reverse.entry((to, channel)).or_default().insert(from);

            self.forward_channels
                .entry(from)
                .and_modify(|set| set.insert(channel))
                .or_insert_with(|| channel.into_set());
            self.reverse_channels
                .entry(to)
                .and_modify(|set| set.insert(channel))
                .or_insert_with(|| channel.into_set());
        }

        Ok(inserted)
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
            if let Some(deps) = self.forward.get(&(current, channel)) {
                stack.extend(deps.iter().copied());
            }
        }

        false
    }

    /// Removes a dependency: `from` no longer depends on `to` in the given channel.
    ///
    /// Returns `true` if the dependency existed and was removed.
    pub fn remove_dependency(&mut self, from: K, to: K, channel: Channel) -> bool {
        let mut removed = false;
        let mut removed_forward_entry = false;
        if let Some(deps) = self.forward.get_mut(&(from, channel)) {
            removed = deps.remove(&to);
            if removed && deps.is_empty() {
                self.forward.remove(&(from, channel));
                removed_forward_entry = true;
            }
        }

        if !removed {
            return false;
        }

        let mut removed_reverse_entry = false;
        if let Some(dependents) = self.reverse.get_mut(&(to, channel)) {
            dependents.remove(&from);
            if dependents.is_empty() {
                self.reverse.remove(&(to, channel));
                removed_reverse_entry = true;
            }
        }

        if removed_forward_entry && let Some(set) = self.forward_channels.get_mut(&from) {
            set.remove(channel);
            if set.is_empty() {
                self.forward_channels.remove(&from);
            }
        }
        if removed_reverse_entry && let Some(set) = self.reverse_channels.get_mut(&to) {
            set.remove(channel);
            if set.is_empty() {
                self.reverse_channels.remove(&to);
            }
        }

        true
    }

    /// Replaces all direct dependencies of `from` in `channel`.
    ///
    /// This is a batch convenience for the common “set all deps” workflow.
    ///
    /// - Existing dependencies of `from` in `channel` are removed.
    /// - Each key in `to` is added as a dependency of `from`.
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
        let old: HashSet<K> = self
            .forward
            .get(&(from, channel))
            .cloned()
            .unwrap_or_default();

        let new: HashSet<K> = to.into_iter().collect();
        if old == new {
            return Ok(false);
        }

        // Remove previous deps.
        for dep in old.iter().copied() {
            let _ = self.remove_dependency(from, dep, channel);
        }

        // Add new deps, rolling back on error.
        let mut added: Vec<K> = Vec::new();
        for dep in new.iter().copied() {
            match self.add_dependency(from, dep, channel, handling) {
                Ok(true) => added.push(dep),
                Ok(false) => {}
                Err(e) => {
                    // Remove any deps we just added.
                    for d in added {
                        let _ = self.remove_dependency(from, d, channel);
                    }
                    // Restore old deps without cycle checks.
                    for d in old.iter().copied() {
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
        // Remove forward edges: (key, channel) -> deps
        if let Some(channels) = self.forward_channels.remove(&key) {
            for channel in channels {
                if let Some(deps) = self.forward.remove(&(key, channel)) {
                    for dep in deps {
                        let Some(dependents) = self.reverse.get_mut(&(dep, channel)) else {
                            continue;
                        };
                        dependents.remove(&key);
                        if dependents.is_empty() {
                            self.reverse.remove(&(dep, channel));
                            if let Some(set) = self.reverse_channels.get_mut(&dep) {
                                set.remove(channel);
                                if set.is_empty() {
                                    self.reverse_channels.remove(&dep);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Remove reverse edges: (key, channel) <- dependents
        if let Some(channels) = self.reverse_channels.remove(&key) {
            for channel in channels {
                if let Some(dependents) = self.reverse.remove(&(key, channel)) {
                    for dependent in dependents {
                        let Some(deps) = self.forward.get_mut(&(dependent, channel)) else {
                            continue;
                        };
                        deps.remove(&key);
                        if deps.is_empty() {
                            self.forward.remove(&(dependent, channel));
                            if let Some(set) = self.forward_channels.get_mut(&dependent) {
                                set.remove(channel);
                                if set.is_empty() {
                                    self.forward_channels.remove(&dependent);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Returns an iterator over the direct dependencies of `key` in the given channel.
    ///
    /// These are the keys that `key` depends on (i.e., if they become dirty,
    /// `key` should be recomputed).
    ///
    /// The iteration order is not specified and may vary across runs or platforms.
    pub fn dependencies(&self, key: K, channel: Channel) -> impl Iterator<Item = K> + '_ {
        self.forward
            .get(&(key, channel))
            .into_iter()
            .flat_map(|deps| deps.iter().copied())
    }

    /// Returns an iterator over the direct dependents of `key` in the given channel.
    ///
    /// These are the keys that depend on `key` (i.e., if `key` becomes dirty,
    /// they should be recomputed).
    ///
    /// The iteration order is not specified and may vary across runs or platforms.
    pub fn dependents(&self, key: K, channel: Channel) -> impl Iterator<Item = K> + '_ {
        self.reverse
            .get(&(key, channel))
            .into_iter()
            .flat_map(|deps| deps.iter().copied())
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
        self.forward_channels
            .get(&key)
            .copied()
            .unwrap_or(ChannelSet::EMPTY)
    }

    /// Returns the set of channels in which `key` has any dependents.
    #[must_use]
    pub fn dependent_channels(&self, key: K) -> ChannelSet {
        self.reverse_channels
            .get(&key)
            .copied()
            .unwrap_or(ChannelSet::EMPTY)
    }

    /// Returns `true` if `key` has any dependencies in the given channel.
    #[must_use]
    pub fn has_dependencies(&self, key: K, channel: Channel) -> bool {
        self.forward
            .get(&(key, channel))
            .is_some_and(|deps| !deps.is_empty())
    }

    /// Returns `true` if `key` has any dependents in the given channel.
    #[must_use]
    pub fn has_dependents(&self, key: K, channel: Channel) -> bool {
        self.reverse
            .get(&(key, channel))
            .is_some_and(|deps| !deps.is_empty())
    }

    /// Returns the in-degree of `key` in the given channel.
    ///
    /// The in-degree is the number of keys that `key` depends on.
    #[must_use]
    pub fn in_degree(&self, key: K, channel: Channel) -> usize {
        self.forward
            .get(&(key, channel))
            .map(HashSet::len)
            .unwrap_or(0)
    }

    /// Returns the out-degree of `key` in the given channel.
    ///
    /// The out-degree is the number of keys that depend on `key`.
    #[must_use]
    pub fn out_degree(&self, key: K, channel: Channel) -> usize {
        self.reverse
            .get(&(key, channel))
            .map(HashSet::len)
            .unwrap_or(0)
    }

    /// Returns an iterator over all unique keys that have dependencies or dependents.
    ///
    /// Each key is yielded at most once, even if it appears in both the forward
    /// and reverse edge maps.
    ///
    /// The iteration order is not specified and may vary across runs or platforms.
    pub fn keys(&self) -> impl Iterator<Item = K> + '_ {
        // Use a HashSet to deduplicate keys that appear in both maps.
        let mut seen = HashSet::new();
        self.forward_channels
            .keys()
            .chain(self.reverse_channels.keys())
            .copied()
            .filter(move |&k| seen.insert(k))
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
    K: Copy + Eq + Hash,
{
    graph: &'a DirtyGraph<K>,
    channel: Channel,
    visited: HashSet<K>,
    stack: Vec<K>,
}

impl<'a, K> TransitiveDependentsIter<'a, K>
where
    K: Copy + Eq + Hash,
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
    K: Copy + Eq + Hash,
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
