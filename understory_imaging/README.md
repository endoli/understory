<div align="center">

# Understory Imaging

**Backend-agnostic imaging IR and backend traits**

[![Latest published version.](https://img.shields.io/crates/v/understory_imaging.svg)](https://crates.io/crates/understory_imaging)
[![Documentation build status.](https://img.shields.io/docsrs/understory_imaging.svg)](https://docs.rs/understory_imaging)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_imaging
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Imaging: backend-agnostic imaging IR and backend traits.

This crate defines a small, plain‑old‑data (POD) friendly imaging
intermediate representation and traits for backends that consume it.
It sits between higher-level presentation / display layers and concrete
renderers (Vello CPU / Hybrid / Classic, Skia, etc.).

# Position in the stack

Conceptually there are three layers:

- **Presentation / display**: box trees, layout, styling, timelines,
  and interaction. This lives in other `understory_*` crates.
- **Imaging IR (this crate)**: paths, paints, images, and pictures
  expressed as POD state + draw operations, plus resource and backend
  traits.
- **Backends**: concrete renderers such as Vello CPU/Hybrid/Classic
  or Skia that implement [`ImagingBackend`] on top of wgpu, a CPU
  rasterizer, or other technology.

# Core concepts

- **Resources**: small, opaque handles ([`PathId`], [`ImageId`],
  [`PaintId`], [`PictureId`]) whose lifetimes are managed
  via [`ResourceBackend`].
- **Imaging operations**: [`StateOp`] (mutate state) and [`DrawOp`]
  (produce pixels), combined into [`ImagingOp`] for recording.
- **Backends**: [`ImagingBackend`] accepts imaging ops; helpers
  [`record_ops`] and [`record_picture`] turn short sequences into
  reusable recordings and picture resources.
- **Transform classes**: [`TransformClass`] and [`transform_diff_class`]
  provide a conservative language for deciding when cached results or
  recordings remain valid under a new transform.

The current API is intentionally minimal and experimental. Caching,
recordings, and advanced backend semantics are expected to evolve as
we integrate real backends; expect breaking changes while the design
is still being iterated.

# Recordings and resource environments

In the v1 model, recordings are conceptually just sequences of [`ImagingOp`]
that reference external resources by handle ([`PathId`], [`ImageId`],
[`PaintId`], [`PictureId`]). A recording is therefore *bound to the resource
environment* in which it was produced:

- The same resource IDs must exist and refer to compatible resources when
  the recording is replayed.
- Recordings do not embed inline resource data and cannot reconstruct
  missing resources on their own.

This keeps the implementation simple and efficient for v1 and matches
existing renderer architectures. Future versions may experiment with
inline resource references, recording-local resource tables, or ephemeral
arenas, but those are intentionally out of scope here.

# Example

A minimal sketch of how a backend might be used looks like:

```rust
let mut backend = MyBackend { /* ... */ };

let paint = backend.create_paint(PaintDesc {
    brush: Brush::Solid(Color::WHITE),
});
let path = backend.create_path(PathDesc {
    commands: Box::new([PathCmd::MoveTo { x: 0.0, y: 0.0 }]),
});

backend.state(StateOp::SetPaint(paint));
backend.draw(DrawOp::FillPath(path));

// Optionally capture a reusable recording:
let recording = record_ops(&mut backend, |b| {
    b.draw(DrawOp::StrokePath(path));
});
assert!(recording.ops.len() > 0);
```

For full design notes and background, see the `issue_understory_imaging.md`
RFC in the `docs/` directory of the Understory repository.

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

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[LICENSE-APACHE]: ../LICENSE-APACHE
[LICENSE-MIT]: ../LICENSE-MIT
[Rust code of conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: ../AUTHORS
