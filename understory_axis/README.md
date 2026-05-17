<div align="center">

# Understory Axis

**Headless numeric axis scale and tick primitives for Understory**

[![Latest published version.](https://img.shields.io/crates/v/understory_axis.svg)](https://crates.io/crates/understory_axis)
[![Documentation build status.](https://img.shields.io/docsrs/understory_axis.svg)](https://docs.rs/understory_axis)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/forest-rs/understory/ci.yml?logo=github&label=CI)](https://github.com/forest-rs/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_axis --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Axis: headless axis mapping and tick primitives.

This crate focuses on one narrow concern: mapping numeric domains onto a 1D
view span and deriving stable, "nice" tick positions for that mapping.

It owns:
- linear and logarithmic 1D axis mappings
- major / medium / minor tick selection
- label eligibility decisions based on spacing thresholds
- spacing metadata for callers that need consistent axis-derived policy
- configurable major-step ladders and subdivision policies for different axis domains
- scalar ruler snapshots that can be placed along arbitrary 2D baselines

It does not own:
- domain-specific label formatting
- time units or dates
- viewport transforms
- rendering or text layout

The intended split is:
- a caller supplies a headless axis mapping plus tick policy
- this crate returns tick positions plus their semantic kind
- an adapter above this crate decides how to place those scalar marks in 2D
- the caller formats tick labels appropriate to its own domain

## Minimal example

```rust
use understory_axis::{
    AxisMajorStepLadder, AxisMapping1D, AxisScale1D, AxisScaleOptions,
    AxisSubdivisionPolicy, AxisTickKind,
};

let mapping = AxisMapping1D::linear(0.0..200.0, 0.0..100.0);
let scale = AxisScale1D::from_mapping(
    &mapping,
    AxisScaleOptions {
        target_major_spacing_px: 100.0,
        min_major_step: 0.0,
        medium_label_min_spacing_px: 220.0,
        major_step_ladder: AxisMajorStepLadder::Decimal125,
        subdivision_policy: AxisSubdivisionPolicy::Auto,
    },
);

let ticks: std::vec::Vec<_> = scale.iter_ticks_in_range(0.0..100.0).collect();
assert!(ticks.iter().any(|tick| tick.kind == AxisTickKind::Major && tick.labeled));
```

This crate is `no_std` and uses `alloc`.

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

[LICENSE-APACHE]: https://github.com/forest-rs/understory/blob/main/LICENSE-APACHE
[LICENSE-MIT]: https://github.com/forest-rs/understory/blob/main/LICENSE-MIT
[Rust code of conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: https://github.com/forest-rs/understory/blob/main/AUTHORS
