<div align="center">

# Understory View 2D

**2D view and viewport primitives for Understory**

[![Latest published version.](https://img.shields.io/crates/v/understory_view2d.svg)](https://crates.io/crates/understory_view2d)
[![Documentation build status.](https://img.shields.io/docsrs/understory_view2d.svg)](https://docs.rs/understory_view2d)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_view2d
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory View 2D: 1D and 2D view/viewport primitives.

This crate provides small, headless models of world‑space views where the
view extents are typically expressed in device pixels. It focuses on:
- Camera / viewport state (pan + zoom).
- Coordinate conversion between world and view/device (pixel) space.
- View fitting and centering/alignment helpers.
- Simple zoom / pan constraints.

It does **not** own any scene graph or rendering backend. Callers are
expected to:
- Maintain their own scene or display tree.
- Use [`Viewport2D`] / [`Viewport1D`] to derive transforms and
  visible‑region bounds.
- Wire input events (for example, from `ui-events`) into pan/zoom
  operations at a higher layer.
- Optionally combine `world_units_per_pixel` helpers with display DPI and
  external unit libraries (for example `joto_constants`) to reason about
  physical sizes.

## Minimal 2D example

```rust
use kurbo::{Point, Rect};
use understory_view2d::Viewport2D;

// Device/view rect: 800x600 window.
let view_rect = Rect::new(0.0, 0.0, 800.0, 600.0);
let mut view = Viewport2D::new(view_rect);

// Optional world bounds for fitting/clamping.
view.set_world_bounds(Some(Rect::new(-100.0, -100.0, 100.0, 100.0)));
view.fit_world();

// Convert a device-space point into world space (for hit testing, etc.).
let device_pt = Point::new(400.0, 300.0);
let world_pt = view.view_to_world_point(device_pt);
```

## Minimal 1D example (timeline/axis)

```rust
use understory_view2d::Viewport1D;

// 0..800 view span in pixels.
let span = 0.0..800.0;
let mut view = Viewport1D::new(span);

// World bounds in \"time\" units.
view.set_world_bounds(Some(0.0..120.0));
view.fit_world();

// Convert a device-space X coordinate into world-space time.
let device_x = 400.0;
let world_t = view.view_to_world_x(device_x);
```

## Design notes

- Cameras are axis‑aligned with a **uniform** zoom factor.
- Panning operates in view space; zooming is expressed as a scalar.
- Rotation is intentionally left out of the initial design and can be
  added later as a backwards‑compatible extension.
- Controllers that interpret `ui-events` and more complex behaviors such
  as inertia are expected to live in higher‑level crates built on top of
  this one.

## Culling example

`Viewport2D` can be used to compute a visible world rectangle for culling.
For example, given a list of world‑space rectangles, you can retain only
those that intersect the current view:

```rust
use kurbo::Rect;
use understory_view2d::Viewport2D;

let view_rect = Rect::new(0.0, 0.0, 800.0, 600.0);
let view = Viewport2D::new(view_rect);

let visible_world = view.visible_world_rect();
let world_items: &[Rect] = &[
    Rect::new(-10.0, -10.0, 10.0, 10.0),
    Rect::new(1_000.0, 1_000.0, 1_100.0, 1_100.0),
];

let visible_items: Vec<Rect> = world_items
    .iter()
    .copied()
    .filter(|r| r.intersect(visible_world).area() > 0.0)
    .collect();
assert!(!visible_items.is_empty());
```

This crate is `no_std`.

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

