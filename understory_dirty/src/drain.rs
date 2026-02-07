// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Topologically sorted drain iterator.

use alloc::collections::BinaryHeap;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::cmp::Reverse;
use core::hash::Hash;

use hashbrown::HashMap;
use hashbrown::hash_map::Entry;

use crate::channel::Channel;
use crate::graph::DirtyGraph;

/// Indicates whether a drain finished normally or stalled due to a cycle.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DrainCompletion {
    /// All reachable keys were yielded.
    Complete,
    /// The drain stalled: some keys remained with non-zero in-degree (cycle).
    Stalled {
        /// Number of keys that could not be yielded.
        remaining: usize,
    },
}

/// Iterator that yields dirty keys in topological order.
///
/// Uses Kahn's algorithm to yield dependencies before their dependents.
/// This ensures that when processing the dirty set, a node is only processed
/// after all of its dependencies have been processed.
///
/// # Algorithm
///
/// 1. Collect all dirty keys and their in-degrees (within the dirty subset).
/// 2. Initialize a queue with keys that have no dirty dependencies.
/// 3. Repeatedly dequeue a key, yield it, and decrement in-degrees of its
///    dirty dependents. When a dependent's in-degree reaches zero, enqueue it.
///
/// # Performance
///
/// - Time complexity: O(V + E) where V is the number of dirty keys and E is
///   the number of edges between them.
/// - Space complexity: O(V) for the in-degree map and queue.
///
/// # Important Notes
///
/// - **Duplicates**: If `dirty_keys` contains duplicates, they are deduplicated
///   internally. Each key is yielded at most once.
/// - **Cycles**: This iterator assumes the dependency subgraph induced by the
///   dirty keys is acyclic (a DAG). If cycles exist, keys involved in cycles
///   will never have their in-degree reach zero and will not be yielded.
///   You can detect this by exhausting the iterator and then checking
///   [`is_stalled`](Self::is_stalled) / [`completion`](Self::completion), or by
///   using [`collect_with_completion`](Self::collect_with_completion).
///   Use [`CycleHandling::Error`](crate::CycleHandling::Error) when building the graph
///   to prevent cycles, or ensure cycles are intentional.
/// - **Nondeterminism**: When multiple keys have in-degree zero simultaneously,
///   the order among them depends on hash iteration order and is not guaranteed
///   to be deterministic across runs or platforms.
///
/// # Example
///
/// ```
/// use understory_dirty::{drain_sorted, Channel, CycleHandling, DirtyGraph, DirtySet};
///
/// const LAYOUT: Channel = Channel::new(0);
///
/// let mut graph = DirtyGraph::<u32>::new();
/// // 1 <- 2 <- 3
/// graph.add_dependency(2, 1, LAYOUT, CycleHandling::Error).unwrap();
/// graph.add_dependency(3, 2, LAYOUT, CycleHandling::Error).unwrap();
///
/// let mut dirty = DirtySet::new();
/// dirty.mark(1, LAYOUT);
/// dirty.mark(2, LAYOUT);
/// dirty.mark(3, LAYOUT);
///
/// let sorted: Vec<_> = drain_sorted(&mut dirty, &graph, LAYOUT).collect();
/// assert_eq!(sorted, vec![1, 2, 3]); // Dependencies before dependents
/// ```
#[derive(Debug)]
pub struct DrainSorted<'a, K>
where
    K: Copy + Eq + Hash,
{
    graph: &'a DirtyGraph<K>,
    channel: Channel,
    /// Keys with zero in-degree, ready to yield.
    queue: VecDeque<K>,
    /// Remaining in-degree for each dirty key (within the dirty subset).
    in_degree: HashMap<K, usize>,
    stalled: bool,
}

/// Deterministic variant of [`DrainSorted`].
///
/// When multiple keys are simultaneously ready, this drain yields the smallest
/// key first (according to `Ord`).
#[derive(Debug)]
pub struct DrainSortedDeterministic<'a, K>
where
    K: Copy + Eq + Hash + Ord,
{
    graph: &'a DirtyGraph<K>,
    channel: Channel,
    /// Keys with zero in-degree, ready to yield (min-heap via `Reverse`).
    ready: BinaryHeap<Reverse<K>>,
    /// Remaining in-degree for each dirty key (within the dirty subset).
    in_degree: HashMap<K, usize>,
    stalled: bool,
}

