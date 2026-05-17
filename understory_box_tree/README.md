<div align="center">

# Understory Box Tree

**Kurbo-native spatially indexed box tree**

[![Latest published version.](https://img.shields.io/crates/v/understory_box_tree.svg)](https://crates.io/crates/understory_box_tree)
[![Documentation build status.](https://img.shields.io/docsrs/understory_box_tree.svg)](https://docs.rs/understory_box_tree)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/forest-rs/understory/ci.yml?logo=github&label=CI)](https://github.com/forest-rs/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_box_tree --heading-base-level=0
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

This crate also does not model stacking contexts, opacity, or blend modes. It provides a single
global z ordering (`z_index`) over boxes plus hit-testing and visibility queries. Higher-level
code is expected to introduce groups, stacking semantics, and paint order if needed.

## Integration with Understory Index

This crate uses [`understory_index`] for spatial queries. You can choose the backend and scalar to
fit your workload (flat vector, grid, R-tree, or BVH). Float inputs are
assumed to be finite (no NaNs). AABBs are loose for non-axis-aligned transforms and rounded
clips: we store an axis-aligned box that fully contains what is drawn, but it is not
guaranteed to be tight.

See [`understory_index::Index`],
[`understory_index::backends::GridF32`]/[`understory_index::backends::GridF64`]/[`understory_index::backends::GridI64`],
[`understory_index::backends::RTreeF32`]/[`understory_index::backends::RTreeF64`]/[`understory_index::backends::RTreeI64`],
and
[`understory_index::backends::BvhF32`]/[`understory_index::backends::BvhF64`]/[`understory_index::backends::BvhI64`]
for details.

## API overview

- [`Tree`]: container managing nodes and the spatial index synchronization.
- [`LocalNode`]: per-node local data (bounds, transform, optional clip, z, flags).
  See [`LocalNode::flags`] for visibility/picking/focusable controls.
- [`NodeFlags`]: visibility, picking, and focusable controls.
- [`NodeId`]: generational handle of a node.
- [`QueryFilter`]: restricts hit/intersect results (visible/pickable/focusable).
  See [`NodeFlags::VISIBLE`], [`NodeFlags::PICKABLE`], and [`NodeFlags::FOCUSABLE`].

Key operations:
- [`Tree::insert`] → [`NodeId`]
- [`Tree::set_local_transform`] / [`Tree::set_local_clip`] /
  [`Tree::set_local_bounds`] / [`Tree::set_flags`]
- [`Tree::commit`] → damage summary; updates world data and the spatial index.
- [`Tree::hit_test_point`] and [`Tree::intersect_rect`].
- [`Tree::z_index`] exposes the stacking order of a live [`NodeId`].
- [`Tree::parent_of`] returns the parent of a live [`NodeId`].
- [`Tree::flags`] returns the [`NodeFlags`] of a live [`NodeId`].
- [`Tree::world_transform`] / [`Tree::world_bounds`]
  expose the local→world transform and world-space AABB for a live [`NodeId`].
- [`Tree::local_transform`] / [`Tree::local_bounds`] /
  [`Tree::local_clip`] expose the node's current local geometry state for a
  live [`NodeId`].
- [`Tree::children_of`] returns the children of a live [`NodeId`].
- [`Tree::next_depth_first`] and [`Tree::prev_depth_first`] provide depth-first tree traversal.

## Damage and debugging notes

- [`Tree::commit`] batches adds/updates/removals and produces coarse damage (added/removed AABBs and
  old/new pairs for moved nodes). The reported rectangles may overlap and are not a minimal cover,
  but are sufficient to bound a paint traversal in most UIs.
- World AABBs are loose under rotation/shear and rounded-rect clips are approximated by
  their axis-aligned bounds for acceleration; precise hit-filtering is applied where cheap.

## Examples

- `examples/basic_box_tree.rs`: builds a trivial tree, commits, and runs a couple of queries.
- `examples/visible_list.rs`: demonstrates using `intersect_rect` to compute a visible set,
  a building block for virtualization.

This crate is `no_std` and uses `alloc`.

<!-- cargo-rdme end -->

[`LocalNode`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.LocalNode.html
[`LocalNode::flags`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.LocalNode.html#structfield.flags
[`NodeFlags`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.NodeFlags.html
[`NodeFlags::FOCUSABLE`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.NodeFlags.html#associatedconstant.FOCUSABLE
[`NodeFlags::PICKABLE`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.NodeFlags.html#associatedconstant.PICKABLE
[`NodeFlags::VISIBLE`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.NodeFlags.html#associatedconstant.VISIBLE
[`NodeId`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.NodeId.html
[`QueryFilter`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.QueryFilter.html
[`Tree`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html
[`Tree::children_of`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.children_of
[`Tree::commit`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.commit
[`Tree::flags`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.flags
[`Tree::hit_test_point`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.hit_test_point
[`Tree::insert`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.insert
[`Tree::intersect_rect`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.intersect_rect
[`Tree::local_bounds`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.local_bounds
[`Tree::local_clip`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.local_clip
[`Tree::local_transform`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.local_transform
[`Tree::next_depth_first`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.next_depth_first
[`Tree::parent_of`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.parent_of
[`Tree::prev_depth_first`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.prev_depth_first
[`Tree::set_flags`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.set_flags
[`Tree::set_local_bounds`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.set_local_bounds
[`Tree::set_local_clip`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.set_local_clip
[`Tree::set_local_transform`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.set_local_transform
[`Tree::world_bounds`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.world_bounds
[`Tree::world_transform`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.world_transform
[`Tree::z_index`]: https://docs.rs/understory_box_tree/latest/understory_box_tree/struct.Tree.html#method.z_index
[`understory_index`]: https://docs.rs/understory_index/latest/understory_index/
[`understory_index::backends::BvhF32`]: https://docs.rs/understory_index/latest/understory_index/backends/type.BvhF32.html
[`understory_index::backends::BvhF64`]: https://docs.rs/understory_index/latest/understory_index/backends/type.BvhF64.html
[`understory_index::backends::BvhI64`]: https://docs.rs/understory_index/latest/understory_index/backends/type.BvhI64.html
[`understory_index::backends::GridF32`]: https://docs.rs/understory_index/latest/understory_index/backends/type.GridF32.html
[`understory_index::backends::GridF64`]: https://docs.rs/understory_index/latest/understory_index/backends/type.GridF64.html
[`understory_index::backends::GridI64`]: https://docs.rs/understory_index/latest/understory_index/backends/type.GridI64.html
[`understory_index::backends::RTreeF32`]: https://docs.rs/understory_index/latest/understory_index/backends/type.RTreeF32.html
[`understory_index::backends::RTreeF64`]: https://docs.rs/understory_index/latest/understory_index/backends/type.RTreeF64.html
[`understory_index::backends::RTreeI64`]: https://docs.rs/understory_index/latest/understory_index/backends/type.RTreeI64.html
[`understory_index::Index`]: https://docs.rs/understory_index/latest/understory_index/type.Index.html

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE] or <http://www.apache.org/licenses/LICENSE-2.0>).

## Contribution

Contributions are welcome by pull request. The [Rust code of conduct] applies.
Please feel free to add your name to the [AUTHORS] file in any substantive pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be licensed as above, without any additional terms or conditions.

[LICENSE]: https://github.com/forest-rs/understory/blob/main/LICENSE-APACHE
[Rust code of conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: https://github.com/forest-rs/understory/blob/main/AUTHORS
