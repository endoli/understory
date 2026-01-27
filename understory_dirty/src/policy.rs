// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Propagation policies for dirty marking.

use core::hash::Hash;

use crate::channel::Channel;
use crate::graph::DirtyGraph;
use crate::scratch::TraversalScratch;
use crate::set::DirtySet;

/// Trait for dirty propagation policies.
///
/// A propagation policy determines how dirty marks spread through the
/// dependency graph. When a key is marked dirty, the policy can choose
/// to immediately propagate to all dependents (eager), defer propagation
/// (lazy), or implement custom strategies.
///
/// # Example
///
/// ```
/// use understory_dirty::{
///     Channel, CycleHandling, DirtyGraph, DirtySet, EagerPolicy, PropagationPolicy,
/// };
///
/// const LAYOUT: Channel = Channel::new(0);
///
/// let mut graph = DirtyGraph::<u32>::new();
/// graph.add_dependency(2, 1, LAYOUT, CycleHandling::Error).unwrap();
/// graph.add_dependency(3, 2, LAYOUT, CycleHandling::Error).unwrap();
///
/// let mut dirty = DirtySet::new();
/// let eager = EagerPolicy;
///
/// // Mark node 1 dirty with eager propagation
/// eager.propagate(1, LAYOUT, &graph, &mut dirty);
///
/// // All transitive dependents are now dirty
/// assert!(dirty.is_dirty(1, LAYOUT));
/// assert!(dirty.is_dirty(2, LAYOUT));
/// assert!(dirty.is_dirty(3, LAYOUT));
/// ```
pub trait PropagationPolicy<K>
where
    K: Copy + Eq + Hash,
{
    /// Propagates dirty marks from `key` through the dependency graph.
    ///
    /// This method is called after `key` has been marked dirty. The policy
    /// should mark any additional keys that should become dirty as a result.
    ///
    /// # Parameters
    ///
    /// - `key`: The key that was just marked dirty.
    /// - `channel`: The channel in which the key is dirty.
    /// - `graph`: The dependency graph (read-only).
    /// - `dirty`: The dirty set to mark additional keys in.
    fn propagate(&self, key: K, channel: Channel, graph: &DirtyGraph<K>, dirty: &mut DirtySet<K>);
}

/// Eager propagation policy: immediately mark all transitive dependents.
///
/// When a key is marked dirty, `EagerPolicy` performs a DFS traversal of
/// the dependency graph and marks all transitive dependents as dirty.
///
/// This is useful when you want to know the full dirty set immediately
/// after marking, without waiting for drain time.
///
/// # Example
///
/// ```
/// use understory_dirty::{
///     Channel, CycleHandling, DirtyGraph, DirtySet, EagerPolicy, PropagationPolicy,
/// };
///
/// const LAYOUT: Channel = Channel::new(0);
///
/// let mut graph = DirtyGraph::<u32>::new();
/// // Chain: 1 <- 2 <- 3
/// graph.add_dependency(2, 1, LAYOUT, CycleHandling::Error).unwrap();
/// graph.add_dependency(3, 2, LAYOUT, CycleHandling::Error).unwrap();
///
/// let mut dirty = DirtySet::new();
/// let eager = EagerPolicy;
///
/// // Mark node 1, propagates to 2 and 3
/// eager.propagate(1, LAYOUT, &graph, &mut dirty);
///
/// assert!(dirty.is_dirty(1, LAYOUT));
/// assert!(dirty.is_dirty(2, LAYOUT));
/// assert!(dirty.is_dirty(3, LAYOUT));
/// ```
#[derive(Copy, Clone, Debug, Default)]
pub struct EagerPolicy;

impl<K> PropagationPolicy<K> for EagerPolicy
where
    K: Copy + Eq + Hash,
{
    fn propagate(&self, key: K, channel: Channel, graph: &DirtyGraph<K>, dirty: &mut DirtySet<K>) {
        // Mark the key itself
        dirty.mark(key, channel);

        // DFS to mark all transitive dependents
        for dependent in graph.transitive_dependents(key, channel) {
            dirty.mark(dependent, channel);
        }
    }
}

impl EagerPolicy {
    /// Propagates using reusable scratch buffers.
    ///
    /// This is equivalent to calling [`PropagationPolicy::propagate`] for
    /// [`EagerPolicy`], but avoids per-call allocations by reusing `scratch`.
    ///
    /// # See Also
    ///
    /// - [`TraversalScratch`]: Reusable traversal storage.
    /// - [`DirtyGraph::for_each_transitive_dependent`]: Scratch-powered traversal.
    pub fn propagate_with_scratch<K>(
        &self,
        key: K,
        channel: Channel,
        graph: &DirtyGraph<K>,
        dirty: &mut DirtySet<K>,
        scratch: &mut TraversalScratch<K>,
    ) where
        K: Copy + Eq + Hash,
    {
        dirty.mark(key, channel);
        graph.for_each_transitive_dependent(key, channel, scratch, |dependent| {
            dirty.mark(dependent, channel);
        });
    }
}

