<div align="center">

# Understory Precise Hit

**Geometry-level precise hit testing for 2D UI and graphics**

[![Latest published version.](https://img.shields.io/crates/v/understory_precise_hit.svg)](https://crates.io/crates/understory_precise_hit)
[![Documentation build status.](https://img.shields.io/docsrs/understory_precise_hit.svg)](https://docs.rs/understory_precise_hit)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_precise_hit
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Geometry-level precise hit testing utilities.

This crate provides small, reusable primitives for doing narrow-phase
hit testing in local 2D coordinates, built on top of [`kurbo`]. It is
intentionally decoupled from any particular scene tree or event router.

# Typical usage

- Use your own broad-phase index (e.g., `understory_index` or
  `understory_box_tree`) to cull candidates.
- Transform the query point into a shape's local coordinates.
- Call [`PreciseHitTest::hit_test_local`] on the shape.
- Use the returned [`HitScore`] only for *scoring and tie-breaking*.
  Any rich metadata (segment indices, glyphs, winding info, etc.) should
  be carried alongside the score in your own structures (for example as
  the `meta` payload in a responder hit type).

# Key types

- [`HitParams`] – per-query parameters such as fill/stroke tolerances and
  a hint for preferring fill vs stroke when both are possible.
- [`HitScore`] – a small scoring record `{ distance, kind }` used for
  ranking candidates. Lower distance is preferred; [`HitKind`] is a coarse
  class (fill, stroke, handle, other).
- [`PreciseHitTest`] – a trait implemented by shapes that can answer
  “does this local-space point hit me?” queries.

## Shapes and scope

This crate includes [`PreciseHitTest`] implementations for several
[`kurbo`] primitives:

- [`Rect`] – axis-aligned rectangle, with configurable fill tolerance.
- [`Circle`] – circle, treated as a filled disk.
- [`RoundedRect`] – rounded rectangle, treated as a filled shape; tolerant
  hits are based on its bounding box.
- [`BezPath`] – **fill-only** path hit using [`kurbo::Shape::contains`].
  The fill rule is whatever `kurbo` uses for `contains`; it is not
  currently configurable here. Stroke hits for paths are intentionally left
  to higher-level engines, which can wrap their own stroke representations
  and still implement [`PreciseHitTest`].

The [`stroke`] module provides helpers for stroke-oriented tests (for
example, a simple [`stroke::StrokedLine`] type). These are minimal
building blocks and not a full stroke model; engines are expected to build
richer stroke behavior on top.

<!-- cargo-rdme end -->

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

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be licensed as above, without any additional terms or conditions.

[Rust Code of Conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: ../AUTHORS
[LICENSE-APACHE]: LICENSE-APACHE
[LICENSE-MIT]: LICENSE-MIT
