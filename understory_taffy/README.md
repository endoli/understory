<div align="center">

# Understory Taffy

**Taffy ↔ Understory box tree adapter**

[![Latest published version.](https://img.shields.io/crates/v/understory_taffy.svg)](https://crates.io/crates/understory_taffy)
[![Documentation build status.](https://img.shields.io/docsrs/understory_taffy.svg)](https://docs.rs/understory_taffy)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_taffy
Full documentation at https://github.com/endoli/understory -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Taffy ↔ box tree adapter.

This crate provides a thin bridge between the [`taffy`] layout engine and
[`understory_box_tree::Tree`]. It is intentionally small and opinionated:
Taffy owns layout (sizes and positions), while the box tree owns geometry
and spatial queries (hit testing, visibility).

In this workspace, `taffy` is defined in the workspace dependencies with the `taffy_tree`
feature enabled, and the examples (and this crate's dev-dependencies) enable the `flexbox`
feature so that `Style` participates in layout. If you depend on this crate from another
project, make sure that *your* `taffy` dependency enables at least one layout algorithm (for
example `flexbox` or `grid`), or use Taffy's default feature set, so that `Style` participates
in layout.

## Status

This crate is **experimental**. The core concepts (`TaffyBoxMap` as a non-owning adapter and
the `TaffyBoxTree` convenience wrapper) are expected to remain, but function names and error
types may be refined based on early adopter feedback.

## Design

The adapter keeps a mapping from Taffy nodes to box-tree nodes and offers
helpers to synchronize layout results into `LocalNode::local_bounds`.
It does *not* create Taffy nodes or run layout itself; callers are
expected to:

- Build and mutate the Taffy tree as usual.
- Call `compute_layout` on their root.
- Use [`TaffyBoxTree::attach_node`] / [`TaffyBoxTree::detach_node`] to
  mirror the subset of nodes that should participate in spatial queries.
- Call [`TaffyBoxTree::sync_layout`] (and optionally
  [`TaffyBoxTree::sync_layout_and_commit`]) to push layout results into
  the box tree.

This keeps responsibilities clear and lets you mix Taffy-managed nodes
with box-tree-only nodes (for overlays, popups, debug visuals, etc.).

<!-- cargo-rdme end -->

## API overview

- [`TaffyBoxTree`]: adapter that owns an [`understory_box_tree::Tree`] and a mapping from
  `taffy::NodeId` to [`understory_box_tree::NodeId`].
- [`TaffyBoxTree::attach_node`]: register an existing Taffy node and create a corresponding
  box-tree node using a `LocalNode` template (flags, clip, z, etc.).
- [`TaffyBoxTree::detach_node`]: remove a Taffy node from the mapping and drop its box-tree node.
- [`TaffyBoxTree::sync_layout`]: read `Layout`s from a `TaffyTree` and update
  `LocalNode::local_bounds` for all attached nodes (does **not** call `Tree::commit`).
- [`TaffyBoxTree::sync_layout_and_commit`]: convenience wrapper that calls [`sync_layout`] and then
  [`Tree::commit`], returning the resulting [`Damage`].
- [`layout_to_rect`]: helper for converting a Taffy [`Layout`] into a `kurbo::Rect` suitable for
  `LocalNode::local_bounds`.

## Example

Attaching a single Taffy node and syncing layout into the box tree:

```rust
use taffy::prelude::{AvailableSpace, Size, Style, TaffyTree};
use understory_box_tree::{LocalNode, NodeFlags};
use understory_taffy::TaffyBoxTree;
use kurbo::Rect;

let mut taffy: TaffyTree<()> = TaffyTree::new();
let root = taffy.new_leaf(Style::DEFAULT).unwrap();

let mut adapter = TaffyBoxTree::new();
let root_box = adapter.attach_node(
    root,
    None,
    LocalNode {
        flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
        ..LocalNode::default()
    },
);

taffy
    .compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(100.0),
            height: AvailableSpace::Definite(50.0),
        },
    )
    .unwrap();

let damage = adapter.sync_layout_and_commit(&taffy);
assert!(damage.union_rect().is_some());

// For a default Style with no content, Taffy currently computes a zero-size layout.
let bounds = adapter.tree.world_bounds(root_box).unwrap();
assert_eq!(bounds, Rect::new(0.0, 0.0, 0.0, 0.0));
```

## Features

- `std` (default): enables the `std` features in `taffy` and `understory_box_tree`. The adapter
  itself builds on top of `hashbrown` and Taffy’s high-level [`TaffyTree`] API and can participate
  in `no_std + alloc` configurations when its dependencies are configured accordingly.

## Minimum supported Rust Version (MSRV)

This crate follows the workspace MSRV, currently **Rust 1.88**.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE-APACHE]) and the MIT license
([LICENSE-MIT]).

You may not use this crate except in compliance with at least one of these licenses.

[LICENSE-APACHE]: ../LICENSE-APACHE
[LICENSE-MIT]: ../LICENSE-MIT
