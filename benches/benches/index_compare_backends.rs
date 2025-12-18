// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use understory_index::{Aabb2D, Backend, Index, IndexGeneric};

struct Size<T> {
    width: T,
    height: T,
}

fn gen_grid_rects_f64(n: usize, cell: f64) -> Vec<Aabb2D<f64>> {
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

fn gen_overlap_grid_rects_f64(n: usize, cell: f64, scale: f64) -> Vec<Aabb2D<f64>> {
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

/// Generates `count` random rectangles whose centers are all contained in `world`, and whose sizes
/// are uniformly between `min_rect_size` and `max_rect_size`.
///
/// Note: the generated rectangles can spill out of `world` if their center is close to the world
/// edge.
fn gen_random_rects_in_world_f64(
    count: usize,
    world: Aabb2D<f64>,
    min_rect_size: Size<f64>,
    max_rect_size: Size<f64>,
) -> Vec<Aabb2D<f64>> {
    let mut rng = Rng::new(0x3C6E_F35F_4750_2932);
    (0..count)
        .map(|_| {
            let center_x = rng.next_f64() * (world.max_x - world.min_x) + world.min_x;
            let center_y = rng.next_f64() * (world.max_y - world.min_y) + world.min_y;

            let width =
                rng.next_f64() * (max_rect_size.width - min_rect_size.width) + min_rect_size.width;
            let height = rng.next_f64() * (max_rect_size.height - min_rect_size.height)
                + min_rect_size.height;

            Aabb2D::from_xywh(
                center_x - 0.5 * width,
                center_y - 0.5 * height,
                width,
                height,
            )
        })
        .collect()
}

/// Generates `count` random points `(x, y)` that are all contained in `world`.
fn gen_random_points_in_world_f64(count: usize, world: Aabb2D<f64>) -> Vec<(f64, f64)> {
    let mut rng = Rng::new(0x81FD_BEE7_94F0_AF1A);
    (0..count)
        .map(|_| {
            let x = rng.next_f64() * (world.max_x - world.min_x) + world.min_x;
            let y = rng.next_f64() * (world.max_y - world.min_y) + world.min_y;
            (x, y)
        })
        .collect()
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

fn bench_insert_commit_rects_f64(
    c: &mut Criterion,
    benchmark_group_name: &str,
    make_rects: impl Fn(usize) -> Vec<Aabb2D<f64>>,
) {
    fn bench<F, B>(b: &mut criterion::Bencher, rects: &[Aabb2D<f64>], make_index: F)
    where
        F: Fn() -> IndexGeneric<f64, u32, B> + Clone + 'static,
        B: Backend<f64> + 'static,
    {
        b.iter_batched(
            make_index,
            |mut idx| {
                for (i, r) in rects.iter().copied().enumerate() {
                    idx.insert(r, i as u32);
                }
                idx.commit();
                idx
            },
            BatchSize::SmallInput,
        )
    }

    let mut group = c.benchmark_group(benchmark_group_name);
    for &n in &[32usize, 64, 128] {
        let rects = make_rects(n);
        group.throughput(Throughput::Elements(rects.len() as u64));
        group.bench_function(BenchmarkId::new("FlatVec", n), |b| {
            bench(b, &rects, Index::<f64, u32>::new)
        });
        group.bench_function(BenchmarkId::new("Bvh", n), |b| {
            bench(b, &rects, Index::<f64, u32>::with_bvh)
        });
        group.bench_function(BenchmarkId::new("RTree", n), |b| {
            bench(b, &rects, Index::<f64, u32>::with_rtree)
        });
        group.bench_function(BenchmarkId::new("Grid(10.)", n), |b| {
            bench(b, &rects, || Index::<f64, u32>::with_grid(10.0))
        });
    }
    group.finish();
}

fn bench_visit_point_f64(
    c: &mut Criterion,
    benchmark_group_name: &str,
    make_rects: impl Fn(usize) -> Vec<Aabb2D<f64>>,
    make_visit_points: impl Fn(usize) -> Vec<(f64, f64)>,
) {
    fn bench<F, B>(
        b: &mut criterion::Bencher,
        rects: &[Aabb2D<f64>],
        visit_points: &[(f64, f64)],
        make_index: F,
    ) where
        F: Fn() -> IndexGeneric<f64, u32, B> + Clone + 'static,
        B: Backend<f64> + 'static,
    {
        let mut idx = make_index();
        for (i, r) in rects.iter().copied().enumerate() {
            idx.insert(r, i as u32);
        }
        idx.commit();

        b.iter(|| {
            let mut total = 0usize;
            for &(x, y) in visit_points {
                idx.visit_point(
                    x,
                    y,
                    #[inline(always)]
                    |_, _| total += 1,
                );
            }
            total
        })
    }

    let mut group = c.benchmark_group(benchmark_group_name);
    for &n in &[32usize, 64, 128] {
        let rects = make_rects(n);
        let visit_points = make_visit_points(n);
        group.throughput(Throughput::Elements(rects.len() as u64));
        group.bench_function(BenchmarkId::new("FlatVec", n), |b| {
            bench(b, &rects, &visit_points, Index::<f64, u32>::new)
        });
        group.bench_function(BenchmarkId::new("Bvh", n), |b| {
            bench(b, &rects, &visit_points, Index::<f64, u32>::with_bvh)
        });
        group.bench_function(BenchmarkId::new("RTree", n), |b| {
            bench(b, &rects, &visit_points, Index::<f64, u32>::with_rtree)
        });
        group.bench_function(BenchmarkId::new("Grid(10.)", n), |b| {
            bench(b, &rects, &visit_points, || {
                Index::<f64, u32>::with_grid(10.0)
            })
        });
    }
    group.finish();
}

fn bench_visit_rect_f64(
    c: &mut Criterion,
    benchmark_group_name: &str,
    make_rects: impl Fn(usize) -> Vec<Aabb2D<f64>>,
    make_visit_rects: impl Fn(usize) -> Vec<Aabb2D<f64>>,
) {
    fn bench<F, B>(
        b: &mut criterion::Bencher,
        rects: &[Aabb2D<f64>],
        visit_rects: &[Aabb2D<f64>],
        make_index: F,
    ) where
        F: Fn() -> IndexGeneric<f64, u32, B> + Clone + 'static,
        B: Backend<f64> + 'static,
    {
        let mut idx = make_index();
        for (i, r) in rects.iter().copied().enumerate() {
            idx.insert(r, i as u32);
        }
        idx.commit();

        b.iter(|| {
            let mut total = 0usize;
            for visit_rect in visit_rects {
                idx.visit_rect(
                    *visit_rect,
                    #[inline(always)]
                    |_, _| total += 1,
                );
            }
            total
        })
    }

    let mut group = c.benchmark_group(benchmark_group_name);
    for &n in &[32usize, 64, 128] {
        let rects = make_rects(n);
        let visit_rects = make_visit_rects(n);
        group.throughput(Throughput::Elements(rects.len() as u64));
        group.bench_function(BenchmarkId::new("FlatVec", n), |b| {
            bench(b, &rects, &visit_rects, Index::<f64, u32>::new)
        });
        group.bench_function(BenchmarkId::new("Bvh", n), |b| {
            bench(b, &rects, &visit_rects, Index::<f64, u32>::with_bvh)
        });
        group.bench_function(BenchmarkId::new("RTree", n), |b| {
            bench(b, &rects, &visit_rects, Index::<f64, u32>::with_rtree)
        });
        group.bench_function(BenchmarkId::new("Grid(10.)", n), |b| {
            bench(b, &rects, &visit_rects, || {
                Index::<f64, u32>::with_grid(10.0)
            })
        });
    }
    group.finish();
}

fn bench_insert_commit_rect_grid_f64(c: &mut Criterion) {
    bench_insert_commit_rects_f64(c, "insert_commit_rect_grid_f64", |size| {
        gen_grid_rects_f64(size, 10.)
    });
}

fn bench_insert_commit_rect_overlap_f64(c: &mut Criterion) {
    bench_insert_commit_rects_f64(c, "insert_commit_rect_overlap_f64", |size| {
        gen_overlap_grid_rects_f64(size, 10.0, 3.0)
    });
}

fn bench_visit_point_grid_f64(c: &mut Criterion) {
    bench_visit_point_f64(
        c,
        "visit_point_grid_f64",
        // Rects that inserted into the index.
        |size| gen_grid_rects_f64(size, 10.),
        // Points that are visited
        |size| {
            // The grid generated above has a total size of `size` * 10. (with each rectangle in that
            // grid being of size 10).
            gen_random_points_in_world_f64(
                1000,
                Aabb2D::from_xywh(0., 0., size as f64 * 10., size as f64 * 10.),
            )
        },
    );
}

fn bench_visit_point_overlap_f64(c: &mut Criterion) {
    bench_visit_point_f64(
        c,
        "visit_point_overlap_f64",
        // Rects that inserted into the index.
        |size| gen_overlap_grid_rects_f64(size, 10., 3.),
        // Points that are visited
        |size| {
            // The grid generated above has a total size of `size-1` * 10 + 10 * 3 = size * 10 + 20
            // (with each rectangle in that grid being of size 10*3 = 30).
            gen_random_points_in_world_f64(
                1000,
                Aabb2D::from_xywh(0., 0., size as f64 * 10. + 20., size as f64 * 10. + 20.),
            )
        },
    );
}

fn bench_visit_rect_grid_f64(c: &mut Criterion) {
    bench_visit_rect_f64(
        c,
        "visit_rect_grid_f64",
        // Rects that inserted into the index.
        |size| gen_grid_rects_f64(size, 10.),
        // Rects whose overlap is queried
        |size| {
            // The grid generated above has a total size of `size` * 10. (with each rectangle in that
            // grid being of size 10).
            gen_random_rects_in_world_f64(
                1000,
                Aabb2D::from_xywh(0., 0., size as f64 * 10., size as f64 * 10.),
                // The generated query rectangles have a random size, overlapping 1 to 6 cells per
                // axis (1 to 36 cells in total, 18 on average).
                Size {
                    width: 1.,
                    height: 1.,
                },
                Size {
                    width: 50.,
                    height: 50.,
                },
            )
        },
    );
}

fn bench_visit_rect_overlap_f64(c: &mut Criterion) {
    bench_visit_rect_f64(
        c,
        "visit_rect_overlap_f64",
        // Rects that inserted into the index.
        |size| gen_overlap_grid_rects_f64(size, 10., 3.),
        // Rects whose overlap is queried
        |size| {
            // The grid generated above has a total size of `size-1` * 10 + 10 * 3 = size * 10 + 20
            // (with each rectangle in that grid being of size 10*3 = 30).
            gen_random_rects_in_world_f64(
                1000,
                Aabb2D::from_xywh(0., 0., size as f64 * 10. + 20., size as f64 * 10. + 20.),
                // The generated query rectangles have a random size, overlapping 1 to 6 cells per
                // axis (1 to 36 cells in total, 18 on average).
                Size {
                    width: 1.,
                    height: 1.,
                },
                Size {
                    width: 50.,
                    height: 50.,
                },
            )
        },
    );
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

criterion_group!(
    benches,
    bench_insert_commit_rect_grid_f64,
    bench_insert_commit_rect_overlap_f64,
    bench_visit_point_grid_f64,
    bench_visit_point_overlap_f64,
    bench_visit_rect_grid_f64,
    bench_visit_rect_overlap_f64,
    bench_bvh_f32,
    bench_rtree,
    bench_rtree_f32,
    bench_update_heavy_rtree_i64,
    bench_bvh_clustered_f64,
);
criterion_main!(benches);
