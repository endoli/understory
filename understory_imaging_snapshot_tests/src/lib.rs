// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_imaging_snapshot_tests --heading-base-level=0

//! Development-only snapshot tests for the Understory imaging stack.
//!
//! This crate contains Kompari-based snapshot tests for `understory_imaging` backends.
//!
//! ## Backends
//!
//! - `vello_cpu` (default for `cargo xtask`)
//! - `skia`
//! - `vello` (GPU)
//!
//! ## Run tests
//!
//! - Vello CPU: `cargo test -p understory_imaging_snapshot_tests --features vello_cpu --test vello_cpu_snapshots`
//! - Skia: `cargo test -p understory_imaging_snapshot_tests --features skia --test skia_snapshots`
//! - Vello GPU: `cargo test -p understory_imaging_snapshot_tests --features vello --test vello_snapshots`
//!
//! Equivalent `xtask` wrapper:
//! - Default backend (`vello_cpu`): `cargo xtask snapshots test`
//! - Select backend:
//!   - `cargo xtask snapshots --backend skia test`
//!   - `cargo xtask snapshots --backend vello test`
//!
//! ## Bless / regenerate
//!
//! Bless current output as the expected snapshots:
//! - `cargo xtask snapshots test --accept`
//! - `cargo xtask snapshots --backend skia test --accept`
//! - `cargo xtask snapshots --backend vello test --accept`
//!
//! Generate `tests/current/<backend>/*.png` for review:
//! - `cargo xtask snapshots test --generate-all`
//! - `cargo xtask snapshots --backend skia test --generate-all`
//! - `cargo xtask snapshots --backend vello test --generate-all`
//!
//! `vello` snapshots will be skipped if no compatible wgpu device is available.
//!
//! ## Filter cases
//!
//! To run only a subset of snapshot cases, set `UNDERSTORY_IMAGING_CASE` (supports `*` globs).
//! The `xtask` wrapper exposes this via `--case`:
//!
//! - Single case: `cargo xtask snapshots test --case stroke_styles`
//! - Prefix: `cargo xtask snapshots test --case 'clip_*'`
//! - Multiple patterns (comma/whitespace-separated): `cargo xtask snapshots test --case 'clip_*,fill_rule_*'`
//!
//! ## Review diffs with `xtask`
//!
//! `xtask` runs the snapshot tests and produces a Kompari HTML report:
//! - Default backend (`vello_cpu`): `cargo xtask report`
//! - Select backend explicitly:
//!   - `cargo xtask report --backend skia`
//!   - `cargo xtask report --backend vello`
//! - Alternatively:
//!   - `cargo xtask snapshots --backend skia report`
//!   - `cargo xtask snapshots --backend vello review`

#![allow(
    missing_docs,
    reason = "development-only crate; snapshot cases are self-documenting via test names"
)]

pub mod cases;
