// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use std::time::Duration;
use understory_selection::Selection;

fn bench_replace_with_vs_unique(c: &mut Criterion) {
    let mut group = c.benchmark_group("selection/replace_with");

    // Hypothesis: `replace_with` is O(n^2) due to de-dup scanning, while
    // `replace_with_unique` / `replace_with_hashed` are O(n) for select-all style inputs.
    for len in [128usize, 512, 2_048, 8_192] {
        let keys: Vec<u32> = (0..(len as u32)).collect();
        group.throughput(Throughput::Elements(len as u64));

        group.bench_with_input(BenchmarkId::new("replace_with", len), &keys, |b, keys| {
            b.iter_batched(
                Selection::<u32>::new,
                |mut sel| {
                    sel.replace_with(keys.iter().copied());
                    black_box(sel);
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_with_input(
            BenchmarkId::new("replace_with_unique", len),
            &keys,
            |b, keys| {
                b.iter_batched(
                    Selection::<u32>::new,
                    |mut sel| {
                        sel.replace_with_unique(keys.iter().copied());
                        black_box(sel);
                    },
                    BatchSize::LargeInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("replace_with_hashed", len),
            &keys,
            |b, keys| {
                b.iter_batched(
                    Selection::<u32>::new,
                    |mut sel| {
                        sel.replace_with_hashed(keys.iter().copied());
                        black_box(sel);
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_replace_with_large_unique(c: &mut Criterion) {
    let mut group = c.benchmark_group("selection/replace_with_large");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));

    // Only benchmark the O(n) variants at large sizes; `replace_with` would take too long.
    for len in [131_072usize, 524_288] {
        let keys: Vec<u32> = (0..(len as u32)).collect();
        group.throughput(Throughput::Elements(len as u64));

        group.bench_with_input(
            BenchmarkId::new("replace_with_unique", len),
            &keys,
            |b, keys| {
                b.iter_batched(
                    Selection::<u32>::new,
                    |mut sel| {
                        sel.replace_with_unique(keys.iter().copied());
                        black_box(sel);
                    },
                    BatchSize::LargeInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("replace_with_hashed", len),
            &keys,
            |b, keys| {
                b.iter_batched(
                    Selection::<u32>::new,
                    |mut sel| {
                        sel.replace_with_hashed(keys.iter().copied());
                        black_box(sel);
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_replace_with_duplicates(c: &mut Criterion) {
    let mut group = c.benchmark_group("selection/replace_with_duplicates");

    // Many duplicates: models building a selection from repeated sources.
    for unique_len in [128usize, 512, 2_048, 8_192] {
        let keys: Vec<u32> = (0..(unique_len as u32))
            .flat_map(|k| core::iter::repeat_n(k, 4))
            .collect();
        group.throughput(Throughput::Elements(keys.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("replace_with", unique_len),
            &keys,
            |b, keys| {
                b.iter_batched(
                    Selection::<u32>::new,
                    |mut sel| {
                        sel.replace_with(keys.iter().copied());
                        black_box(sel);
                    },
                    BatchSize::LargeInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("replace_with_hashed", unique_len),
            &keys,
            |b, keys| {
                b.iter_batched(
                    Selection::<u32>::new,
                    |mut sel| {
                        sel.replace_with_hashed(keys.iter().copied());
                        black_box(sel);
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_replace_with_vs_unique,
    bench_replace_with_large_unique,
    bench_replace_with_duplicates
);
criterion_main!(benches);