impl<'a, K> DrainSorted<'a, K>
where
    K: Copy + Eq + Hash,
{
    pub(crate) fn from_iter_with_capacity<I>(
        dirty_keys: I,
        cap: usize,
        graph: &'a DirtyGraph<K>,
        channel: Channel,
    ) -> Self
    where
        I: Iterator<Item = K>,
    {
        // Deduplicate input keys via the in-degree map keys.
        let mut in_degree: HashMap<K, usize> = HashMap::with_capacity(cap);
        let mut unique_keys = Vec::with_capacity(cap);
        for key in dirty_keys {
            if let Entry::Vacant(e) = in_degree.entry(key) {
                e.insert(0);
                unique_keys.push(key);
            }
        }

        // Compute in-degrees: for each dirty key, count how many of its
        // dependencies are also dirty.
        for &key in &unique_keys {
            for dep in graph.dependencies(key, channel) {
                if in_degree.contains_key(&dep) {
                    *in_degree.get_mut(&key).expect("key is in in_degree") += 1;
                }
            }
        }

        // Initialize queue with keys that have no dirty dependencies
        let mut queue = VecDeque::with_capacity(in_degree.len());
        queue.extend(
            unique_keys
                .into_iter()
                .filter(|&k| in_degree.get(&k).is_some_and(|&deg| deg == 0)),
        );

        Self {
            graph,
            channel,
            queue,
            in_degree,
            stalled: false,
        }
    }

    /// Returns `true` if there are no more keys to yield.
    ///
    /// Note: if this returns `true` while [`remaining`](Self::remaining) is
    /// non-zero, the drain has stalled due to a cycle; see
    /// [`is_stalled`](Self::is_stalled).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns an upper bound on the remaining keys.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.in_degree.len()
    }

    /// Returns `true` if the drain has stalled due to a cycle.
    ///
    /// This becomes `true` once `next()` has returned `None` while there were
    /// still keys remaining.
    ///
    /// This is only meaningful after the iterator has been exhausted.
    #[must_use]
    pub fn is_stalled(&self) -> bool {
        self.stalled
    }

    /// Returns whether the drain completed or stalled due to a cycle.
    ///
    /// If the drain stalled, this returns how many keys could not be yielded.
    ///
    /// This is only meaningful after the iterator has been exhausted.
    /// Prefer [`collect_with_completion`](Self::collect_with_completion) to
    /// avoid accidental early checks.
    #[must_use]
    pub fn completion(&self) -> DrainCompletion {
        if self.stalled {
            DrainCompletion::Stalled {
                remaining: self.remaining(),
            }
        } else {
            DrainCompletion::Complete
        }
    }

    /// Collects all yielded keys and returns completion status.
    #[must_use]
    pub fn collect_with_completion(mut self) -> (Vec<K>, DrainCompletion) {
        let mut out = Vec::with_capacity(self.in_degree.len());
        out.extend(&mut self);
        let completion = self.completion();
        (out, completion)
    }
}

impl<'a, K> DrainSortedDeterministic<'a, K>
where
    K: Copy + Eq + Hash + Ord,
{
    pub(crate) fn from_iter_with_capacity<I>(
        dirty_keys: I,
        cap: usize,
        graph: &'a DirtyGraph<K>,
        channel: Channel,
    ) -> Self
    where
        I: Iterator<Item = K>,
    {
        // Deduplicate input keys via the in-degree map keys.
        let mut in_degree: HashMap<K, usize> = HashMap::with_capacity(cap);
        let mut unique_keys = Vec::with_capacity(cap);
        for key in dirty_keys {
            if let Entry::Vacant(e) = in_degree.entry(key) {
                e.insert(0);
                unique_keys.push(key);
            }
        }

        // Compute in-degrees within the dirty subset.
        for &key in &unique_keys {
            for dep in graph.dependencies(key, channel) {
                if in_degree.contains_key(&dep) {
                    *in_degree.get_mut(&key).expect("key is in in_degree") += 1;
                }
            }
        }

        // Initialize ready set with zero in-degree keys.
        let mut ready = BinaryHeap::with_capacity(in_degree.len());
        for key in unique_keys {
            if in_degree.get(&key).is_some_and(|&deg| deg == 0) {
                ready.push(Reverse(key));
            }
        }

        Self {
            graph,
            channel,
            ready,
            in_degree,
            stalled: false,
        }
    }

    /// Returns `true` if there are no more keys to yield.
    ///
    /// Note: if this returns `true` while [`remaining`](Self::remaining) is
    /// non-zero, the drain has stalled due to a cycle.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ready.is_empty()
    }

    /// Returns an upper bound on the remaining keys.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.in_degree.len()
    }

    /// Returns `true` if the drain has stalled due to a cycle.
    ///
    /// This becomes `true` once `next()` has returned `None` while there were
    /// still keys remaining.
    ///
    /// This is only meaningful after the iterator has been exhausted.
    #[must_use]
    pub fn is_stalled(&self) -> bool {
        self.stalled
    }

    /// Returns whether the drain completed or stalled due to a cycle.
    #[must_use]
    pub fn completion(&self) -> DrainCompletion {
        if self.stalled {
            DrainCompletion::Stalled {
                remaining: self.remaining(),
            }
        } else {
            DrainCompletion::Complete
        }
    }

    /// Collects all yielded keys and returns completion status.
    #[must_use]
    pub fn collect_with_completion(mut self) -> (Vec<K>, DrainCompletion) {
        let mut out = Vec::with_capacity(self.in_degree.len());
        out.extend(&mut self);
        let completion = self.completion();
        (out, completion)
    }
}

