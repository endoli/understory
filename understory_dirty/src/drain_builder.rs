// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Builder-based drain API.
//!
//! This API is intended for embedders who need more control than the
//! convenience drain helpers provide (e.g. determinism, targeted drains,
//! scratch reuse, and explainability hooks).
//!
//! The key idea is that drain behavior is configured via a small builder, and
//! only the selected options impose additional trait bounds:
//!
//! - Default order: `Any` (no `Ord` bound).
//! - Deterministic order: opt in via [`DrainBuilder::deterministic`] (requires `K: Ord`).

use alloc::vec::Vec;
use core::hash::Hash;
use core::marker::PhantomData;

use hashbrown::HashSet;

use crate::Channel;
use crate::DenseKey;
use crate::DirtyGraph;
use crate::DirtySet;
use crate::DrainSorted;
use crate::DrainSortedDeterministic;
use crate::TraversalScratch;
use crate::trace::DirtyTrace;

/// Type-level marker for “any” drain ordering (ties are not specified).
#[derive(Copy, Clone, Debug, Default)]
pub struct AnyOrder;

/// Type-level marker for deterministic drain ordering (ties broken by `Ord`).
#[derive(Copy, Clone, Debug, Default)]
pub struct DeterministicOrder;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum DrainMode {
    DirtyOnly,
    Affected,
}

#[derive(Copy, Clone, Debug)]
enum Within<'w, K> {
    All,
    Keys(&'w [K]),
    DependenciesOf(K),
}

/// A builder that configures and performs a drain.
///
/// Construct this via [`DirtyTracker::drain`](crate::DirtyTracker::drain).
///
/// # Targeted drains
///
/// The `within_*` methods provide targeted drains that do **not** require the
/// “global drain then restore” pattern: dirty roots outside the target remain
/// marked dirty for subsequent drains.
pub struct DrainBuilder<'d, 'g, 's, K, O = AnyOrder>
where
    K: Copy + Eq + Hash,
{
    dirty: &'d mut DirtySet<K>,
    graph: &'g DirtyGraph<K>,
    channel: Channel,
    mode: DrainMode,
    within: Within<'d, K>,
    scratch: Option<&'s mut TraversalScratch<K>>,
    trace: Option<&'s mut dyn DirtyTrace<K>>,
    _order: PhantomData<O>,
}

impl<K, O> core::fmt::Debug for DrainBuilder<'_, '_, '_, K, O>
where
    K: Copy + Eq + Hash,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DrainBuilder")
            .field("channel", &self.channel)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

impl<'d, 'g, K> DrainBuilder<'d, 'g, 'd, K, AnyOrder>
where
    K: Copy + Eq + Hash,
{
    pub(crate) fn new(
        dirty: &'d mut DirtySet<K>,
        graph: &'g DirtyGraph<K>,
        channel: Channel,
    ) -> Self {
        Self {
            dirty,
            graph,
            channel,
            mode: DrainMode::DirtyOnly,
            within: Within::All,
            scratch: None,
            trace: None,
            _order: PhantomData,
        }
    }
}

