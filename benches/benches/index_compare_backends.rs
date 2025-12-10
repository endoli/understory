// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use understory_index::{Aabb2D, Backend, Index, IndexGeneric};

fn gen_grid_rects(n: usize, cell: f64) -> Vec<Aabb2D<f64>> {
    let mut out = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let x0 = x as f64 * cell;
            let y0 = y as f64 * cell;
            out.push(Aabb2D::<f64>::from_xywh(x0, y0, cell, cell));
        }
    }
    out
}

fn gen_grid_rects_f32(n: usize, cell: f32) -> Vec<Aabb2D<f32>> {
    let mut out = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let x0 = x as f32 * cell;
            let y0 = y as f32 * cell;
            out.push(Aabb2D::<f32>::from_xywh(x0, y0, cell, cell));
        }
    }
    out
}

fn gen_grid_rects_i64(n: usize, cell: i64) -> Vec<Aabb2D<i64>> {
    let mut out = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let x0 = x as i64 * cell;
            let y0 = y as i64 * cell;
            out.push(Aabb2D::<i64>::from_xywh(x0, y0, cell, cell));
        }
    }
    out
}

fn gen_overlap_grid_rects(n: usize, cell: f64, scale: f64) -> Vec<Aabb2D<f64>> {
    let mut out = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let x0 = x as f64 * cell;
            let y0 = y as f64 * cell;
            out.push(Aabb2D::<f64>::from_xywh(x0, y0, cell * scale, cell * scale));
        }
    }
    out
}

#[derive(Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn next_f64(&mut self) -> f64 {
        let v = self.next_u64() >> 11;
        (v as f64) / ((1u64 << 53) as f64)
    }
}

fn gen_clustered_rects(n_clusters: usize, per_cluster: usize, spread: f64) -> Vec<Aabb2D<f64>> {
    let mut out = Vec::with_capacity(n_clusters * per_cluster);
    let mut rng = Rng::new(0xC1A5_7E55_9999_ABCD);
    let mut centers = Vec::with_capacity(n_clusters);
    for _ in 0..n_clusters {
        centers.push((rng.next_f64() * 2000.0, rng.next_f64() * 2000.0));
    }
    for (cx, cy) in centers {
        for _ in 0..per_cluster {
            let dx = (rng.next_f64() - 0.5) * spread;
            let dy = (rng.next_f64() - 0.5) * spread;
            out.push(Aabb2D::<f64>::from_xywh(cx + dx, cy + dy, 12.0, 12.0));
        }
    }
    out
}

fn gen_random_rects_f32(
    count: usize,
    max_w: f32,
    max_h: f32,
    rect_w: f32,
    rect_h: f32,
) -> Vec<Aabb2D<f32>> {
    let mut out = Vec::with_capacity(count);
    let mut rng = Rng::new(0xFACE_FEED_CAFE_BABE);
    for _ in 0..count {
        let x0 = (rng.next_f64() as f32) * (max_w - rect_w).max(1.0);
        let y0 = (rng.next_f64() as f32) * (max_h - rect_h).max(1.0);
        out.push(Aabb2D::<f32>::from_xywh(x0, y0, rect_w, rect_h));
    }
    out
}

trait IndexOpsF64U32 {
    fn insert_box(&mut self, aabb: Aabb2D<f64>, payload: u32);
    fn commit_box(&mut self);
    fn query_rect_count_box(&self, rect: Aabb2D<f64>) -> usize;
}

impl<B> IndexOpsF64U32 for IndexGeneric<f64, u32, B>
where
    B: Backend<f64>,
{
    fn insert_box(&mut self, aabb: Aabb2D<f64>, payload: u32) {
        let _ = self.insert(aabb, payload);
    }

    fn commit_box(&mut self) {
        let _ = self.commit();
    }

    fn query_rect_count_box(&self, rect: Aabb2D<f64>) -> usize {
        self.query_rect(rect).count()
    }
}