impl<K> Iterator for DrainSorted<'_, K>
where
    K: Copy + Eq + Hash,
{
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(key) = self.queue.pop_front() else {
            if !self.in_degree.is_empty() {
                self.stalled = true;
            }
            return None;
        };

        // Remove from in_degree to mark as processed
        self.in_degree.remove(&key);

        // Decrement in-degree of dirty dependents
        for dependent in self.graph.dependents(key, self.channel) {
            if let Some(deg) = self.in_degree.get_mut(&dependent) {
                *deg -= 1;
                if *deg == 0 {
                    self.queue.push_back(dependent);
                }
            }
        }

        Some(key)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.in_degree.len();
        (remaining, Some(remaining))
    }
}

impl<K> Iterator for DrainSortedDeterministic<'_, K>
where
    K: Copy + Eq + Hash + Ord,
{
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(Reverse(key)) = self.ready.pop() else {
            if !self.in_degree.is_empty() {
                self.stalled = true;
            }
            return None;
        };

        self.in_degree.remove(&key);

        for dependent in self.graph.dependents(key, self.channel) {
            if let Some(deg) = self.in_degree.get_mut(&dependent) {
                *deg -= 1;
                if *deg == 0 {
                    self.ready.push(Reverse(dependent));
                }
            }
        }

        Some(key)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.in_degree.len();
        (remaining, Some(remaining))
    }
}

/// Creates a topologically sorted drain from a dirty set.
///
/// This is a convenience function that collects the dirty keys from a
/// [`DirtySet`](crate::DirtySet), clears the channel, and returns a
/// [`DrainSorted`] iterator.
///
/// See [`DrainSorted`] for notes on cycles and nondeterministic ordering.
///
/// # Example
///
/// ```
/// use understory_dirty::{
///     Channel, CycleHandling, DirtyGraph, DirtySet, drain_sorted,
/// };
///
/// const LAYOUT: Channel = Channel::new(0);
///
/// let mut graph = DirtyGraph::<u32>::new();
/// graph.add_dependency(2, 1, LAYOUT, CycleHandling::Error).unwrap();
///
/// let mut dirty = DirtySet::new();
/// dirty.mark(1, LAYOUT);
/// dirty.mark(2, LAYOUT);
///
/// // Drain in topological order
/// let sorted: Vec<_> = drain_sorted(&mut dirty, &graph, LAYOUT).collect();
/// assert_eq!(sorted, vec![1, 2]);
///
/// // Channel is now empty
/// assert!(!dirty.has_dirty(LAYOUT));
/// ```
pub fn drain_sorted<'a, K>(
    dirty: &mut crate::DirtySet<K>,
    graph: &'a DirtyGraph<K>,
    channel: Channel,
) -> DrainSorted<'a, K>
where
    K: Copy + Eq + Hash,
{
    // Buffer the drained keys into a `Vec` first. This gives us stable, exact
    // sizing information and avoids interleaving reads from the source hash
    // table with writes to the destination maps/queues (which can regress due
    // to cache locality).
    let cap = dirty.len(channel);
    let mut dirty_keys = Vec::with_capacity(cap);
    dirty_keys.extend(dirty.drain(channel));

    DrainSorted::from_iter_with_capacity(dirty_keys.into_iter(), cap, graph, channel)
}

