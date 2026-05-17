<div align="center">

# Understory Virtual List

**Core 1D virtualization primitives for dense index strips**

[![Latest published version.](https://img.shields.io/crates/v/understory_virtual_list.svg)](https://crates.io/crates/understory_virtual_list)
[![Documentation build status.](https://img.shields.io/docsrs/understory_virtual_list.svg)](https://docs.rs/understory_virtual_list)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/forest-rs/understory/ci.yml?logo=github&label=CI)](https://github.com/forest-rs/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_virtual_list --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Virtual List: core 1D virtualization primitives.

This crate provides a small, renderer-agnostic core for virtualizing a dense strip
of items indexed `0..len`. It is intended to be shared across different UI stacks
and list/stack implementations.

The core concepts are:

- [`Scalar`]: a small abstraction over `f32`/`f64` used for extents, offsets,
  and scroll positions.
- [`ExtentModel`]: a trait describing a 1D strip of items with per-item extents
  and prefix-sum-style queries.
- [`compute_visible_strip`]: a helper that, given a scroll offset, viewport
  extent, and asymmetric overscan distances, returns which indices should be
  realized plus how much padding exists before and after them.
- [`VirtualList`]: a small controller that wraps an [`ExtentModel`] implementation,
  scroll state, viewport extent, and overscan, and caches the most recent
  [`VisibleStrip`]. It also provides index-based scrolling via [`ScrollAlign`]
  and convenience methods for visibility queries and scroll clamping.
- [`GridTrackModel`]: an adapter that maps a per-track [`ExtentModel`] onto a
  per-cell view for grid-like layouts (tracks × cells).
- [`TailAnchoredExtentModel`]: a wrapper that adds tail-anchoring helpers
  for chat/log-style lists that stick to the end of content.

This crate deliberately does **not** know about widgets, display trees, or any
particular UI framework. Host frameworks are responsible for:

- Owning the actual data and view/widget instances.
- Calling [`VirtualList::visible_strip`] when scroll or viewport changes.
- Diffing the returned `[start, end)` index range to create/destroy children.
  Use [`VirtualList::visible_range`] for the materialized range including
  overscan, and [`VirtualList::viewport_range`] for the range that overlaps
  the viewport itself.
- Calling [`VirtualList::set_len`] when the backing collection length changes
  and the model implements [`ResizableExtentModel`].
- Feeding measured item sizes back into an [`ExtentModel`] (for example via
  [`PrefixSumExtentModel`]).

## Minimal example

A very simple fixed-height list:

```rust
use understory_virtual_list::{FixedExtentModel, VirtualList};

// 100 items, each 20 logical pixels tall.
let model = FixedExtentModel::new(100, 20.0);
let mut list = VirtualList::new(model, 200.0, 40.0);

// Scroll to 100px from the start.
list.set_scroll_offset(100.0);

let strip = list.visible_strip();
assert!(strip.start < strip.end);
assert!(strip.content_extent > 0.0);

// Host frameworks would now instantiate views for indices in `strip.range()`
// and position them after `before_extent` worth of spacer.
assert_eq!(strip.range(), list.visible_range());

// To report the non-overscanned range that overlaps the viewport:
let range_in_viewport = list.viewport_range();
assert!(range_in_viewport.start <= range_in_viewport.end);
```

For non-uniform item sizes, use either [`PrefixSumExtentModel`] if all items are readily
available and it is feasible to load them up-front, or [`SparsePrefixSumExtentModel`] if there
are too many items to keep loaded at once and feed measured extents back into it after layout.
A typical pattern is:
- start with a rough estimate for all items,
- measure actual extents after layout and call [`PrefixSumExtentModel::set_extent`] /
  [`SparsePrefixSumExtentModel::set_extent`] or [`PrefixSumExtentModel::rebuild`] /
  [`SparsePrefixSumExtentModel::rebuild`],
- and use [`PrefixSumExtentModel::total_extent_for_len`] /
  [`SparsePrefixSumExtentModel::total_extent_for_len`] and
  [`PrefixSumExtentModel::index_at_offset_for_len`] /
  [`SparsePrefixSumExtentModel::index_at_offset_for_len`] to keep scroll behavior
  stable as measurements refine.

All extents and offsets live in a caller-chosen 1D coordinate space
(typically logical pixels) and are expected to be finite and non-negative.

## Grid-like example with tracks and cells

For grids, use [`GridTrackModel`] to adapt a per-track model to per-cell
indices. In a vertical grid, tracks typically correspond to rows and cells
to columns:

```rust
use core::num::NonZeroUsize;
use understory_virtual_list::{FixedExtentModel, GridTrackModel, VirtualList};

// Four tracks (rows), each 20 logical pixels tall.
let row_model = FixedExtentModel::new(4, 20.0);
// A 3-column grid over 12 cells (4 rows × 3 columns).
let columns = NonZeroUsize::new(3).unwrap();
let grid_model = GridTrackModel::new(row_model, columns, 12);
let mut list = VirtualList::new(grid_model, 40.0, 0.0);

let strip = list.visible_strip();
assert!(strip.start < strip.end);
// Host code can map each visible cell index `i` to:
//   let track = list.model().track_of_cell(i);
//   let cell_in_track = list.model().cell_in_track(i);
```

This crate is `no_std` and uses `alloc`.

<!-- cargo-rdme end -->

[`compute_visible_strip`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/fn.compute_visible_strip.html
[`ExtentModel`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/trait.ExtentModel.html
[`GridTrackModel`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.GridTrackModel.html
[`PrefixSumExtentModel`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.PrefixSumExtentModel.html
[`PrefixSumExtentModel::index_at_offset_for_len`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.PrefixSumExtentModel.html#method.index_at_offset_for_len
[`PrefixSumExtentModel::rebuild`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.PrefixSumExtentModel.html#method.rebuild
[`PrefixSumExtentModel::set_extent`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.PrefixSumExtentModel.html#method.set_extent
[`PrefixSumExtentModel::total_extent_for_len`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.PrefixSumExtentModel.html#method.total_extent_for_len
[`ResizableExtentModel`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/trait.ResizableExtentModel.html
[`Scalar`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/trait.Scalar.html
[`ScrollAlign`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/enum.ScrollAlign.html
[`SparsePrefixSumExtentModel`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.SparsePrefixSumExtentModel.html
[`SparsePrefixSumExtentModel::index_at_offset_for_len`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.SparsePrefixSumExtentModel.html#method.index_at_offset_for_len
[`SparsePrefixSumExtentModel::rebuild`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.SparsePrefixSumExtentModel.html#method.rebuild
[`SparsePrefixSumExtentModel::set_extent`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.SparsePrefixSumExtentModel.html#method.set_extent
[`SparsePrefixSumExtentModel::total_extent_for_len`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.SparsePrefixSumExtentModel.html#method.total_extent_for_len
[`TailAnchoredExtentModel`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.TailAnchoredExtentModel.html
[`VirtualList`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.VirtualList.html
[`VirtualList::set_len`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.VirtualList.html#method.set_len
[`VirtualList::viewport_range`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.VirtualList.html#method.viewport_range
[`VirtualList::visible_range`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.VirtualList.html#method.visible_range
[`VirtualList::visible_strip`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.VirtualList.html#method.visible_strip
[`VisibleStrip`]: https://docs.rs/understory_virtual_list/latest/understory_virtual_list/struct.VisibleStrip.html

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE] or <http://www.apache.org/licenses/LICENSE-2.0>), or
- MIT license ([LICENSE-MIT] or <http://opensource.org/licenses/MIT>),

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Contribution

Contributions are welcome by pull request. The [Rust code of conduct] applies.
Please feel free to add your name to the [AUTHORS] file in any substantive pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[LICENSE-APACHE]: https://github.com/forest-rs/understory/blob/main/LICENSE-APACHE
[LICENSE-MIT]: https://github.com/forest-rs/understory/blob/main/LICENSE-MIT
[Rust code of conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: https://github.com/forest-rs/understory/blob/main/AUTHORS
