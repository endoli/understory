<div align="center">

# Understory Focus

**Focus navigation primitives for Understory**

[![Latest published version.](https://img.shields.io/crates/v/understory_focus.svg)](https://crates.io/crates/understory_focus)
[![Documentation build status.](https://img.shields.io/docsrs/understory_focus.svg)](https://docs.rs/understory_focus)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_focus
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Focus: focus navigation primitives.

This crate models focus navigation as a combination of:
- **Navigation intents** ([`Navigation`]) such as [`Navigation::Next`], [`Navigation::Prev`],
  or arrow directions.
- **Per-node focus properties** ([`FocusProps`]) such as enabled state, explicit order, and
  optional grouping or policy hints.
- A **spatial view of candidates** ([`FocusEntry`] / [`FocusSpace`]) that describes where
  focusable nodes live in a chosen 2D coordinate space (for example, a surface/world space
  or a container-local space).
- Pluggable **policies** ([`FocusPolicy`]) that select the next focused node given an
  origin, a direction, and a read-only view of focusable candidates.

## Minimal example

A simple focus loop over two buttons laid out left-to-right:

```rust
use kurbo::Rect;
use understory_focus::{
    DefaultPolicy, FocusEntry, FocusPolicy, FocusSpace, Navigation, WrapMode,
};

let entries = vec![
    FocusEntry {
        id: 1_u32,
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        order: None,
        group: None,
        enabled: true,
        scope_depth: 0,
    },
    FocusEntry {
        id: 2_u32,
        rect: Rect::new(20.0, 0.0, 30.0, 10.0),
        order: None,
        group: None,
        enabled: true,
        scope_depth: 0,
    },
];

let space = FocusSpace { nodes: &entries };
let policy = DefaultPolicy { wrap: WrapMode::Scope };

// Tab moves from the first button to the second…
assert_eq!(policy.next(1, Navigation::Next, &space), Some(2));
// …and wraps back to the first.
assert_eq!(policy.next(2, Navigation::Next, &space), Some(1));
```

## Patterns: groups and policy hints

[`FocusSymbol`] is a small, copyable handle you can use to describe
higher-level focus intent without baking policy into the geometry layer.

- Use [`FocusProps::group`] to keep navigation within a logical cluster
  (for example, a grid, toolbar, or inspector section) before jumping
  elsewhere.
- Use [`FocusProps::policy_hint`] to mark containers that should use a
  specific traversal style (for example, reading-order vs. grid-like).

```rust
use understory_focus::{FocusProps, FocusSymbol};

const GROUP_GRID: FocusSymbol = FocusSymbol(1);
const HINT_GRID_POLICY: FocusSymbol = FocusSymbol(10);

// A cell inside a grid: share GROUP_GRID so a policy can keep arrows
// within the grid until the user explicitly exits the scope.
let cell_props = FocusProps {
    group: Some(GROUP_GRID),
    ..FocusProps::default()
};

// The grid container itself: mark it with a policy hint so the host
// can choose an appropriate FocusPolicy implementation.
let grid_props = FocusProps {
    policy_hint: Some(HINT_GRID_POLICY),
    ..FocusProps::default()
};
```

The core types are generic over the node identifier `K`, so callers can use any small,
copyable handle (for example `understory_box_tree::NodeId` when used with the box tree,
or an application-specific id).
Geometry is expressed in terms of [`kurbo::Rect`], which matches the rest of the Understory
crates and allows directional policies to reason about spatial layout. A [`FocusSpace`]
should use a consistent coordinate space for all of its entries (for example, the world
space of a box tree or the local space of a focus scope).

## Features

- `std` (default): enables `std` support for dependencies such as `kurbo`.
- `libm`: enables `no_std` + `alloc` builds that rely on `libm` for floating-point math;
  typically used when integrating into embedded or `no_std` environments.
- `box_tree_adapter`: enables the [`adapters::box_tree`] module and pulls in
  `understory_box_tree` and `understory_index` so you can build a [`FocusSpace`] directly
  from an `understory_box_tree::Tree`.

This crate is `no_std` and uses `alloc`.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE] or <http://www.apache.org/licenses/LICENSE-2.0>)

<!-- Needs to be defined here for rustdoc's benefit -->
[LICENSE]: LICENSE