/// Creates a deterministic, topologically sorted drain from a dirty set.
///
/// This is equivalent to [`drain_sorted`], but when multiple keys are ready
/// simultaneously it yields them in ascending key order (`Ord`).
pub fn drain_sorted_deterministic<'a, K>(
    dirty: &mut crate::DirtySet<K>,
    graph: &'a DirtyGraph<K>,
    channel: Channel,
) -> DrainSortedDeterministic<'a, K>
where
    K: Copy + Eq + Hash + Ord,
{
    let cap = dirty.len(channel);
    let mut dirty_keys = Vec::with_capacity(cap);
    dirty_keys.extend(dirty.drain(channel));

    DrainSortedDeterministic::from_iter_with_capacity(dirty_keys.into_iter(), cap, graph, channel)
}

/// Creates a topologically sorted drain that includes all affected keys.
///
/// Unlike [`drain_sorted`], this function expands the dirty set to include
/// all transitive dependents of the marked keys before sorting. This is the
/// correct drain function to use with [`LazyPolicy`](crate::LazyPolicy),
/// which only marks roots at mark-time.
///
/// # Algorithm
///
/// 1. Collect all keys currently marked dirty in the channel.
/// 2. For each dirty key, compute all transitive dependents.
/// 3. Build the affected set: dirty keys ∪ all transitive dependents.
/// 4. Return a topologically sorted drain over the affected set.
///
/// See [`DrainSorted`] for notes on cycles and nondeterministic ordering.
///
/// # Example
///
/// ```
/// use understory_dirty::{
///     Channel, CycleHandling, DirtyGraph, DirtySet, drain_affected_sorted,
/// };
///
/// const LAYOUT: Channel = Channel::new(0);
///
/// let mut graph = DirtyGraph::<u32>::new();
/// // 1 <- 2 <- 3
/// graph.add_dependency(2, 1, LAYOUT, CycleHandling::Error).unwrap();
/// graph.add_dependency(3, 2, LAYOUT, CycleHandling::Error).unwrap();
///
/// let mut dirty = DirtySet::new();
/// // Only mark the root
/// dirty.mark(1, LAYOUT);
///
/// // drain_affected_sorted expands to include all dependents
/// let sorted: Vec<_> = drain_affected_sorted(&mut dirty, &graph, LAYOUT).collect();
/// assert_eq!(sorted, vec![1, 2, 3]);
///
/// // Channel is now empty
/// assert!(!dirty.has_dirty(LAYOUT));
/// ```
pub fn drain_affected_sorted<'a, K>(
    dirty: &mut crate::DirtySet<K>,
    graph: &'a DirtyGraph<K>,
    channel: Channel,
) -> DrainSorted<'a, K>
where
    K: Copy + Eq + Hash,
{
    // Collect dirty keys (roots) and clear the channel.
    //
    // Keep a stable view of the roots (the prefix of the vec) while we append
    // dependents onto the end.
    let mut affected_keys: Vec<K> = dirty.drain(channel).collect();
    let roots_len = affected_keys.len();

    // Expand to include all transitive dependents
    for i in 0..roots_len {
        let root = affected_keys[i];
        for dependent in graph.transitive_dependents(root, channel) {
            affected_keys.push(dependent);
        }
    }

    let cap = affected_keys.len();
    DrainSorted::from_iter_with_capacity(affected_keys.into_iter(), cap, graph, channel)
}

