<div align="center">

# Understory Dirty

**Generic dirty-tracking and invalidation primitives**

[![Latest published version.](https://img.shields.io/crates/v/understory_dirty.svg)](https://crates.io/crates/understory_dirty)
[![Documentation build status.](https://img.shields.io/docsrs/understory_dirty.svg)](https://docs.rs/understory_dirty)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_dirty
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Dirty: generic dirty-tracking and invalidation primitives.

This crate provides building blocks for incremental computation systems
where changes to upstream data must propagate to downstream consumers.
It models invalidation as a combination of:

- **Channels** ([`Channel`], [`ChannelSet`]): Named domains for dirty tracking
  (e.g., layout, paint, accessibility).
- **Dependency graphs** ([`DirtyGraph`]): DAG of "A depends on B" edges,
  with cycle detection and bidirectional traversal.
- **Dirty sets** ([`DirtySet`]): Accumulated dirty keys per channel with
  generation tracking for stale-computation detection.
- **Propagation policies** ([`PropagationPolicy`], [`EagerPolicy`], [`LazyPolicy`]):
  Pluggable strategies for how dirty marks spread through the graph.
- **Topological drain** ([`DrainSorted`]): Kahn's algorithm to yield dirty keys
  in dependency order.
- **Scratch buffers** ([`TraversalScratch`]): Reusable traversal state for
  tight loops (many marks per frame) to avoid repeated allocations.

## Quick Start

```rust
use understory_dirty::{Channel, DirtyTracker, EagerPolicy};

const LAYOUT: Channel = Channel::new(0);
const PAINT: Channel = Channel::new(1);

let mut tracker = DirtyTracker::<u32>::new();

// Build dependency graph: 3 depends on 2, 2 depends on 1
tracker.add_dependency(2, 1, LAYOUT).unwrap();
tracker.add_dependency(3, 2, LAYOUT).unwrap();

// Mark with eager propagation (marks 1, 2, 3)
tracker.mark_with(1, LAYOUT, &EagerPolicy);

// Or mark manually without propagation
tracker.mark(1, PAINT);

// Drain in topological order: 1, 2, 3
for key in tracker.drain_sorted(LAYOUT) {
    // recompute_layout(key);
}
```

## Using Components Separately

While [`DirtyTracker`] provides a convenient combined API, you can also
use the underlying types directly for more control:

```rust
use understory_dirty::{
    Channel, CycleHandling, DirtyGraph, DirtySet, EagerPolicy, PropagationPolicy,
};

const LAYOUT: Channel = Channel::new(0);

// Build the dependency graph
let mut graph = DirtyGraph::<u32>::new();
graph.add_dependency(2, 1, LAYOUT, CycleHandling::Error).unwrap();
graph.add_dependency(3, 2, LAYOUT, CycleHandling::Error).unwrap();

// Maintain dirty state separately
let mut dirty = DirtySet::new();
let eager = EagerPolicy;

// Propagate dirty marks
eager.propagate(1, LAYOUT, &graph, &mut dirty);

assert!(dirty.is_dirty(1, LAYOUT));
assert!(dirty.is_dirty(2, LAYOUT));
assert!(dirty.is_dirty(3, LAYOUT));
```

## Propagation Policies

The crate provides two built-in policies:

- [`EagerPolicy`]: Immediately marks all transitive dependents when a key
  is marked dirty. Use this when you need to know the full dirty set
  immediately after marking. Use with [`DirtyTracker::drain_sorted`].

- [`LazyPolicy`]: Only marks the key itself at mark-time; no propagation
  occurs. Use [`DirtyTracker::drain_affected_sorted`] to expand and process
  all affected keys (marked roots + their transitive dependents) at drain
  time. This avoids redundant traversals when many marks happen in succession.

You can implement [`PropagationPolicy`] for custom strategies (e.g.,
depth-limited propagation, priority-based marking).

## Performance Notes

For eager propagation in hot loops, consider reusing a [`TraversalScratch`]
and calling [`EagerPolicy::propagate_with_scratch`] to avoid per-mark
allocation during graph traversal.

## Choosing a Drain Function

- [`drain_sorted`] / [`DirtyTracker::drain_sorted`]: Drains exactly the keys
  that are currently marked dirty, in topological order. Use with [`EagerPolicy`]
  or when you only want to process explicitly marked keys.

- [`drain_affected_sorted`] / [`DirtyTracker::drain_affected_sorted`]: Expands
  the dirty set to include all transitive dependents before draining. Use with
  [`LazyPolicy`] to get the "lazy at mark-time, eager at drain-time" workflow.

## Cycle Detection

[`DirtyGraph::add_dependency`] supports configurable cycle handling via
[`CycleHandling`]:

- `DebugAssert` (default): Panics in debug builds, ignores in release.
- `Error`: Returns `Err(CycleError)` if a cycle would be created.
- `Ignore`: Silently ignores the dependency.
- `Allow`: Skips cycle detection entirely (useful when cycles are intentional).

## `no_std` Support

This crate is `no_std` and uses `alloc`. It does not depend on `std`.

## Features

This crate currently has no optional features. All functionality is always
available.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE] or <http://www.apache.org/licenses/LICENSE-2.0>)

<!-- Needs to be defined here for rustdoc's benefit -->
[LICENSE]: LICENSE