impl<'d, 'g, 's, K, O> DrainBuilder<'d, 'g, 's, K, O>
where
    K: Copy + Eq + Hash,
{
    /// Drains exactly the keys currently marked dirty (topologically sorted).
    ///
    /// This is the default; it is included for symmetry with
    /// [`DrainBuilder::affected`].
    #[must_use]
    pub fn dirty_only(mut self) -> Self {
        self.mode = DrainMode::DirtyOnly;
        self
    }

    /// Drains roots plus all transitive dependents (“affected” keys), then
    /// topologically sorts the result.
    ///
    /// This is the “lazy at mark-time, eager at drain-time” workflow, intended
    /// for use with [`LazyPolicy`](crate::LazyPolicy).
    #[must_use]
    pub fn affected(mut self) -> Self {
        self.mode = DrainMode::Affected;
        self
    }

    /// Restricts the drain to keys contained in `keys`.
    ///
    /// Dirty roots outside `keys` remain dirty for later drains.
    ///
    /// Note: `keys` is borrowed for the lifetime of the builder, so it must
    /// outlive the drain call.
    #[must_use]
    pub fn within_keys(mut self, keys: &'d [K]) -> Self {
        self.within = Within::Keys(keys);
        self
    }

    /// Restricts the drain to the transitive dependency-closure of `key` (plus
    /// `key` itself) in this channel.
    ///
    /// Dirty roots outside the closure remain dirty for later drains.
    #[must_use]
    pub fn within_dependencies_of(mut self, key: K) -> Self {
        self.within = Within::DependenciesOf(key);
        self
    }

    /// Reuses `scratch` for internal traversals (affected expansion, targeted
    /// dependency closure computation).
    ///
    /// If you want tracing, prefer [`DrainBuilder::trace`], which also
    /// configures scratch reuse.
    #[must_use]
    pub fn scratch<'s2>(
        self,
        scratch: &'s2 mut TraversalScratch<K>,
    ) -> DrainBuilder<'d, 'g, 's2, K, O> {
        let DrainBuilder {
            dirty,
            graph,
            channel,
            mode,
            within,
            trace,
            ..
        } = self;
        debug_assert!(
            trace.is_none(),
            "calling `DrainBuilder::scratch` after configuring trace is not supported; call `DrainBuilder::trace` instead",
        );
        DrainBuilder {
            dirty,
            graph,
            channel,
            mode,
            within,
            scratch: Some(scratch),
            trace: None,
            _order: PhantomData,
        }
    }

    /// Records a best-effort explanation while expanding affected keys.
    ///
    /// This records **one plausible cause path** (a spanning forest): when a
    /// key is reachable via multiple roots or paths, the first discovered path
    /// wins.
    ///
    /// This also configures scratch reuse; you do not need to call
    /// [`DrainBuilder::scratch`] separately.
    #[must_use]
    pub fn trace<'s2, T>(
        self,
        scratch: &'s2 mut TraversalScratch<K>,
        trace: &'s2 mut T,
    ) -> DrainBuilder<'d, 'g, 's2, K, O>
    where
        T: DirtyTrace<K>,
    {
        let DrainBuilder {
            dirty,
            graph,
            channel,
            mode,
            within,
            ..
        } = self;
        DrainBuilder {
            dirty,
            graph,
            channel,
            mode,
            within,
            scratch: Some(scratch),
            trace: Some(trace),
            _order: PhantomData,
        }
    }
}

impl<'d, 'g, 's, K> DrainBuilder<'d, 'g, 's, K, AnyOrder>
where
    K: Copy + Eq + Hash,
{
    /// Switches the drain to deterministic tie-breaking (`Ord`).
    #[must_use]
    pub fn deterministic(self) -> DrainBuilder<'d, 'g, 's, K, DeterministicOrder>
    where
        K: Ord + DenseKey,
    {
        let DrainBuilder {
            dirty,
            graph,
            channel,
            mode,
            within,
            scratch,
            trace,
            ..
        } = self;
        DrainBuilder {
            dirty,
            graph,
            channel,
            mode,
            within,
            scratch,
            trace,
            _order: PhantomData,
        }
    }
}