/// Creates a deterministic, topologically sorted drain that includes all
/// affected keys.
///
/// This is equivalent to [`drain_affected_sorted`], but when multiple keys are
/// ready simultaneously it yields them in ascending key order (`Ord`).
pub fn drain_affected_sorted_deterministic<'a, K>(
    dirty: &mut crate::DirtySet<K>,
    graph: &'a DirtyGraph<K>,
    channel: Channel,
) -> DrainSortedDeterministic<'a, K>
where
    K: Copy + Eq + Hash + Ord,
{
    let mut affected_keys: Vec<K> = dirty.drain(channel).collect();
    let roots_len = affected_keys.len();

    for i in 0..roots_len {
        let root = affected_keys[i];
        for dependent in graph.transitive_dependents(root, channel) {
            affected_keys.push(dependent);
        }
    }

    let cap = affected_keys.len();
    DrainSortedDeterministic::from_iter_with_capacity(
        affected_keys.into_iter(),
        cap,
        graph,
        channel,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    use crate::graph::CycleHandling;
    use crate::set::DirtySet;

    const LAYOUT: Channel = Channel::new(0);

    #[test]
    fn topological_order_chain() {
        let mut graph = DirtyGraph::new();
        // 1 <- 2 <- 3 <- 4
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(4, 3, LAYOUT, CycleHandling::Error)
            .unwrap();

        let dirty_keys = vec![4, 2, 1, 3]; // Out of order
        let cap = dirty_keys.len();
        let sorted: Vec<_> =
            DrainSorted::from_iter_with_capacity(dirty_keys.into_iter(), cap, &graph, LAYOUT)
                .collect();

        // Must be in topological order
        assert_eq!(sorted, vec![1, 2, 3, 4]);
    }

    #[test]
    fn topological_order_diamond() {
        let mut graph = DirtyGraph::new();
        // 1 <- 2, 1 <- 3, 2 <- 4, 3 <- 4
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(4, 2, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(4, 3, LAYOUT, CycleHandling::Error)
            .unwrap();

        let dirty_keys = vec![4, 3, 2, 1];
        let cap = dirty_keys.len();
        let sorted: Vec<_> =
            DrainSorted::from_iter_with_capacity(dirty_keys.into_iter(), cap, &graph, LAYOUT)
                .collect();

        // 1 must come first, 4 must come last, 2 and 3 can be in either order
        assert_eq!(sorted[0], 1);
        assert_eq!(sorted[3], 4);
        assert!(sorted[1] == 2 || sorted[1] == 3);
        assert!(sorted[2] == 2 || sorted[2] == 3);
    }

    #[test]
    fn partial_dirty_set() {
        let mut graph = DirtyGraph::new();
        // 1 <- 2 <- 3
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();

        // Only 2 and 3 are dirty (not 1)
        let dirty_keys = vec![3, 2];
        let cap = dirty_keys.len();
        let sorted: Vec<_> =
            DrainSorted::from_iter_with_capacity(dirty_keys.into_iter(), cap, &graph, LAYOUT)
                .collect();

        // 2 should come before 3, but 1 is not in the dirty set
        assert_eq!(sorted, vec![2, 3]);
    }

    #[test]
    fn no_dependencies() {
        let graph = DirtyGraph::<u32>::new();
        let dirty_keys = vec![3, 1, 2];
        let cap = dirty_keys.len();
        let sorted: Vec<_> =
            DrainSorted::from_iter_with_capacity(dirty_keys.into_iter(), cap, &graph, LAYOUT)
                .collect();

        // All have in-degree 0, so order is based on iteration order (set-dependent)
        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn drain_sorted_function() {
        let mut graph = DirtyGraph::new();
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();

        let mut dirty = DirtySet::new();
        dirty.mark(1, LAYOUT);
        dirty.mark(2, LAYOUT);

        let sorted: Vec<_> = drain_sorted(&mut dirty, &graph, LAYOUT).collect();
        assert_eq!(sorted, vec![1, 2]);

        // Channel should be empty
        assert!(!dirty.has_dirty(LAYOUT));
    }

    #[test]
    fn empty_dirty_set() {
        let graph = DirtyGraph::<u32>::new();
        let dirty_keys: Vec<u32> = vec![];
        let cap = dirty_keys.len();
        let sorted: Vec<_> =
            DrainSorted::from_iter_with_capacity(dirty_keys.into_iter(), cap, &graph, LAYOUT)
                .collect();
        assert!(sorted.is_empty());
    }

    #[test]
    fn size_hint_accurate() {
        let mut graph = DirtyGraph::new();
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();

        let dirty_keys = vec![1, 2];
        let cap = dirty_keys.len();
        let mut drain =
            DrainSorted::from_iter_with_capacity(dirty_keys.into_iter(), cap, &graph, LAYOUT);

        assert_eq!(drain.size_hint(), (2, Some(2)));
        assert_eq!(drain.remaining(), 2);

        let _ = drain.next();
        assert_eq!(drain.size_hint(), (1, Some(1)));

        let _ = drain.next();
        assert_eq!(drain.size_hint(), (0, Some(0)));
        assert!(drain.is_empty());
    }

    #[test]
    fn duplicate_keys_deduplicated() {
        let mut graph = DirtyGraph::new();
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();

        // Duplicates in input
        let dirty_keys = vec![1, 2, 1, 2, 1];
        let cap = dirty_keys.len();
        let sorted: Vec<_> =
            DrainSorted::from_iter_with_capacity(dirty_keys.into_iter(), cap, &graph, LAYOUT)
                .collect();

        // Should deduplicate to just [1, 2]
        assert_eq!(sorted.len(), 2);
        assert_eq!(sorted, vec![1, 2]);
    }

    #[test]
    fn cycles_stall_drain() {
        let mut graph = DirtyGraph::new();
        // Create a cycle: 1 <- 2 <- 3 <- 1 (with Allow)
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Allow)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Allow)
            .unwrap();
        graph
            .add_dependency(1, 3, LAYOUT, CycleHandling::Allow)
            .unwrap();

        // All three are dirty
        let dirty_keys = vec![1, 2, 3];
        let cap = dirty_keys.len();
        let mut drain =
            DrainSorted::from_iter_with_capacity(dirty_keys.into_iter(), cap, &graph, LAYOUT);
        let sorted: Vec<_> = drain.by_ref().collect();

        // All keys are in a cycle, so no key has in-degree 0, nothing is yielded
        assert!(
            sorted.is_empty(),
            "cycle should prevent any keys from being yielded"
        );
        assert!(drain.is_stalled());
        assert_eq!(
            drain.completion(),
            DrainCompletion::Stalled { remaining: 3 }
        );
    }

    #[test]
    fn cycles_stall_drain_collect_with_completion() {
        let mut graph = DirtyGraph::new();
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Allow)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Allow)
            .unwrap();
        graph
            .add_dependency(1, 3, LAYOUT, CycleHandling::Allow)
            .unwrap();

        let mut dirty = DirtySet::new();
        dirty.mark(1, LAYOUT);
        dirty.mark(2, LAYOUT);
        dirty.mark(3, LAYOUT);

        let (sorted, completion) =
            drain_sorted(&mut dirty, &graph, LAYOUT).collect_with_completion();
        assert!(sorted.is_empty());
        assert_eq!(completion, DrainCompletion::Stalled { remaining: 3 });
    }

    #[test]
    fn drain_affected_sorted_expands_dependents() {
        let mut graph = DirtyGraph::new();
        // 1 <- 2 <- 3 <- 4
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(4, 3, LAYOUT, CycleHandling::Error)
            .unwrap();

        let mut dirty = DirtySet::new();
        // Only mark the root
        dirty.mark(1, LAYOUT);

        // drain_affected_sorted should expand to include all dependents
        let sorted: Vec<_> = drain_affected_sorted(&mut dirty, &graph, LAYOUT).collect();
        assert_eq!(sorted, vec![1, 2, 3, 4]);

        // Channel should be empty
        assert!(!dirty.has_dirty(LAYOUT));
    }

    #[test]
    fn drain_affected_sorted_multiple_roots() {
        let mut graph = DirtyGraph::new();
        // Two chains: 1 <- 2, 3 <- 4
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(4, 3, LAYOUT, CycleHandling::Error)
            .unwrap();

        let mut dirty = DirtySet::new();
        dirty.mark(1, LAYOUT);
        dirty.mark(3, LAYOUT);

        let sorted: Vec<_> = drain_affected_sorted(&mut dirty, &graph, LAYOUT).collect();
        // Should include all 4 keys (order between chains is nondeterministic)
        assert_eq!(sorted.len(), 4);
    }

    #[test]
    fn deterministic_topological_order_diamond_is_total() {
        let mut graph = DirtyGraph::new();
        // 1 <- 2, 1 <- 3, 2 <- 4, 3 <- 4
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(4, 2, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(4, 3, LAYOUT, CycleHandling::Error)
            .unwrap();

        let dirty_keys = vec![4, 3, 2, 1];
        let cap = dirty_keys.len();
        let sorted: Vec<_> = DrainSortedDeterministic::from_iter_with_capacity(
            dirty_keys.into_iter(),
            cap,
            &graph,
            LAYOUT,
        )
        .collect();

        // Deterministic tie-breaker yields 2 before 3.
        assert_eq!(sorted, vec![1, 2, 3, 4]);
    }
}
