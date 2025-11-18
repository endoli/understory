<div align="center">

# Understory Box Tree

**Kurbo-native spatially indexed box tree**

[![Latest published version.](https://img.shields.io/crates/v/understory_box_tree.svg)](https://crates.io/crates/understory_box_tree)
[![Documentation build status.](https://img.shields.io/docsrs/understory_box_tree.svg)](https://docs.rs/understory_box_tree)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_box_tree
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Box Tree: a Kurbo-native, spatially indexed box tree.

Understory Box Tree is a reusable building block for UIs, canvas and vector editors, and CAD viewers.

- Represents a hierarchy of regions with local transforms, clips, z-order, and flags.
- Provides hit testing and rectangle intersection queries over world-space AABBs.
- Supports batched updates with a [`Tree::commit`] step that yields coarse damage regions.

It aims for a stable, minimal API and leaves room to evolve internals (for example a pluggable spatial index) without churn at call sites.

## Where this fits: three-tree model

We’re standardizing on a simple separation of concerns for UI stacks.
- Widget tree: interaction/state.
- Box tree: geometry/spatial indexing (this crate).
- Render tree: display list (future crate).

The box tree computes world-space AABBs from local bounds, transforms, and clips, and synchronizes them into a spatial index for fast hit testing and visibility queries.
This decouples scene structure from the spatial acceleration and makes debugging and incremental updates tractable.

## Not a layout engine

This crate does not perform layout (measurement or arrangement) or apply layout policies such as flex, grid, or stack.
Upstream code is expected to compute positions and sizes using whatever layout system you choose and then update this tree with the resulting world-space boxes, transforms, optional clips, and z-order.
Think of this as a scene and spatial index, not a layout system.

## Integration with Understory Index

This crate uses [`understory_index`] for spatial queries. You can choose the backend and scalar to
fit your workload (flat vector, R-tree or BVH). Float inputs are
assumed to be finite (no NaNs). AABBs are conservative for non-axis transforms and rounded clips.

See [`understory_index::Index`], [`understory_index::RTreeF32`]/[`understory_index::RTreeF64`]/[`understory_index::RTreeI64`], and
[`understory_index::BvhF32`]/[`understory_index::BvhF64`]/[`understory_index::BvhI64`] for details.

## API overview

- [`Tree`]: container managing nodes and the spatial index synchronization.
- [`LocalNode`]: per-node local data (bounds, transform, optional clip, z, flags).
  See [`LocalNode::flags`] for visibility/picking controls.
- [`NodeFlags`]: visibility and picking controls.
- [`NodeId`]: generational handle of a node.
- [`QueryFilter`]: restricts hit/intersect results (visible/pickable).
  See [`NodeFlags::VISIBLE`] and [`NodeFlags::PICKABLE`].

Key operations:
- [`Tree::insert`](Tree::insert) → [`NodeId`]
- [`Tree::set_local_transform`](Tree::set_local_transform) / [`Tree::set_local_clip`](Tree::set_local_clip) / [`Tree::set_local_bounds`](Tree::set_local_bounds) / [`Tree::set_flags`](Tree::set_flags)
- [`Tree::commit`](Tree::commit) → damage summary; updates world data and the spatial index.
- [`Tree::hit_test_point`](Tree::hit_test_point) and [`Tree::intersect_rect`](Tree::intersect_rect).
- [`Tree::z_index`](Tree::z_index) exposes the stacking order of a live [`NodeId`].
- [`Tree::parent_of`](Tree::parent_of) returns the parent of a live [`NodeId`].
- [`Tree::world_transform`](Tree::world_transform) / [`Tree::world_bounds`](Tree::world_bounds)
  expose the local→world transform and world-space AABB for a live [`NodeId`].

## Damage and debugging notes

- [`Tree::commit`] batches adds/updates/removals and produces coarse damage (added/removed AABBs and
  old/new pairs for moved nodes). This is enough to bound a paint traversal in most UIs.
- World AABBs are conservative under rotation/shear and rounded-rect clips are approximated by
  their axis-aligned bounds for acceleration; precise hit-filtering is applied where cheap.

## Examples

- `examples/basic_box_tree.rs`: builds a trivial tree, commits, and runs a couple of queries.
- `examples/visible_list.rs`: demonstrates using `intersect_rect` to compute a visible set,
  a building block for virtualization.

This crate is `no_std` and uses `alloc`.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE] or <http://www.apache.org/licenses/LICENSE-2.0>)

<!-- Needs to be defined here for rustdoc's benefit -->
[LICENSE]: LICENSE
