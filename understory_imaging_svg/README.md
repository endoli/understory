<div align="center">

# Understory Imaging SVG

**SVG export backend for Understory imaging IR**

[![Latest published version.](https://img.shields.io/crates/v/understory_imaging_svg.svg)](https://crates.io/crates/understory_imaging_svg)
[![Documentation build status.](https://img.shields.io/docsrs/understory_imaging_svg.svg)](https://docs.rs/understory_imaging_svg)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_imaging_svg
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

SVG export backend for the Understory imaging IR.

This crate provides a small implementation of
[`ImagingBackend`] and [`ResourceBackend`] that records imaging ops and
can export them as an SVG document.

This is intended for debugging/inspection, not pixel-perfect rendering:
- Not all brush types are supported yet (solid colors are; other brushes use a fallback).
- Layer compositing semantics are approximated using SVG `<g>` with `opacity`/`mix-blend-mode`.
- Image drawing is represented as placeholders (no embedded pixels yet).

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
