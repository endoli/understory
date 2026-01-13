// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Benchmarks for Understory box tree query/update behavior.
//!
//! By default this uses a deterministic synthetic tree intended to approximate a UI widget
//! gallery's shape and geometry. To benchmark a real scene, set `UI_BOX_TREE_JSON` to the path
//! of a JSON dump matching the expected schema.

use core::time::Duration;
use criterion::measurement::WallTime;
use criterion::{
    BatchSize, BenchmarkGroup, BenchmarkId, Criterion, black_box, criterion_group, criterion_main,
};
use kurbo::{Affine, Point, Rect, RoundedRect, RoundedRectRadii, Vec2};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use understory_box_tree::{LocalNode, NodeId, QueryFilter, Tree};
use understory_index::Backend;

const ENV_JSON_PATH: &str = "UI_BOX_TREE_JSON";

#[derive(Clone, Copy, Debug, Default)]
struct BuildStats {
    nodes: usize,
    clips: usize,
    nonidentity_transforms: usize,
    max_depth: u16,
    max_children: usize,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct DumpRect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl DumpRect {
    fn to_rect(self) -> Rect {
        Rect::new(self.x0, self.y0, self.x1, self.y1)
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct DumpRadii {
    top_left: f64,
    top_right: f64,
    bottom_right: f64,
    bottom_left: f64,
}

impl DumpRadii {
    fn to_kurbo(self) -> RoundedRectRadii {
        RoundedRectRadii::new(
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct DumpRoundedRect {
    rect: DumpRect,
    radii: DumpRadii,
}

impl DumpRoundedRect {
    fn to_kurbo(self) -> RoundedRect {
        let r = self.rect;
        RoundedRect::new(r.x0, r.y0, r.x1, r.y1, self.radii.to_kurbo())
    }
}

#[derive(Clone, Debug, Deserialize)]
struct DumpNode {
    local_bounds: DumpRect,
    local_clip: Option<DumpRoundedRect>,
    local_transform: [f64; 6],
    children: Vec<DumpNode>,
    #[allow(dead_code)]
    view_id: Option<String>,
}

fn build_tree_from_json<B: Backend<f64>>(
    backend: B,
    path: &Path,
) -> (Tree<B>, Vec<NodeId>, BuildStats) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"));
    let dump: DumpNode =
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("invalid JSON {path:?}: {e}"));
    build_tree_from_dump(backend, &dump)
}

fn build_tree_from_dump<B: Backend<f64>>(
    backend: B,
    dump: &DumpNode,
) -> (Tree<B>, Vec<NodeId>, BuildStats) {
    let mut tree = Tree::with_backend(backend);
    let mut ids = Vec::new();
    let mut stats = BuildStats::default();

    build_subtree_from_dump(&mut tree, &mut ids, &mut stats, None, dump, 1);
    let _ = tree.commit();
    (tree, ids, stats)
}

fn build_subtree_from_dump<B: Backend<f64>>(
    tree: &mut Tree<B>,
    ids: &mut Vec<NodeId>,
    stats: &mut BuildStats,
    parent: Option<NodeId>,
    node: &DumpNode,
    depth: u16,
) {
    stats.max_depth = stats.max_depth.max(depth);
    stats.max_children = stats.max_children.max(node.children.len());
    if node.local_clip.is_some() {
        stats.clips += 1;
    }
    if node.local_transform != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] {
        stats.nonidentity_transforms += 1;
    }

    let id = tree.insert(
        parent,
        LocalNode {
            local_bounds: node.local_bounds.to_rect(),
            local_transform: Affine::new(node.local_transform),
            local_clip: node.local_clip.map(DumpRoundedRect::to_kurbo),
            ..LocalNode::default()
        },
    );
    stats.nodes += 1;
    ids.push(id);

    let child_depth = depth.saturating_add(1);
    for child in &node.children {
        build_subtree_from_dump(tree, ids, stats, Some(id), child, child_depth);
    }
}

/// Synthetic tree intended to match a UI-ish gallery.
///
/// Rough target shape (from a real-world dump):
/// - ~523 nodes
/// - max depth ~10
/// - max children ~100
/// - ~8 clips
/// - ~134 non-identity transforms
fn build_synthetic_ui_box_tree<B: Backend<f64>>(backend: B) -> (Tree<B>, Vec<NodeId>, BuildStats) {
    let mut tree = Tree::with_backend(backend);
    let mut ids = Vec::new();
    let mut stats = BuildStats::default();
    let mut depth_by_id = HashMap::<NodeId, u16>::new();

    let root = insert(
        &mut tree,
        &mut ids,
        &mut stats,
        &mut depth_by_id,
        None,
        Rect::new(0.0, 0.0, 1200.0, 800.0),
        Affine::IDENTITY,
        Some(RoundedRect::from_rect(
            Rect::new(0.0, 0.0, 1200.0, 800.0),
            0.0,
        )),
    );

    let viewport = insert(
        &mut tree,
        &mut ids,
        &mut stats,
        &mut depth_by_id,
        Some(root),
        Rect::new(0.0, 0.0, 1200.0, 800.0),
        Affine::IDENTITY,
        Some(RoundedRect::from_rect(
            Rect::new(0.0, 0.0, 1200.0, 800.0),
            0.0,
        )),
    );

    let _sidebar = insert(
        &mut tree,
        &mut ids,
        &mut stats,
        &mut depth_by_id,
        Some(viewport),
        Rect::new(0.0, 0.0, 200.0, 800.0),
        Affine::IDENTITY,
        Some(RoundedRect::from_rect(
            Rect::new(0.0, 0.0, 200.0, 800.0),
            0.0,
        )),
    );
    let content = insert(
        &mut tree,
        &mut ids,
        &mut stats,
        &mut depth_by_id,
        Some(viewport),
        Rect::new(0.0, 0.0, 1000.0, 800.0),
        Affine::translate(Vec2::new(200.0, 0.0)),
        None,
    );

    let _header = insert(
        &mut tree,
        &mut ids,
        &mut stats,
        &mut depth_by_id,
        Some(content),
        Rect::new(0.0, 0.0, 1000.0, 60.0),
        Affine::IDENTITY,
        None,
    );
    let scroll_container = insert(
        &mut tree,
        &mut ids,
        &mut stats,
        &mut depth_by_id,
        Some(content),
        Rect::new(0.0, 0.0, 1000.0, 740.0),
        Affine::translate(Vec2::new(0.0, 60.0)),
        Some(RoundedRect::from_rect(
            Rect::new(0.0, 0.0, 1000.0, 740.0),
            6.0,
        )),
    );

    let grid = insert(
        &mut tree,
        &mut ids,
        &mut stats,
        &mut depth_by_id,
        Some(scroll_container),
        Rect::new(0.0, 0.0, 1000.0, 740.0),
        Affine::IDENTITY,
        None,
    );
    stats.max_children = stats.max_children.max(100);

    let cols = 10;
    let rows = 10;
    let cell_w = 96.0;
    let cell_h = 64.0;
    let gap_x = 4.0;
    let gap_y = 4.0;
    let pad_x = 8.0;
    let pad_y = 8.0;

    for row in 0..rows {
        for col in 0..cols {
            let idx = row * cols + col;
            let x = pad_x + (cell_w + gap_x) * (col as f64);
            let y = pad_y + (cell_h + gap_y) * (row as f64);

            // 100 non-identity transforms (cell placement) + a few rotations.
            let mut tf = Affine::translate(Vec2::new(x, y));
            if idx % 23 == 0 {
                tf *= Affine::rotate(0.05);
            }

            let cell = insert(
                &mut tree,
                &mut ids,
                &mut stats,
                &mut depth_by_id,
                Some(grid),
                Rect::new(0.0, 0.0, cell_w, cell_h),
                tf,
                None,
            );

            let _bg = insert(
                &mut tree,
                &mut ids,
                &mut stats,
                &mut depth_by_id,
                Some(cell),
                Rect::new(0.0, 0.0, cell_w, cell_h),
                Affine::IDENTITY,
                None,
            );
            let _border = insert(
                &mut tree,
                &mut ids,
                &mut stats,
                &mut depth_by_id,
                Some(cell),
                Rect::new(0.0, 0.0, cell_w, cell_h),
                Affine::IDENTITY,
                None,
            );
            let _icon = insert(
                &mut tree,
                &mut ids,
                &mut stats,
                &mut depth_by_id,
                Some(cell),
                Rect::new(8.0, 8.0, 40.0, 40.0),
                Affine::IDENTITY,
                None,
            );

            // 32 non-identity transforms: label position for 32/100 cells.
            let label_tf = if idx % 3 == 0 && idx != 0 && idx != 3 {
                Affine::translate(Vec2::new(8.0, cell_h - 24.0))
            } else {
                Affine::IDENTITY
            };
            let label_bounds = if label_tf == Affine::IDENTITY {
                Rect::new(8.0, cell_h - 24.0, cell_w - 8.0, cell_h - 8.0)
            } else {
                Rect::new(0.0, 0.0, cell_w - 16.0, 16.0)
            };
            let _label = insert(
                &mut tree,
                &mut ids,
                &mut stats,
                &mut depth_by_id,
                Some(cell),
                label_bounds,
                label_tf,
                None,
            );

            // 4 deeper subtrees with clips to hit ~10 max depth and ~8 clips total.
            if idx % 25 == 0 {
                let w1 = insert(
                    &mut tree,
                    &mut ids,
                    &mut stats,
                    &mut depth_by_id,
                    Some(cell),
                    Rect::new(0.0, 0.0, cell_w, cell_h),
                    Affine::IDENTITY,
                    Some(RoundedRect::from_rect(
                        Rect::new(0.0, 0.0, cell_w, cell_h),
                        4.0,
                    )),
                );
                let w2 = insert(
                    &mut tree,
                    &mut ids,
                    &mut stats,
                    &mut depth_by_id,
                    Some(w1),
                    Rect::new(2.0, 2.0, cell_w - 2.0, cell_h - 2.0),
                    Affine::IDENTITY,
                    None,
                );
                let w3 = insert(
                    &mut tree,
                    &mut ids,
                    &mut stats,
                    &mut depth_by_id,
                    Some(w2),
                    Rect::new(4.0, 4.0, cell_w - 4.0, cell_h - 4.0),
                    Affine::IDENTITY,
                    None,
                );
                let _deep = insert(
                    &mut tree,
                    &mut ids,
                    &mut stats,
                    &mut depth_by_id,
                    Some(w3),
                    Rect::new(6.0, 6.0, cell_w - 6.0, cell_h - 6.0),
                    Affine::IDENTITY,
                    None,
                );
            }
        }
    }

    let _ = tree.commit();
    stats.nodes = ids.len();
    (tree, ids, stats)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Local helper to keep the synthetic tree construction readable."
)]
fn insert<B: Backend<f64>>(
    tree: &mut Tree<B>,
    ids: &mut Vec<NodeId>,
    stats: &mut BuildStats,
    depth_by_id: &mut HashMap<NodeId, u16>,
    parent: Option<NodeId>,
    local_bounds: Rect,
    local_transform: Affine,
    local_clip: Option<RoundedRect>,
) -> NodeId {
    if local_clip.is_some() {
        stats.clips += 1;
    }
    if local_transform != Affine::IDENTITY {
        stats.nonidentity_transforms += 1;
    }

    let id = tree.insert(
        parent,
        LocalNode {
            local_bounds,
            local_transform,
            local_clip,
            ..LocalNode::default()
        },
    );
    stats.nodes += 1;
    ids.push(id);

    let depth = parent
        .and_then(|p| depth_by_id.get(&p).copied())
        .unwrap_or(0)
        + 1;
    depth_by_id.insert(id, depth);
    stats.max_depth = stats.max_depth.max(depth);

    id
}

fn points() -> Vec<Point> {
    let mut out = Vec::new();
    for iy in 0..=8 {
        for ix in 0..=12 {
            out.push(Point::new(ix as f64 * 100.0, iy as f64 * 100.0));
        }
    }
    out.extend([
        Point::new(0.0, 0.0),
        Point::new(1199.0, 0.0),
        Point::new(0.0, 799.0),
        Point::new(1199.0, 799.0),
        Point::new(600.0, 400.0),
    ]);
    out
}

fn build_ui_box_tree<B: Backend<f64>>(backend: B) -> (Tree<B>, Vec<NodeId>, BuildStats) {
    if let Ok(path) = std::env::var(ENV_JSON_PATH) {
        return build_tree_from_json(backend, Path::new(&path));
    }
    build_synthetic_ui_box_tree(backend)
}

fn load_dump_from_env() -> Option<DumpNode> {
    let path = std::env::var(ENV_JSON_PATH).ok()?;
    let bytes =
        fs::read(Path::new(&path)).unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"));
    let dump: DumpNode =
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("invalid JSON {path:?}: {e}"));
    Some(dump)
}

fn bench_hit_test<B: Backend<f64>>(g: &mut BenchmarkGroup<'_, WallTime>, name: &str, backend: B) {
    let (tree, _ids, _stats) = build_ui_box_tree(backend);
    let pts = points();
    let filter = QueryFilter::new().pickable();

    g.bench_with_input(
        BenchmarkId::new("hit_test_point", name),
        &tree,
        |b, tree| {
            b.iter(|| {
                for &p in &pts {
                    black_box(tree.hit_test_point(black_box(p), filter));
                }
            });
        },
    );
}

fn bench_commit_noop<B: Backend<f64>>(
    g: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    backend: B,
) {
    let (mut tree, _ids, _stats) = build_ui_box_tree(backend);
    g.bench_with_input(BenchmarkId::new("commit_noop", name), &name, |b, _| {
        b.iter(|| black_box(tree.commit()));
    });
}

fn bench_commit_one_transform<B: Backend<f64>>(
    g: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    backend: B,
) {
    let (mut tree, ids, _stats) = build_ui_box_tree(backend);
    let id = ids[ids.len() / 2];
    let t0 = Affine::translate(Vec2::new(0.0, 0.0));
    let t1 = Affine::translate(Vec2::new(0.01, 0.0));
    let mut toggle = false;

    g.bench_with_input(
        BenchmarkId::new("commit_one_transform", name),
        &name,
        |b, _| {
            b.iter(|| {
                toggle = !toggle;
                tree.set_local_transform(id, if toggle { t0 } else { t1 });
                black_box(tree.commit())
            });
        },
    );
}

fn bench_commit_one_bounds<B: Backend<f64>>(
    g: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    backend: B,
) {
    let (mut tree, ids, _stats) = build_ui_box_tree(backend);
    let id = ids[ids.len() / 2];
    let b0 = Rect::new(0.0, 0.0, 1.0, 1.0);
    let b1 = Rect::new(0.0, 0.0, 1.01, 1.0);
    let mut toggle = false;

    g.bench_with_input(
        BenchmarkId::new("commit_one_bounds", name),
        &name,
        |b, _| {
            b.iter(|| {
                toggle = !toggle;
                tree.set_local_bounds(id, if toggle { b0 } else { b1 });
                black_box(tree.commit())
            });
        },
    );
}

fn bench_build_and_commit<B, F>(
    g: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    dump: Option<&DumpNode>,
    make_backend: F,
) where
    B: Backend<f64>,
    F: Fn() -> B + Copy,
{
    g.bench_with_input(BenchmarkId::new("build_and_commit", name), &name, |b, _| {
        b.iter_batched(
            make_backend,
            |backend| {
                if let Some(dump) = dump {
                    black_box(build_tree_from_dump(backend, dump));
                } else {
                    black_box(build_synthetic_ui_box_tree(backend));
                }
            },
            BatchSize::SmallInput,
        );
    });
}

fn ui_box_tree(c: &mut Criterion) {
    let dump = load_dump_from_env();

    {
        // Keep these short: they rebuild a sizable tree once per benchmark.
        let mut g = c.benchmark_group("ui_box_tree");
        g.warm_up_time(Duration::from_secs(1));
        g.measurement_time(Duration::from_secs(3));

        // Hit testing.
        bench_hit_test(
            &mut g,
            "flatvec",
            understory_index::backends::FlatVec::<f64>::default(),
        );
        bench_hit_test(
            &mut g,
            "grid_f64_100",
            understory_index::backends::GridF64::new(100.0),
        );
        bench_hit_test(
            &mut g,
            "rtree_f64",
            understory_index::backends::RTreeF64::<()>::default(),
        );
        bench_hit_test(
            &mut g,
            "bvh_f64",
            understory_index::backends::BvhF64::default(),
        );

        // Commit.
        bench_commit_noop(
            &mut g,
            "flatvec",
            understory_index::backends::FlatVec::<f64>::default(),
        );
        bench_commit_one_transform(
            &mut g,
            "flatvec",
            understory_index::backends::FlatVec::<f64>::default(),
        );
        bench_commit_one_bounds(
            &mut g,
            "flatvec",
            understory_index::backends::FlatVec::<f64>::default(),
        );
        bench_commit_noop(
            &mut g,
            "grid_f64_100",
            understory_index::backends::GridF64::new(100.0),
        );
        bench_commit_one_transform(
            &mut g,
            "grid_f64_100",
            understory_index::backends::GridF64::new(100.0),
        );
        bench_commit_one_bounds(
            &mut g,
            "grid_f64_100",
            understory_index::backends::GridF64::new(100.0),
        );
        bench_commit_noop(
            &mut g,
            "rtree_f64",
            understory_index::backends::RTreeF64::<()>::default(),
        );
        bench_commit_one_transform(
            &mut g,
            "rtree_f64",
            understory_index::backends::RTreeF64::<()>::default(),
        );
        bench_commit_one_bounds(
            &mut g,
            "rtree_f64",
            understory_index::backends::RTreeF64::<()>::default(),
        );
        bench_commit_noop(
            &mut g,
            "bvh_f64",
            understory_index::backends::BvhF64::default(),
        );
        bench_commit_one_transform(
            &mut g,
            "bvh_f64",
            understory_index::backends::BvhF64::default(),
        );
        bench_commit_one_bounds(
            &mut g,
            "bvh_f64",
            understory_index::backends::BvhF64::default(),
        );

        g.finish();
    }

    // Full rebuild (tree construction + commit). This is the cost you pay on initial build or
    // when doing a full rebuild from scratch.
    {
        let mut g_build = c.benchmark_group("ui_box_tree_build");
        g_build.warm_up_time(Duration::from_secs(1));
        g_build.measurement_time(Duration::from_secs(3));

        bench_build_and_commit::<understory_index::backends::FlatVec<f64>, _>(
            &mut g_build,
            "flatvec",
            dump.as_ref(),
            understory_index::backends::FlatVec::<f64>::default,
        );
        bench_build_and_commit::<understory_index::backends::GridF64, _>(
            &mut g_build,
            "grid_f64_100",
            dump.as_ref(),
            || understory_index::backends::GridF64::new(100.0),
        );
        bench_build_and_commit::<understory_index::backends::RTreeF64<()>, _>(
            &mut g_build,
            "rtree_f64",
            dump.as_ref(),
            understory_index::backends::RTreeF64::<()>::default,
        );
        bench_build_and_commit::<understory_index::backends::BvhF64, _>(
            &mut g_build,
            "bvh_f64",
            dump.as_ref(),
            understory_index::backends::BvhF64::default,
        );

        g_build.finish();
    }
}

criterion_group!(benches, ui_box_tree);
criterion_main!(benches);