/// Lazy propagation policy: only marks the key itself, no propagation.
///
/// `LazyPolicy` does not propagate dirty marks at mark time. Only the
/// explicitly marked key is added to the dirty set. To process all affected
/// keys (marked roots + their transitive dependents), use
/// [`drain_affected_sorted`](crate::drain_affected_sorted) or
/// [`DirtyTracker::drain_affected_sorted`](crate::DirtyTracker::drain_affected_sorted)
/// at drain time.
///
/// This is useful when many marks happen in succession and you want to
/// avoid redundant traversals. The tradeoff is that [`DirtySet::is_dirty`]
/// will not reflect transitive dirty state; only the explicitly marked
/// roots are in the dirty set.
///
/// # Important
///
/// - Use [`drain_affected_sorted`](crate::drain_affected_sorted) (not `drain_sorted`)
///   to correctly process all affected keys when using `LazyPolicy`.
/// - Using `drain_sorted` with `LazyPolicy` will only process the marked roots,
///   not their dependents.
///
/// # Example
///
/// ```
/// use understory_dirty::{
///     Channel, CycleHandling, DirtyGraph, DirtySet, LazyPolicy, PropagationPolicy,
///     drain_affected_sorted,
/// };
///
/// const LAYOUT: Channel = Channel::new(0);
///
/// let mut graph = DirtyGraph::<u32>::new();
/// graph.add_dependency(2, 1, LAYOUT, CycleHandling::Error).unwrap();
/// graph.add_dependency(3, 2, LAYOUT, CycleHandling::Error).unwrap();
///
/// let mut dirty = DirtySet::new();
/// let lazy = LazyPolicy;
///
/// // Mark node 1 with lazy policy
/// lazy.propagate(1, LAYOUT, &graph, &mut dirty);
///
/// // Only node 1 is marked (dependents not marked yet)
/// assert!(dirty.is_dirty(1, LAYOUT));
/// assert!(!dirty.is_dirty(2, LAYOUT));
///
/// // Use drain_affected_sorted to expand and process all affected keys
/// let affected: Vec<_> = drain_affected_sorted(&mut dirty, &graph, LAYOUT).collect();
/// assert_eq!(affected, vec![1, 2, 3]); // All affected keys in topological order
/// ```
#[derive(Copy, Clone, Debug, Default)]
pub struct LazyPolicy;

impl<K> PropagationPolicy<K> for LazyPolicy
where
    K: Copy + Eq + Hash,
{
    fn propagate(&self, key: K, channel: Channel, _graph: &DirtyGraph<K>, dirty: &mut DirtySet<K>) {
        // Just mark the key, no propagation
        dirty.mark(key, channel);
    }
}

/// Blanket implementation for boxed policies.
impl<K, P> PropagationPolicy<K> for &P
where
    K: Copy + Eq + Hash,
    P: PropagationPolicy<K> + ?Sized,
{
    fn propagate(&self, key: K, channel: Channel, graph: &DirtyGraph<K>, dirty: &mut DirtySet<K>) {
        (*self).propagate(key, channel, graph, dirty);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    use crate::graph::CycleHandling;

    const LAYOUT: Channel = Channel::new(0);

    fn setup_chain_graph() -> DirtyGraph<u32> {
        let mut graph = DirtyGraph::new();
        // Chain: 1 <- 2 <- 3 <- 4
        graph
            .add_dependency(2, 1, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(3, 2, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
            .add_dependency(4, 3, LAYOUT, CycleHandling::Error)
            .unwrap();
        graph
    }

    #[test]
    fn eager_policy_marks_all_dependents() {
        let graph = setup_chain_graph();
        let mut dirty = DirtySet::new();
        let eager = EagerPolicy;

        eager.propagate(1, LAYOUT, &graph, &mut dirty);

        assert!(dirty.is_dirty(1, LAYOUT));
        assert!(dirty.is_dirty(2, LAYOUT));
        assert!(dirty.is_dirty(3, LAYOUT));
        assert!(dirty.is_dirty(4, LAYOUT));
    }

    #[test]
    fn eager_policy_from_middle() {
        let graph = setup_chain_graph();
        let mut dirty = DirtySet::new();
        let eager = EagerPolicy;

        eager.propagate(2, LAYOUT, &graph, &mut dirty);

        // Node 1 is NOT dirty (not a dependent)
        assert!(!dirty.is_dirty(1, LAYOUT));
        // Nodes 2, 3, 4 are dirty
        assert!(dirty.is_dirty(2, LAYOUT));
        assert!(dirty.is_dirty(3, LAYOUT));
        assert!(dirty.is_dirty(4, LAYOUT));
    }

    #[test]
    fn lazy_policy_only_marks_key() {
        let graph = setup_chain_graph();
        let mut dirty = DirtySet::new();
        let lazy = LazyPolicy;

        lazy.propagate(1, LAYOUT, &graph, &mut dirty);

        assert!(dirty.is_dirty(1, LAYOUT));
        assert!(!dirty.is_dirty(2, LAYOUT));
        assert!(!dirty.is_dirty(3, LAYOUT));
        assert!(!dirty.is_dirty(4, LAYOUT));
    }

    #[test]
    fn policy_through_reference() {
        let graph = setup_chain_graph();
        let mut dirty = DirtySet::new();
        let eager = EagerPolicy;
        let policy: &dyn PropagationPolicy<u32> = &eager;

        policy.propagate(1, LAYOUT, &graph, &mut dirty);

        let dirty_keys: Vec<_> = dirty.iter(LAYOUT).collect();
        assert_eq!(dirty_keys.len(), 4);
    }

    #[test]
    fn eager_handles_diamond() {
        let mut graph = DirtyGraph::new();
        // Diamond: 1 <- 2, 1 <- 3, 2 <- 4, 3 <- 4
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

        let mut dirty = DirtySet::new();
        EagerPolicy.propagate(1, LAYOUT, &graph, &mut dirty);

        assert!(dirty.is_dirty(1, LAYOUT));
        assert!(dirty.is_dirty(2, LAYOUT));
        assert!(dirty.is_dirty(3, LAYOUT));
        assert!(dirty.is_dirty(4, LAYOUT));
        // Node 4 should only appear once in the dirty set
        assert_eq!(dirty.len(LAYOUT), 4);
    }
}
