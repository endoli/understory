<div align="center">

# Understory Imaging Ref

**Reference backend for recording and state tracing**

[![Latest published version.](https://img.shields.io/crates/v/understory_imaging_ref.svg)](https://crates.io/crates/understory_imaging_ref)
[![Documentation build status.](https://img.shields.io/docsrs/understory_imaging_ref.svg)](https://docs.rs/understory_imaging_ref)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_imaging_ref
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Imaging Reference Backend.

This crate provides a small, stateful implementation of
[`ImagingBackend`] and [`ResourceBackend`]
for **IR recording and state tracing**.

It is intentionally *not* a “reference renderer”:
- It does **not** rasterize to pixels.
- It does **not** establish “golden” rendering behavior across backends.
- It is intended primarily for tests and debugging that want to assert on
  emitted ops and the imaging state at the time each op is applied.

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
