<div align="center">

# Understory Guide

**Headless 2D guide geometry primitives for Understory**

[![Latest published version.](https://img.shields.io/crates/v/understory_guide.svg)](https://crates.io/crates/understory_guide)
[![Documentation build status.](https://img.shields.io/docsrs/understory_guide.svg)](https://docs.rs/understory_guide)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/forest-rs/understory/ci.yml?logo=github&label=CI)](https://github.com/forest-rs/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_guide --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Guide: headless 2D guide geometry primitives.

This crate owns small 2D geometric adapters above lower-level numeric axis
and selection primitives. It is intended for things like floating rulers,
measurement guides, and timeline headers attached to arbitrary baselines.

It owns:
- line-guide pose and projection math
- semantic hit targets for guide body and endpoint handles
- lifting [`understory_axis::AxisRuler1D`] marks into 2D geometry

It does not own:
- rendering
- text shaping
- event routing
- domain navigation policy

This crate is `no_std` and uses `alloc`.

<!-- cargo-rdme end -->

[`understory_axis::AxisRuler1D`]: https://docs.rs/understory_axis/latest/understory_axis/struct.AxisRuler1D.html

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
