// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Combined dirty tracker: graph + set convenience type.

use core::hash::Hash;

use crate::channel::Channel;
use crate::drain::DrainSorted;
use crate::graph::{CycleError, CycleHandling, DirtyGraph};
use crate::policy::PropagationPolicy;
use crate::set::DirtySet;

/// Combined dirty tracker with dependency graph and dirty set.
///
/// `DirtyTracker` is a convenience type that bundles a [`DirtyGraph`] and
/// [`DirtySet`] together, providing a unified API for common dirty-tracking
/// operations.
///
/// # Type Parameters
///
/// - `K`: The key type, typically a node identifier. Must be `Copy + Eq + Hash`.
///
/// # Example
///
/// ```
/// use understory_dirty::{Channel, CycleHandling, DirtyTracker, EagerPolicy};
///
/// const LAYOUT: Channel = Channel::new(0);
/// const PAINT: Channel = Channel::new(1);
///
/// let mut tracker = DirtyTracker::<u32>::new();
///
/// // Build dependency graph: 3 depends on 2, 2 depends on 1
/// tracker.add_dependency(2, 1, LAYOUT).unwrap();
/// tracker.add_dependency(3, 2, LAYOUT).unwrap();
///
/// // Mark with eager propagation (marks 1, 2, 3)
/// tracker.mark_with(1, LAYOUT, &EagerPolicy);
///
/// // Or mark manually without propagation
/// tracker.mark(1, PAINT);
///
/// // Drain in topological order: 1, 2, 3
/// let order: Vec<_> = tracker.drain_sorted(LAYOUT).collect();
/// assert_eq!(order, vec![1, 2, 3]);
/// ```
///
/// # See Also
///
/// - [`DirtyGraph`] and [`DirtySet`]: The underlying components.
/// - [`EagerPolicy`](crate::EagerPolicy) and [`LazyPolicy`](crate::LazyPolicy): Built-in propagation strategies.
/// - [`drain_sorted`](crate::drain_sorted) and [`drain_affected_sorted`](crate::drain_affected_sorted): Free-function drain helpers.
#[derive(Debug, Clone)]
pub struct DirtyTracker<K>
where
    K: Copy + Eq + Hash,
{
    /// The dependency graph.
    graph: DirtyGraph<K>,
    /// The dirty set.
    dirty: DirtySet<K>,
    /// How to handle cycles when adding dependencies.
    cycle_handling: CycleHandling,
}

impl<K> Default for DirtyTracker<K>
where
    K: Copy + Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K> DirtyTracker<K>