fn bench_insert_commit_rect_grid_f64_with_backend<F, I>(
    c: &mut Criterion,
    group_name: &str,
    make_index: F,
) where
    F: Fn() -> I + Clone + 'static,
    I: IndexOpsF64U32 + 'static,
{
    let mut group = c.benchmark_group(group_name);
    for &n in &[32usize, 64, 128] {
        let rects = gen_grid_rects(n, 10.0);
        group.throughput(Throughput::Elements((n * n) as u64));
        group.bench_function(format!("insert_commit_rect_n{}", n), |b| {
            let make_index = make_index.clone();
            b.iter_batched(
                || {
                    let mut idx = make_index();
                    for (i, r) in rects.iter().copied().enumerate() {
                        idx.insert_box(r, i as u32);
                    }
                    idx.commit_box();
                    idx
                },
                |idx| {
                    let hits: usize = idx
                        .query_rect_count_box(Aabb2D::<f64>::from_xywh(100.0, 100.0, 400.0, 400.0));
                    black_box(hits);
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_query_heavy_f64_with_backend<F, I>(c: &mut Criterion, group_name: &str, make_index: F)
where
    F: Fn() -> I + Clone + 'static,
    I: IndexOpsF64U32 + 'static,
{
    let rects = gen_grid_rects(128, 8.0);
    let mut group = c.benchmark_group(group_name);
    group.bench_function("build_then_many_queries", |b| {
        let make_index = make_index.clone();
        b.iter_batched(
            || {
                let mut idx = make_index();
                for (i, r) in rects.iter().copied().enumerate() {
                    idx.insert_box(r, i as u32);
                }
                idx.commit_box();
                idx
            },
            |idx| {
                let mut total = 0usize;
                for q in 0..256 {
                    let x = (q % 64) as f64 * 8.0;
                    let y = (q / 64) as f64 * 8.0;
                    total += idx.query_rect_count_box(Aabb2D::<f64>::from_xywh(x, y, 64.0, 64.0));
                }
                black_box(total);
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_flatvec(c: &mut Criterion) {
    let mut group = c.benchmark_group("flatvec");
    for &n in &[32usize, 64, 128] {
        let rects = gen_grid_rects(n, 10.0);
        group.throughput(Throughput::Elements((n * n) as u64));
        group.bench_function(format!("insert_commit_rect_n{}", n), |b| {
            b.iter_batched(
                Index::<f64, u32>::new,
                |mut idx| {
                    for (i, r) in rects.iter().copied().enumerate() {
                        let _ = idx.insert(r, i as u32);
                    }
                    let _ = idx.commit();
                    let hits: usize = idx
                        .query_rect(Aabb2D::<f64>::from_xywh(100.0, 100.0, 400.0, 400.0))
                        .count();
                    black_box(hits);
                },
                BatchSize::SmallInput,
            )
        });
    }
    let rects = gen_overlap_grid_rects(64, 10.0, 3.0);
    group.bench_function("insert_commit_rect_overlap", |b| {
        b.iter_batched(
            Index::<f64, u32>::new,
            |mut idx| {
                for (i, r) in rects.iter().copied().enumerate() {
                    let _ = idx.insert(r, i as u32);
                }
                let _ = idx.commit();
                let hits: usize = idx
                    .query_rect(Aabb2D::<f64>::from_xywh(100.0, 100.0, 400.0, 400.0))
                    .count();
                black_box(hits);
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_bvh(c: &mut Criterion) {
    bench_insert_commit_rect_grid_f64_with_backend(c, "bvh_f64", Index::<f64, u32>::with_bvh);
}

fn bench_rtree(c: &mut Criterion) {
    let mut group = c.benchmark_group("rtree_i64");
    for &n in &[32usize, 64, 128] {
        let rects = gen_grid_rects_i64(n, 10);
        group.throughput(Throughput::Elements((n * n) as u64));
        group.bench_function(format!("insert_commit_rect_n{}", n), |b| {
            b.iter_batched(
                Index::<i64, u32>::with_rtree,
                |mut idx| {
                    for (i, r) in rects.iter().copied().enumerate() {
                        let _ = idx.insert(r, i as u32);
                    }
                    let _ = idx.commit();
                    let hits: usize = idx.query_rect(Aabb2D::new(100, 100, 500, 500)).count();
                    black_box(hits);
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_bvh_f32(c: &mut Criterion) {
    let mut group = c.benchmark_group("bvh_f32");
    for &n in &[32usize, 64, 128] {
        let rects = gen_grid_rects_f32(n, 10.0);
        group.throughput(Throughput::Elements((n * n) as u64));
        group.bench_function(format!("insert_commit_rect_n{}", n), |b| {
            b.iter_batched(
                Index::<f32, u32>::with_bvh,
                |mut idx| {
                    for (i, r) in rects.iter().copied().enumerate() {
                        let _ = idx.insert(r, i as u32);
                    }
                    let _ = idx.commit();
                    let hits: usize = idx
                        .query_rect(Aabb2D::<f32>::from_xywh(100.0, 100.0, 400.0, 400.0))
                        .count();
                    black_box(hits);
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_rtree_f64(c: &mut Criterion) {
    // Note: this uses the same query rect as the other f64 grids for consistency.
    bench_insert_commit_rect_grid_f64_with_backend(c, "rtree_f64", || {
        Index::<f64, u32>::with_rtree()
    });
}

fn bench_rtree_f32(c: &mut Criterion) {
    let mut group = c.benchmark_group("rtree_f32");
    let rects = gen_random_rects_f32(4096, 2000.0, 2000.0, 12.0, 12.0);
    group.bench_function("insert_commit_rect_random", |b| {
        b.iter_batched(
            Index::<f32, u32>::with_rtree,
            |mut idx| {
                for (i, r) in rects.iter().copied().enumerate() {
                    let _ = idx.insert(r, i as u32);
                }
                let _ = idx.commit();
                let hits: usize = idx
                    .query_rect(Aabb2D::<f32>::from_xywh(800.0, 800.0, 400.0, 400.0))
                    .count();
                black_box(hits);
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_update_heavy_rtree_i64(c: &mut Criterion) {
    let mut group = c.benchmark_group("rtree_i64_update_heavy");
    let rects = gen_grid_rects_i64(64, 10);
    group.bench_function("update_move_then_commit", |b| {
        b.iter_batched(
            || {
                let mut idx = Index::<i64, u32>::with_rtree();
                let mut keys = Vec::new();
                for (i, r) in rects.iter().copied().enumerate() {
                    keys.push(idx.insert(r, i as u32));
                }
                let _ = idx.commit();
                (idx, keys)
            },
            |(mut idx, keys)| {
                for (j, k) in keys.into_iter().enumerate() {
                    let dx = (j as i64 % 5) - 2;
                    let dy = ((j * 7) as i64 % 5) - 2;
                    // shift by small delta
                    // read current aabb indirectly by reusing known pattern
                    idx.update(
                        k,
                        Aabb2D::<i64>::from_xywh(10 * (j as i64) + dx, dy, 10, 10),
                    );
                }
                let _ = idx.commit();
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_query_heavy_rtree_f64(c: &mut Criterion) {
    bench_query_heavy_f64_with_backend(c, "rtree_f64_query_heavy", || {
        Index::<f64, u32>::with_rtree()
    });
}

fn bench_query_heavy_flatvec_and_grid_f64(c: &mut Criterion) {
    // Flatvec: build once, then many queries.
    bench_query_heavy_f64_with_backend(c, "flatvec_query_heavy_f64", Index::<f64, u32>::new);

    // Grid: same workload, grid-backed index.
    bench_query_heavy_f64_with_backend(c, "grid_f64_query_heavy", || {
        Index::<f64, u32>::with_grid(8.0)
    });
}

fn bench_bvh_clustered_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("bvh_f64_clustered");
    let rects = gen_clustered_rects(16, 256, 128.0);
    group.bench_function("insert_commit_query", |b| {
        b.iter_batched(
            Index::<f64, u32>::with_bvh,
            |mut idx| {
                for (i, r) in rects.iter().copied().enumerate() {
                    let _ = idx.insert(r, i as u32);
                }
                let _ = idx.commit();
                let hits = idx
                    .query_rect(Aabb2D::<f64>::from_xywh(800.0, 800.0, 400.0, 400.0))
                    .count();
                black_box(hits);
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_grid_f64(c: &mut Criterion) {
    bench_insert_commit_rect_grid_f64_with_backend(c, "grid_f64", || {
        Index::<f64, u32>::with_grid(10.0)
    });
    let mut group = c.benchmark_group("grid_f64");
    let rects = gen_overlap_grid_rects(64, 10.0, 3.0);
    group.bench_function("insert_commit_rect_overlap", |b| {
        b.iter_batched(
            || Index::<f64, u32>::with_grid(10.0),
            |mut idx| {
                for (i, r) in rects.iter().copied().enumerate() {
                    let _ = idx.insert(r, i as u32);
                }
                let _ = idx.commit();
                let hits: usize = idx
                    .query_rect(Aabb2D::<f64>::from_xywh(100.0, 100.0, 400.0, 400.0))
                    .count();
                black_box(hits);
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_flatvec,
    bench_bvh,
    bench_bvh_f32,
    bench_grid_f64,
    bench_rtree,
    bench_rtree_f64,
    bench_rtree_f32,
    bench_update_heavy_rtree_i64,
    bench_query_heavy_rtree_f64,
    bench_bvh_clustered_f64,
    bench_query_heavy_flatvec_and_grid_f64,
);
criterion_main!(benches);