impl<'d, 'g, 's, K, O> DrainBuilder<'d, 'g, 's, K, O>
where
    K: Copy + Eq + Hash,
{
    fn is_allowed(within: &Within<'d, K>, key: K, allowed: Option<&HashSet<K>>) -> bool {
        match *within {
            Within::All => true,
            Within::Keys(keys) => keys.contains(&key),
            Within::DependenciesOf(_) => allowed.is_some_and(|set| set.contains(&key)),
        }
    }

    fn compute_allowed_dependencies(
        graph: &DirtyGraph<K>,
        channel: Channel,
        key: K,
        scratch: Option<&mut TraversalScratch<K>>,
    ) -> HashSet<K> {
        let mut allowed: HashSet<K> = HashSet::new();
        allowed.insert(key);

        match scratch {
            Some(s) => {
                s.reset();
                s.stack.push(key);
                while let Some(next) = s.stack.pop() {
                    for dep in graph.dependencies(next, channel) {
                        if allowed.insert(dep) {
                            s.stack.push(dep);
                        }
                    }
                }
            }
            None => {
                let mut stack = Vec::new();
                stack.push(key);
                while let Some(next) = stack.pop() {
                    for dep in graph.dependencies(next, channel) {
                        if allowed.insert(dep) {
                            stack.push(dep);
                        }
                    }
                }
            }
        }

        allowed
    }

    fn take_roots(
        dirty: &mut DirtySet<K>,
        channel: Channel,
        within: &Within<'d, K>,
        allowed: Option<&HashSet<K>>,
    ) -> Vec<K> {
        match within {
            Within::All => dirty.drain(channel).collect(),
            Within::Keys(_) | Within::DependenciesOf(_) => {
                let roots: Vec<K> = dirty
                    .iter(channel)
                    .filter(|&k| Self::is_allowed(within, k, allowed))
                    .collect();
                for &k in &roots {
                    let _ = dirty.take(k, channel);
                }
                roots
            }
        }
    }

    fn collect_affected<'t>(
        graph: &DirtyGraph<K>,
        channel: Channel,
        roots: Vec<K>,
        within: &Within<'d, K>,
        allowed: Option<&HashSet<K>>,
        scratch: Option<&'t mut TraversalScratch<K>>,
        mut trace: Option<&'t mut dyn DirtyTrace<K>>,
    ) -> Vec<K> {
        let mut out = Vec::new();

        // Affected drains need a visited set that persists across roots.
        match scratch {
            Some(s) => {
                s.reset();

                for root in roots {
                    if !Self::is_allowed(within, root, allowed) {
                        continue;
                    }
                    let newly = s.visited.insert(root);
                    if newly {
                        out.push(root);
                        s.stack.push(root);
                    }
                    if let Some(t) = trace.as_deref_mut() {
                        t.root(root, channel, newly);
                    }
                }

                while let Some(because) = s.stack.pop() {
                    for dependent in graph.dependents(because, channel) {
                        if !Self::is_allowed(within, dependent, allowed) {
                            continue;
                        }
                        let newly = s.visited.insert(dependent);
                        if let Some(t) = trace.as_deref_mut() {
                            t.caused_by(dependent, because, channel, newly);
                        }
                        if newly {
                            out.push(dependent);
                            s.stack.push(dependent);
                        }
                    }
                }
            }
            None => {
                let mut visited: HashSet<K> = HashSet::new();
                let mut stack: Vec<K> = Vec::new();

                for root in roots {
                    if !Self::is_allowed(within, root, allowed) {
                        continue;
                    }
                    let newly = visited.insert(root);
                    if newly {
                        out.push(root);
                        stack.push(root);
                    }
                    if let Some(t) = trace.as_deref_mut() {
                        t.root(root, channel, newly);
                    }
                }

                while let Some(because) = stack.pop() {
                    for dependent in graph.dependents(because, channel) {
                        if !Self::is_allowed(within, dependent, allowed) {
                            continue;
                        }
                        let newly = visited.insert(dependent);
                        if let Some(t) = trace.as_deref_mut() {
                            t.caused_by(dependent, because, channel, newly);
                        }
                        if newly {
                            out.push(dependent);
                            stack.push(dependent);
                        }
                    }
                }
            }
        }

        out
    }
}

impl<'d, 'g, 's, K> DrainBuilder<'d, 'g, 's, K, AnyOrder>
where
    K: Copy + Eq + Hash,
{
    /// Executes the drain and returns an iterator in topological order.
    pub fn run(self) -> DrainSorted<'g, K> {
        let DrainBuilder {
            dirty,
            graph,
            channel,
            mode,
            within,
            mut scratch,
            trace,
            ..
        } = self;

        let allowed_set_storage;
        let allowed = match within {
            Within::DependenciesOf(key) => {
                allowed_set_storage =
                    Self::compute_allowed_dependencies(graph, channel, key, scratch.as_deref_mut());
                Some(&allowed_set_storage)
            }
            Within::All | Within::Keys(_) => None,
        };

        let roots = Self::take_roots(dirty, channel, &within, allowed);

        let keys = match mode {
            DrainMode::DirtyOnly => roots,
            DrainMode::Affected => {
                Self::collect_affected(graph, channel, roots, &within, allowed, scratch, trace)
            }
        };

        let cap = keys.len();
        DrainSorted::from_iter_with_capacity(keys.into_iter(), cap, graph, channel)
    }
}

