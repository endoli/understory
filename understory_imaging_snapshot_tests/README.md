<div align="center">

# Understory Imaging Snapshot Tests

**Kompari-based snapshots for Understory imaging backends**

[![Latest published version.](https://img.shields.io/crates/v/understory_imaging_snapshot_tests.svg)](https://crates.io/crates/understory_imaging_snapshot_tests)
[![Documentation build status.](https://img.shields.io/docsrs/understory_imaging_snapshot_tests.svg)](https://docs.rs/understory_imaging_snapshot_tests)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_imaging_snapshot_tests
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Development-only snapshot tests for the Understory imaging stack.

This crate contains Kompari-based snapshot tests for `understory_imaging` backends.

## Backends

- `vello_cpu` (default for `cargo xtask`)
- `skia`
- `vello` (GPU)

## Run tests

- Vello CPU: `cargo test -p understory_imaging_snapshot_tests --features vello_cpu --test vello_cpu_snapshots`
- Skia: `cargo test -p understory_imaging_snapshot_tests --features skia --test skia_snapshots`
- Vello GPU: `cargo test -p understory_imaging_snapshot_tests --features vello --test vello_snapshots`

Equivalent `xtask` wrapper:
- Default backend (`vello_cpu`): `cargo xtask snapshots test`
- Select backend:
  - `cargo xtask snapshots --backend skia test`
  - `cargo xtask snapshots --backend vello test`

## Bless / regenerate

Bless current output as the expected snapshots:
- `cargo xtask snapshots test --accept`
- `cargo xtask snapshots --backend skia test --accept`
- `cargo xtask snapshots --backend vello test --accept`

Generate `tests/current/<backend>/*.png` for review:
- `cargo xtask snapshots test --generate-all`
- `cargo xtask snapshots --backend skia test --generate-all`
- `cargo xtask snapshots --backend vello test --generate-all`

`vello` snapshots will be skipped if no compatible wgpu device is available.

## Filter cases

To run only a subset of snapshot cases, set `UNDERSTORY_IMAGING_CASE` (supports `*` globs).
The `xtask` wrapper exposes this via `--case`:

- Single case: `cargo xtask snapshots test --case stroke_styles`
- Prefix: `cargo xtask snapshots test --case 'clip_*'`
- Multiple patterns (comma/whitespace-separated): `cargo xtask snapshots test --case 'clip_*,fill_rule_*'`

## Review diffs with `xtask`

`xtask` runs the snapshot tests and produces a Kompari HTML report:
- Default backend (`vello_cpu`): `cargo xtask report`
- Select backend explicitly:
  - `cargo xtask report --backend skia`
  - `cargo xtask report --backend vello`
- Alternatively:
  - `cargo xtask snapshots --backend skia report`
  - `cargo xtask snapshots --backend vello review`

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