where
    K: Copy + Eq + Hash,
{
    /// Creates a new empty dirty tracker with default cycle handling.
    #[must_use]
    pub fn new() -> Self {
        Self::with_cycle_handling(CycleHandling::default())
    }

    /// Creates a new empty dirty tracker with the specified cycle handling.
    #[must_use]
    pub fn with_cycle_handling(cycle_handling: CycleHandling) -> Self {
        Self {
            graph: DirtyGraph::new(),
            dirty: DirtySet::new(),
            cycle_handling,
        }
    }

    /// Returns a reference to the underlying dependency graph.
    #[must_use]
    pub fn graph(&self) -> &DirtyGraph<K> {
        &self.graph
    }

    /// Returns a mutable reference to the underlying dependency graph.
    #[must_use]
    pub fn graph_mut(&mut self) -> &mut DirtyGraph<K> {
        &mut self.graph
    }

    /// Returns a reference to the underlying dirty set.
    #[must_use]
    pub fn dirty(&self) -> &DirtySet<K> {
        &self.dirty
    }

    /// Returns a mutable reference to the underlying dirty set.
    #[must_use]
    pub fn dirty_mut(&mut self) -> &mut DirtySet<K> {
        &mut self.dirty
    }

    /// Returns the current generation of the dirty set.
    ///
    /// See [`DirtySet::generation`] for details.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.dirty.generation()
    }

    /// Returns the current cycle handling mode.
    #[must_use]
    pub fn cycle_handling(&self) -> CycleHandling {
        self.cycle_handling
    }

    /// Sets the cycle handling mode for future operations.
    pub fn set_cycle_handling(&mut self, handling: CycleHandling) {
        self.cycle_handling = handling;
    }

    // -------------------------------------------------------------------------
    // Graph operations
    // -------------------------------------------------------------------------

    /// Adds a dependency: `from` depends on `to` in the given channel.
    ///
    /// Uses the tracker's configured cycle handling mode.
    ///
    /// See [`DirtyGraph::add_dependency`] for details.
    pub fn add_dependency(
        &mut self,
        from: K,
        to: K,
        channel: Channel,
    ) -> Result<bool, CycleError<K>> {
        self.graph
            .add_dependency(from, to, channel, self.cycle_handling)
    }

    /// Adds a dependency with explicit cycle handling.
    ///
    /// See [`DirtyGraph::add_dependency`] for details.
    pub fn add_dependency_with(
        &mut self,
        from: K,
        to: K,
        channel: Channel,
        handling: CycleHandling,
    ) -> Result<bool, CycleError<K>> {
        self.graph.add_dependency(from, to, channel, handling)
    }

    /// Removes a dependency: `from` no longer depends on `to`.
    ///
    /// See [`DirtyGraph::remove_dependency`] for details.
    pub fn remove_dependency(&mut self, from: K, to: K, channel: Channel) -> bool {
        self.graph.remove_dependency(from, to, channel)
    }

    /// Removes a key from both the graph and the dirty set.
    ///
    /// This is useful when a node is removed from the tree entirely.
    pub fn remove_key(&mut self, key: K) {
        self.graph.remove_key(key);
        self.dirty.remove_key(key);
    }

    // -------------------------------------------------------------------------
    // Dirty marking
    // -------------------------------------------------------------------------

    /// Marks a key as dirty without propagation.
    ///
    /// Returns `true` if the key was newly marked dirty.
    pub fn mark(&mut self, key: K, channel: Channel) -> bool {
        self.dirty.mark(key, channel)
    }

    /// Marks a key as dirty using the given propagation policy.
    ///
    /// The policy determines how dirty marks spread through the dependency
    /// graph. See [`PropagationPolicy`] for details.
    pub fn mark_with<P>(&mut self, key: K, channel: Channel, policy: &P)
    where
        P: PropagationPolicy<K>,
    {
        policy.propagate(key, channel, &self.graph, &mut self.dirty);
    }

    /// Returns `true` if the key is dirty in the given channel.
    #[must_use]
    pub fn is_dirty(&self, key: K, channel: Channel) -> bool {
        self.dirty.is_dirty(key, channel)
    }

    /// Returns `true` if there are any dirty keys in the given channel.
    #[must_use]
    pub fn has_dirty(&self, channel: Channel) -> bool {
        self.dirty.has_dirty(channel)
    }

    /// Returns `true` if there are no dirty keys in any channel.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.dirty.is_empty()
    }

    // -------------------------------------------------------------------------
    // Draining
    // -------------------------------------------------------------------------

    /// Drains dirty keys in topological order using Kahn's algorithm.
    ///
    /// Keys are yielded in dependency order: a key is only yielded after
    /// all of its dirty dependencies have been yielded. This ensures that
    /// when processing the dirty set, a node is only processed after all
    /// of its dependencies have been processed.
    ///
    /// The channel is cleared eagerly when this iterator is created.
    ///
    /// Note: if the dependency subgraph induced by the dirty keys contains a
    /// cycle, the drain will stall and some keys will not be yielded. You can
    /// detect this by exhausting the iterator and checking
    /// [`DrainSorted::completion`], or by using
    /// [`DrainSorted::collect_with_completion`].
    ///
    /// # Example
    ///
    /// ```
    /// use understory_dirty::{Channel, DirtyTracker, EagerPolicy};
    ///
    /// const LAYOUT: Channel = Channel::new(0);
    ///
    /// let mut tracker = DirtyTracker::<u32>::new();
    /// tracker.add_dependency(2, 1, LAYOUT).unwrap();
    /// tracker.add_dependency(3, 2, LAYOUT).unwrap();
    ///
    /// tracker.mark_with(1, LAYOUT, &EagerPolicy);
    ///
    /// // Process in order: 1, 2, 3
    /// for key in tracker.drain_sorted(LAYOUT) {
    ///     // recompute_layout(key);
    /// }
    /// ```
    pub fn drain_sorted(&mut self, channel: Channel) -> DrainSorted<'_, K> {
        crate::drain::drain_sorted(&mut self.dirty, &self.graph, channel)
    }

    /// Drains all affected keys in topological order.
    ///
    /// Unlike [`drain_sorted`](Self::drain_sorted), this method first expands
    /// the dirty set to include all transitive dependents of the marked keys.
    /// This is the correct drain method to use with [`LazyPolicy`](crate::LazyPolicy).
    ///
    /// Note: the yielded order is only deterministic up to dependency ordering.
    /// When multiple keys are simultaneously ready, the relative order among
    /// them is not specified and may vary across runs or platforms.
    ///
    /// # Algorithm
    ///
    /// 1. Collect all keys currently marked dirty (the "roots").
    /// 2. Compute all transitive dependents of each root.
    /// 3. Return a topologically sorted drain over: roots ∪ dependents.
    ///
    /// # Example
    ///
    /// ```
    /// use understory_dirty::{Channel, DirtyTracker, LazyPolicy};
    ///
    /// const LAYOUT: Channel = Channel::new(0);
    ///
    /// let mut tracker = DirtyTracker::<u32>::new();
    /// tracker.add_dependency(2, 1, LAYOUT).unwrap();
    /// tracker.add_dependency(3, 2, LAYOUT).unwrap();
    ///
    /// // Mark only the root with lazy policy
    /// tracker.mark_with(1, LAYOUT, &LazyPolicy);
    ///
    /// // drain_affected_sorted expands to all affected keys: 1, 2, 3
    /// let order: Vec<_> = tracker.drain_affected_sorted(LAYOUT).collect();
    /// assert_eq!(order, vec![1, 2, 3]);
    /// ```
    pub fn drain_affected_sorted(&mut self, channel: Channel) -> DrainSorted<'_, K> {
        crate::drain::drain_affected_sorted(&mut self.dirty, &self.graph, channel)
    }

    /// Collects dirty keys and returns a [`DrainSorted`] iterator.
    ///
    /// Unlike [`drain_sorted`](Self::drain_sorted), this method does not
    /// clear the dirty set. It's useful when you need to iterate multiple
    /// times or want to keep the dirty state.
    #[must_use]
    pub fn peek_sorted(&self, channel: Channel) -> DrainSorted<'_, K> {
        let cap = self.dirty.len(channel);
        DrainSorted::from_iter_with_capacity(self.dirty.iter(channel), cap, &self.graph, channel)
    }

    /// Clears all dirty keys in the given channel.
    pub fn clear(&mut self, channel: Channel) {
        self.dirty.clear(channel);
    }

    /// Clears all dirty keys in all channels.
    pub fn clear_all(&mut self) {
        self.dirty.clear_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    use crate::policy::{EagerPolicy, LazyPolicy};

    const LAYOUT: Channel = Channel::new(0);
    const PAINT: Channel = Channel::new(1);

    #[test]
    fn basic_workflow() {
        let mut tracker = DirtyTracker::<u32>::new();

        // Build graph
        tracker.add_dependency(2, 1, LAYOUT).unwrap();
        tracker.add_dependency(3, 2, LAYOUT).unwrap();

        // Mark with eager policy
        tracker.mark_with(1, LAYOUT, &EagerPolicy);

        assert!(tracker.is_dirty(1, LAYOUT));
        assert!(tracker.is_dirty(2, LAYOUT));
        assert!(tracker.is_dirty(3, LAYOUT));

        // Drain in topological order
        let order: Vec<_> = tracker.drain_sorted(LAYOUT).collect();
        assert_eq!(order, vec![1, 2, 3]);

        // Channel is now clean
        assert!(!tracker.has_dirty(LAYOUT));
    }

    #[test]
    fn manual_mark_no_propagation() {
        let mut tracker = DirtyTracker::<u32>::new();

        tracker.add_dependency(2, 1, LAYOUT).unwrap();

        // Manual mark - no propagation
        tracker.mark(1, LAYOUT);

        assert!(tracker.is_dirty(1, LAYOUT));
        assert!(!tracker.is_dirty(2, LAYOUT));
    }

    #[test]
    fn lazy_policy() {
        let mut tracker = DirtyTracker::<u32>::new();

        tracker.add_dependency(2, 1, LAYOUT).unwrap();
        tracker.add_dependency(3, 2, LAYOUT).unwrap();

        // Lazy mark - only marks the key itself
        tracker.mark_with(1, LAYOUT, &LazyPolicy);

        assert!(tracker.is_dirty(1, LAYOUT));
        assert!(!tracker.is_dirty(2, LAYOUT));
        assert!(!tracker.is_dirty(3, LAYOUT));
    }

    #[test]
    fn remove_key() {
        let mut tracker = DirtyTracker::<u32>::new();

        tracker.add_dependency(2, 1, LAYOUT).unwrap();
        tracker.mark(1, LAYOUT);
        tracker.mark(2, LAYOUT);

        tracker.remove_key(2);

        // Node 2 is gone from both graph and dirty set
        assert!(!tracker.graph().dependents(1, LAYOUT).any(|_| true));
        assert!(!tracker.is_dirty(2, LAYOUT));
        assert!(tracker.is_dirty(1, LAYOUT));
    }

    #[test]
    fn peek_sorted_preserves_state() {
        let mut tracker = DirtyTracker::<u32>::new();

        tracker.add_dependency(2, 1, LAYOUT).unwrap();
        tracker.mark(1, LAYOUT);
        tracker.mark(2, LAYOUT);

        // Peek does not clear
        let order: Vec<_> = tracker.peek_sorted(LAYOUT).collect();
        assert_eq!(order, vec![1, 2]);

        // Still dirty
        assert!(tracker.is_dirty(1, LAYOUT));
        assert!(tracker.is_dirty(2, LAYOUT));
    }

    #[test]
    fn generation_tracking() {
        let mut tracker = DirtyTracker::<u32>::new();
        let initial = tracker.generation();

        tracker.mark(1, LAYOUT);
        assert_eq!(tracker.generation(), initial + 1);

        tracker.mark(2, LAYOUT);
        assert_eq!(tracker.generation(), initial + 2);
    }

    #[test]
    fn cycle_handling_modes() {
        let mut tracker = DirtyTracker::with_cycle_handling(CycleHandling::Error);

        tracker.add_dependency(2, 1, LAYOUT).unwrap();

        // Self-cycle should error
        let result = tracker.add_dependency(1, 1, LAYOUT);
        assert!(result.is_err());

        // Change to ignore mode
        tracker.set_cycle_handling(CycleHandling::Ignore);
        let result = tracker.add_dependency(1, 1, LAYOUT);
        assert!(result.is_ok());
    }

    #[test]
    fn multiple_channels() {
        let mut tracker = DirtyTracker::<u32>::new();

        tracker.add_dependency(2, 1, LAYOUT).unwrap();
        tracker.add_dependency(2, 1, PAINT).unwrap();

        tracker.mark_with(1, LAYOUT, &EagerPolicy);

        // Only LAYOUT is dirty
        assert!(tracker.is_dirty(1, LAYOUT));
        assert!(tracker.is_dirty(2, LAYOUT));
        assert!(!tracker.is_dirty(1, PAINT));
        assert!(!tracker.is_dirty(2, PAINT));

        tracker.mark_with(1, PAINT, &EagerPolicy);

        // Now both are dirty
        assert!(tracker.is_dirty(1, PAINT));
        assert!(tracker.is_dirty(2, PAINT));
    }

    #[test]
    fn clear_specific_channel() {
        let mut tracker = DirtyTracker::<u32>::new();

        tracker.mark(1, LAYOUT);
        tracker.mark(1, PAINT);

        tracker.clear(LAYOUT);

        assert!(!tracker.has_dirty(LAYOUT));
        assert!(tracker.has_dirty(PAINT));
    }

    #[test]
    fn clear_all() {
        let mut tracker = DirtyTracker::<u32>::new();

        tracker.mark(1, LAYOUT);
        tracker.mark(1, PAINT);

        tracker.clear_all();

        assert!(tracker.is_clean());
    }
}
