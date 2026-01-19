<div align="center">

# Understory Imaging Web Canvas

**Web Canvas (2D) backend for Understory imaging IR**

[![Latest published version.](https://img.shields.io/crates/v/understory_imaging_web_canvas.svg)](https://crates.io/crates/understory_imaging_web_canvas)
[![Documentation build status.](https://img.shields.io/docsrs/understory_imaging_web_canvas.svg)](https://docs.rs/understory_imaging_web_canvas)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_imaging_web_canvas
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Web Canvas (2D) backend for the Understory imaging IR.

This crate provides an [`ImagingBackend`] implementation backed by
`web_sys::CanvasRenderingContext2d` when targeting `wasm32`.

# Usage

Prefer `WebCanvasImagingBackend::new_html_canvas` when you have an `HtmlCanvasElement`.
It enables correct isolated layer compositing for `LayerOp { opacity, blend }` by allocating
temporary scratch canvases on-demand. If you only have a `CanvasRenderingContext2d`, use
`WebCanvasImagingBackend::new` (best-effort for group compositing).

```rust
#[cfg(target_arch = "wasm32")]
fn make_backend(
    canvas: web_sys::HtmlCanvasElement,
) -> Result<understory_imaging_web_canvas::WebCanvasImagingBackend, wasm_bindgen::JsValue> {
    understory_imaging_web_canvas::WebCanvasImagingBackend::new_html_canvas(canvas)
}
```

Notes:
- This is an early, best-effort renderer intended for debugging and prototyping.
- Supported: paths and axis-aligned rects; solid-color fills/strokes; stroke styles including
  dashes/caps/joins; clip layers (including stroke clips and fill-rule aware path clips).
- Supported: linear and radial gradients (best-effort; Canvas 2D does not support all Peniko
  gradient features, such as extend modes beyond pad).
- Placeholders: images and sweep gradients.
- Correct group opacity/blend (isolated layers) is only available when constructed from an
  `HtmlCanvasElement`; the context-only constructor is best-effort for group compositing.
  Canvas 2D does not have a built-in “isolated group” primitive, so matching the IR semantics
  requires rendering into a temporary offscreen buffer and compositing once at `PopLayer`.
- Performance: isolated layers allocate a scratch `<canvas>` on-demand. This is intended for
  prototyping and debugging, not production rendering.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE] or <http://www.apache.org/licenses/LICENSE-2.0>), or
- MIT license ([LICENSE-MIT] or <http://opensource.org/licenses/MIT>),

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

## Contribution

Contributions are welcome by pull request. The [Rust code of conduct] applies. Please feel free to
add your name to the [AUTHORS] file in any substantive pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be licensed as above, without any
additional terms or conditions.

[AUTHORS]: ../AUTHORS
[LICENSE-APACHE]: LICENSE-APACHE
[LICENSE-MIT]: LICENSE-MIT
[Rust code of conduct]: https://www.rust-lang.org/policies/code-of-conduct