impl<'d, 'g, 's, K> DrainBuilder<'d, 'g, 's, K, DeterministicOrder>
where
    K: Copy + Eq + Hash + Ord + DenseKey,
{
    /// Executes the drain and returns an iterator in deterministic topological order.
    pub fn run(self) -> DrainSortedDeterministic<'g, K> {
        let DrainBuilder {
            dirty,
            graph,
            channel,
            mode,
            within,
            mut scratch,
            trace,
            ..
        } = self;

        let allowed_set_storage;
        let allowed = match within {
            Within::DependenciesOf(key) => {
                allowed_set_storage =
                    Self::compute_allowed_dependencies(graph, channel, key, scratch.as_deref_mut());
                Some(&allowed_set_storage)
            }
            Within::All | Within::Keys(_) => None,
        };

        let roots = Self::take_roots(dirty, channel, &within, allowed);

        let keys = match mode {
            DrainMode::DirtyOnly => roots,
            DrainMode::Affected => {
                Self::collect_affected(graph, channel, roots, &within, allowed, scratch, trace)
            }
        };

        let cap = keys.len();
        DrainSortedDeterministic::from_iter_with_capacity(keys.into_iter(), cap, graph, channel)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use alloc::vec;

    use crate::CycleHandling;
    use crate::DirtyTracker;
    use crate::trace::OneParentRecorder;

    const LAYOUT: Channel = Channel::new(0);

    #[test]
    fn within_keys_does_not_clear_outside_roots() {
        let mut t = DirtyTracker::<u32>::new();
        t.mark(1, LAYOUT);
        t.mark(2, LAYOUT);

        let subset = [1];
        let order: Vec<_> = t
            .drain(LAYOUT)
            .dirty_only()
            .within_keys(&subset)
            .run()
            .collect();
        assert_eq!(order, vec![1]);
        assert!(t.is_dirty(2, LAYOUT));
    }

    #[test]
    fn within_dependencies_of_filters_dirty_only() {
        let mut t = DirtyTracker::<u32>::with_cycle_handling(CycleHandling::Error);
        // 1 <- 2 <- 3 and unrelated 9.
        t.add_dependency(2, 1, LAYOUT).unwrap();
        t.add_dependency(3, 2, LAYOUT).unwrap();

        t.mark(1, LAYOUT);
        t.mark(2, LAYOUT);
        t.mark(3, LAYOUT);
        t.mark(9, LAYOUT);

        let order: Vec<_> = t
            .drain(LAYOUT)
            .dirty_only()
            .within_dependencies_of(3)
            .deterministic()
            .run()
            .collect();
        assert_eq!(order, vec![1, 2, 3]);
        assert!(t.is_dirty(9, LAYOUT));
    }

    #[test]
    fn affected_with_trace_records_one_plausible_path() {
        let mut t = DirtyTracker::<u32>::with_cycle_handling(CycleHandling::Error);
        // 1 <- 2 <- 3
        t.add_dependency(2, 1, LAYOUT).unwrap();
        t.add_dependency(3, 2, LAYOUT).unwrap();

        t.mark(1, LAYOUT);

        let mut scratch = TraversalScratch::new();
        let mut rec = OneParentRecorder::new();
        let order: Vec<_> = t
            .drain(LAYOUT)
            .affected()
            .trace(&mut scratch, &mut rec)
            .run()
            .collect();

        assert_eq!(order, vec![1, 2, 3]);
        assert_eq!(rec.explain_path(3, LAYOUT).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn deterministic_diamond_is_total() {
        let mut t = DirtyTracker::<u32>::with_cycle_handling(CycleHandling::Error);
        // 1 <- 2, 1 <- 3, 2 <- 4, 3 <- 4
        t.add_dependency(2, 1, LAYOUT).unwrap();
        t.add_dependency(3, 1, LAYOUT).unwrap();
        t.add_dependency(4, 2, LAYOUT).unwrap();
        t.add_dependency(4, 3, LAYOUT).unwrap();

        t.mark(1, LAYOUT);
        t.mark(2, LAYOUT);
        t.mark(3, LAYOUT);
        t.mark(4, LAYOUT);

        let order: Vec<_> = t.drain(LAYOUT).dirty_only().deterministic().run().collect();
        assert_eq!(order, vec![1, 2, 3, 4]);
    }
}
