// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared helpers for snapshot integration tests.

#![allow(
    missing_docs,
    reason = "Integration-test helper module; not part of the public API."
)]

use std::fs;
use std::path::{Path, PathBuf};

use kompari::{
    Image, ImageDifference, SizeOptimizationLevel, compare_images, image_to_png, load_image,
};
use understory_imaging_snapshot_tests::cases::{SnapshotCase, selected_cases_for_backend};

#[allow(
    dead_code,
    reason = "Not all snapshot backends can use the shared runner (e.g. Vello GPU tolerances/device detection)."
)]
pub(crate) fn run_cases(
    backend: &str,
    render: impl FnMut(&dyn SnapshotCase) -> Image,
    errors: &mut Vec<String>,
) {
    run_cases_with(backend, render, |_case| 0, errors);
}

#[allow(
    dead_code,
    reason = "Not all snapshot backends can use the shared runner (e.g. Vello GPU tolerances/device detection)."
)]
pub(crate) fn run_cases_with(
    backend: &str,
    mut render: impl FnMut(&dyn SnapshotCase) -> Image,
    mut max_allowed_different_pixels: impl FnMut(&dyn SnapshotCase) -> u64,
    errors: &mut Vec<String>,
) {
    for case in selected_cases_for_backend(backend) {
        let image = render(case);
        check_snapshot_with_tolerance(
            backend,
            case.name(),
            &image,
            max_allowed_different_pixels(case),
            errors,
        );
    }
}

pub(crate) fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

pub(crate) fn snapshots_dir(backend: &str) -> PathBuf {
    tests_dir().join("snapshots").join(backend)
}

pub(crate) fn current_dir(backend: &str) -> PathBuf {
    tests_dir().join("current").join(backend)
}

pub(crate) fn accept_enabled() -> bool {
    std::env::var("UNDERSTORY_IMAGING_TEST")
        .ok()
        .is_some_and(|v| v.eq_ignore_ascii_case("accept"))
}

pub(crate) fn generate_all_enabled() -> bool {
    std::env::var("UNDERSTORY_IMAGING_TEST")
        .ok()
        .is_some_and(|v| v.eq_ignore_ascii_case("generate-all"))
}

pub(crate) fn assert_no_snapshot_errors(errors: Vec<String>) {
    if errors.is_empty() {
        return;
    }

    eprintln!(
        "Snapshot failures (use `UNDERSTORY_IMAGING_TEST=accept` to bless; `cargo xtask report` to view diffs):"
    );
    for error in &errors {
        eprintln!("  - {error}");
    }
    panic!("snapshot failures: {}", errors.len());
}

#[allow(
    dead_code,
    reason = "Not all snapshot backends use the same check logic (e.g. Vello GPU tolerances)."
)]
pub(crate) fn check_snapshot(backend: &str, name: &str, image: &Image, errors: &mut Vec<String>) {
    check_snapshot_with_tolerance(backend, name, image, 0, errors);
}

#[allow(
    dead_code,
    reason = "Not all snapshot backends use the same check logic (e.g. Vello GPU tolerances)."
)]
pub(crate) fn check_snapshot_with_tolerance(
    backend: &str,
    name: &str,
    image: &Image,
    max_allowed_different_pixels: u64,
    errors: &mut Vec<String>,
) {
    let expected_dir = snapshots_dir(backend);
    let expected_path = expected_dir.join(format!("{name}.png"));
    let current_dir = current_dir(backend);
    let current_path = current_dir.join(format!("{name}.png"));

    fs::create_dir_all(&expected_dir).expect("create snapshots dir");
    fs::create_dir_all(&current_dir).expect("create current dir");

    let accept = accept_enabled();
    let generate_all = generate_all_enabled();

    if !expected_path.exists() {
        if accept {
            fs::write(
                &expected_path,
                image_to_png(image, SizeOptimizationLevel::High),
            )
            .expect("write new snapshot");
            return;
        }
        fs::write(
            &current_path,
            image_to_png(image, SizeOptimizationLevel::Fast),
        )
        .expect("write current image");
        errors.push(format!(
            "missing snapshot `{}` (generated current `{}`); run with `UNDERSTORY_IMAGING_TEST=accept` to bless",
            expected_path.display(),
            current_path.display()
        ));
        return;
    }

    let expected = load_image(&expected_path).expect("load expected snapshot png");
    match compare_images(&expected, image) {
        ImageDifference::None => {
            if generate_all {
                fs::write(
                    &current_path,
                    image_to_png(image, SizeOptimizationLevel::Fast),
                )
                .expect("write current image");
            }
        }
        ImageDifference::Content {
            n_different_pixels, ..
        } if max_allowed_different_pixels > 0
            && n_different_pixels <= max_allowed_different_pixels =>
        {
            if generate_all {
                fs::write(
                    &current_path,
                    image_to_png(image, SizeOptimizationLevel::Fast),
                )
                .expect("write current image");
            }
        }
        diff => {
            if accept {
                fs::write(
                    &expected_path,
                    image_to_png(image, SizeOptimizationLevel::High),
                )
                .expect("update snapshot");
            } else {
                fs::write(
                    &current_path,
                    image_to_png(image, SizeOptimizationLevel::Fast),
                )
                .expect("write current image");
                errors.push(format!(
                    "snapshot mismatch for `{name}` (wrote `{}`), diff: {diff:?}",
                    current_path.display()
                ));
            }
        }
    };
}
