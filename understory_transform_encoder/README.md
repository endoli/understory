<div align="center">

# Understory Transform Encoder

**Transform encoder for mapping user input events to viewport transformations**

[![Latest published version.](https://img.shields.io/crates/v/understory_transform_encoder.svg)](https://crates.io/crates/understory_transform_encoder)
[![Documentation build status.](https://img.shields.io/docsrs/understory_transform_encoder.svg)](https://docs.rs/understory_transform_encoder)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_transform_encoder
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Transform encoder for mapping user input events to viewport transformations.

This crate provides a **strongly typed, dimension-aware transform system** that maps user
input events (drag, scroll, pinch, rotate, keyboard) to viewport transformations with
compile-time guarantees and zero runtime failures.

## High-Level Usage: Transform Encoder

The primary interface is [`TransformEncoder`], which manages behavior configuration and
drag state to process input events and apply transforms to mutable target references:

```rust
use understory_transform_encoder::{TransformEncoder, Behaviors};
use understory_view2d::Viewport2D;
use ui_events::{keyboard::{Key, NamedKey}, pointer::PointerEvent};
use kurbo::{Rect, Vec2};

// Create a 2D viewport as the transform target
let mut viewport = Viewport2D::new(Rect::new(0.0, 0.0, 800.0, 600.0));

// Configure input behaviors
let behaviors = Behaviors::new()
    .drag(|m, _| m.is_empty(), |d| d.xy().pan(Vec2::new(1.0, 1.0)))
    .scroll_y(|m, _| m.is_empty(), |s| s.rotate(0.01))
    .pinch(|_, _| true, |p| p.uniform().scale(0.1));

// Create encoder and process events
let mut encoder = TransformEncoder::new(behaviors);
// encoder.encode(&pointer_event, &mut viewport);  // Process pointer events
// encoder.encode_keyboard(&key_event, &mut viewport);  // Process keyboard events
```

## Behavior Configuration: Input Event Processing

The [`Behaviors`] type configures how input events map to transforms using a fluent API:

```rust
use understory_transform_encoder::Behaviors;
use ui_events::keyboard::{Key, NamedKey};
use kurbo::Vec2;

let behaviors = Behaviors::new()
    // Pointer input: drag without modifiers → exact pan (no sensitivity, absolute positioning)
    .drag_exact(|m, _| m.is_empty())
    
    // Pointer input: drag with Shift → pan with 1:1 sensitivity
    .drag(|m, _| m.shift(), |d| d.xy().pan(Vec2::new(1.0, 1.0)))
    
    // Pointer input: drag with Ctrl → extract X axis → rotate
    .drag(|m, _| m.ctrl(), |d| d.x().rotate(0.01))
    
    // Pointer input: vertical scroll → rotate
    .scroll_y(|m, _| m.is_empty(), |s| s.rotate(0.01))
    
    // Pointer input: horizontal scroll → uniform scale from 1D
    .scroll_x(|m, _| m.is_empty(), |s| s.uniform().scale(0.1))
    
    // Gesture input: pinch → uniform scale
    .pinch(|_, _| true, |p| p.uniform().scale(0.5))
    
    // Gesture input: rotate → rotate transform
    .rotate(|_, _| true, |r| r.rotate(1.0))
    
    // Keyboard input: arrow keys → pan with fixed deltas
    .key(Key::Named(NamedKey::ArrowLeft), |_, state| state.is_down(),
         |k| k.delta().pan(Vec2::new(-10.0, 0.0)))
    .key(Key::Named(NamedKey::ArrowRight), |_, state| state.is_down(),
         |k| k.delta().pan(Vec2::new(10.0, 0.0)))
    
    // Keyboard input: Enter/Escape → scale with fixed factors
    .key(Key::Named(NamedKey::Enter), |_, state| state.is_down(),
         |k| k.factor().scale_about_pointer(1.1))
    .key(Key::Named(NamedKey::Escape), |_, state| state.is_down(),
         |k| k.factor().scale_about_pointer(0.9));
```

## Input-to-Output Mappings: Dimensional Transformations

The system provides builders for mapping between input dimensions and output transform types:

### 2D Input Sources (Drag, Scroll)
- [`InputValue2DBuilder::xy()`] - Keep original axis order: `(x, y) → (x, y)`
- [`InputValue2DBuilder::yx()`] - Swap X and Y axes: `(x, y) → (y, x)`
- [`InputValue2DBuilder::x()`] - Extract X component only: `(x, y) → x`
- [`InputValue2DBuilder::y()`] - Extract Y component only: `(x, y) → y`
- [`InputValue2DBuilder::magnitude()`] - Extract vector length signed by dominant axis
- [`InputValue2DBuilder::dominant()`] - Extract whichever component has larger magnitude

### 1D Input Sources (Scroll axes, Pinch, Rotate gestures)
- [`InputValue1DBuilder::to_x()`] - Emit to X axis only: `value → (value, 0)`
- [`InputValue1DBuilder::to_y()`] - Emit to Y axis only: `value → (0, value)`
- [`InputValue1DBuilder::uniform()`] - Emit to both axes: `value → (value, value)`
- [`InputValue1DBuilder::rotate()`] - Direct 1D→1D rotation: `value → rotate(value)`

### All Supported Mapping Combinations

| Input → Output | Examples |
|----------------|----------|
| **2D → 2D**    | Drag → Pan, Drag → Non-uniform Scale |
| **2D → 1D**    | Drag X → Rotate, Drag Magnitude → Rotate |
| **1D → 2D**    | Scroll → Uniform Scale, Pinch → Pan Both Axes |
| **1D → 1D**    | Rotate Gesture → Rotate Transform |

## Transform Actions: Low-Level Operations

At the lowest level, the system uses [`TransformAction`] types to represent transform
operations before backend-specific application:

- [`TransformAction::Pan`] - Translation operations with [`PanAction`] variants
- [`TransformAction::Scale`] - Scaling operations with [`ScaleAction`] variants  
- [`TransformAction::Rotate`] - Rotation operations with [`RotateAction`] variants

Each action type supports multiple operation modes:
- **Fixed values**: Absolute positioning (`To` variants)
- **Relative deltas**: Additive changes (`By`, `DeltaBy` variants)
- **Multiplicative factors**: Scaling operations (`By` variants for scale/rotate)

## Transform Targets: Backend Implementations

The [`TransformTarget`] trait allows different backends to handle transforms appropriately:

- **[`Viewport1D`]** - 1D timeline/axis manipulation, maps 2D operations to 1D equivalents
- **[`Viewport2D`]** - 2D canvas/map interaction with uniform scaling  
- **[`kurbo::Affine`]** - Raw affine transforms supporting non-uniform scaling and rotation

### Direct Transform Usage

```rust
use understory_transform_encoder::{TransformAction, TransformTarget, PanAction, ScaleAction};
use understory_view2d::Viewport2D;
use kurbo::{Rect, Vec2, Point};

let mut viewport = Viewport2D::new(Rect::new(0.0, 0.0, 800.0, 600.0));

// Apply transform actions directly
viewport.apply(TransformAction::Pan(PanAction::By(Vec2::new(10.0, 5.0))));
viewport.apply(TransformAction::Scale(ScaleAction::ByAbout {
    scale: Vec2::new(1.5, 1.5),
    anchor: Point::new(400.0, 300.0)
}));
```

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

