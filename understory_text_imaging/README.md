<div align="center">

# Understory Text Imaging

**Text → imaging helpers**

[![Latest published version.](https://img.shields.io/crates/v/understory_text_imaging.svg)](https://crates.io/crates/understory_text_imaging)
[![Documentation build status.](https://img.shields.io/docsrs/understory_text_imaging.svg)](https://docs.rs/understory_text_imaging)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_text_imaging
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Text → Imaging helpers.

This crate provides small, plain‑old‑data (POD) friendly helpers for
expressing text as [`understory_imaging`] operations. It does **not**
perform shaping
or font resolution itself; instead it assumes some upstream text
engine can provide positioned glyphs and, optionally, path resources
for glyph outlines.

The primary *helper* API is [`draw_text_run`], which:
- Accepts font bytes, text, size, and a paint id,
- Prefers bitmap glyphs when available (e.g. emoji fonts),
- Falls back to outline glyphs otherwise,
- Emits imaging ops (`StateOp`/`DrawOp`) suitable for any imaging backend.

The *core* API is glyph‑run based: a slice of [`GlyphInstance`] plus a
`PaintId` lowered via [`draw_glyph_run`] into imaging operations.

In a full text stack, a shaping engine (such as Parley) is expected to
produce glyph runs; `draw_text_run` is intentionally a convenience for
demos and simple cases rather than a general text layout API.

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
