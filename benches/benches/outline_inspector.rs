// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use understory_inspector::{Inspector, InspectorConfig, InspectorModel};
use understory_outline::{ExpansionState, Outline, OutlineModel};

#[derive(Clone, Debug)]
struct Node {
    parent: Option<usize>,
    first_child: Option<usize>,
    next_sibling: Option<usize>,
    depth: usize,
}

#[derive(Clone, Debug)]
struct SyntheticModel {
    nodes: Vec<Node>,
    first_root: Option<usize>,
}

impl OutlineModel for SyntheticModel {
    type Key = usize;
    type Item = ();

    fn first_root_key(&self) -> Option<Self::Key> {
        self.first_root
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        *key < self.nodes.len()
    }

    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.nodes[*key].next_sibling
    }

    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.nodes[*key].first_child
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        self.contains_key(key).then_some(())
    }
}

impl InspectorModel for SyntheticModel {
    fn parent_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.nodes[*key].parent
    }
}

fn wide_roots_model(n: usize) -> SyntheticModel {
    let mut nodes = Vec::with_capacity(n);
    for index in 0..n {
        nodes.push(Node {
            parent: None,
            first_child: None,
            next_sibling: (index + 1 < n).then_some(index + 1),
            depth: 0,
        });
    }
    SyntheticModel {
        nodes,
        first_root: (n > 0).then_some(0),
    }
}

fn deep_chain_model(n: usize) -> SyntheticModel {
    let mut nodes = Vec::with_capacity(n);
    for index in 0..n {
        nodes.push(Node {
            parent: index.checked_sub(1),
            first_child: (index + 1 < n).then_some(index + 1),
            next_sibling: None,
            depth: index,
        });
    }
    SyntheticModel {
        nodes,
        first_root: (n > 0).then_some(0),
    }
}

fn expanded_keys(n: usize) -> impl Iterator<Item = usize> {
    0..n.saturating_sub(1)
}

fn complete_k_ary_tree_model(total_nodes: usize, branching: usize) -> SyntheticModel {
    assert!(branching > 0, "branching must be positive");
    let mut nodes: Vec<Node> = Vec::with_capacity(total_nodes);
    for index in 0..total_nodes {
        let parent = (index > 0).then_some((index - 1) / branching);
        let first_child = {
            let child = index.saturating_mul(branching).saturating_add(1);
            (child < total_nodes).then_some(child)
        };
        let next_sibling = parent.and_then(|parent_index| {
            let sibling_pos = (index - 1) % branching;
            let candidate = index + 1;
            (sibling_pos + 1 < branching
                && candidate < total_nodes
                && (candidate - 1) / branching == parent_index)
                .then_some(candidate)
        });
        let depth = parent.map_or(0, |parent_index| nodes[parent_index].depth + 1);
        nodes.push(Node {
            parent,
            first_child,
            next_sibling,
            depth,
        });
    }

    SyntheticModel {
        nodes,
        first_root: (total_nodes > 0).then_some(0),
    }
}

fn expanded_keys_through_depth<'a>(
    model: &'a SyntheticModel,
    max_expanded_depth: usize,
) -> impl Iterator<Item = usize> + 'a {
    model
        .nodes
        .iter()
        .enumerate()
        .filter_map(move |(index, node)| {
            (node.depth < max_expanded_depth && node.first_child.is_some()).then_some(index)
        })
}

fn first_node_at_depth_with_children(model: &SyntheticModel, depth: usize) -> Option<usize> {
    model.nodes.iter().enumerate().find_map(|(index, node)| {
        (node.depth == depth && node.first_child.is_some()).then_some(index)
    })
}

fn partially_expanded_balanced_inspector(
    model: &SyntheticModel,
    max_expanded_depth: usize,
) -> Inspector<SyntheticModel> {
    let mut inspector = Inspector::new(model.clone(), InspectorConfig::fixed_rows(18.0, 540.0));
    for key in expanded_keys_through_depth(model, max_expanded_depth) {
        let _ = inspector.expand(key);
    }
    inspector
}

fn bench_outline_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("outline/rebuild");

    for &n in &[10_000_usize, 100_000] {
        let model = wide_roots_model(n);
        group.bench_with_input(BenchmarkId::new("wide_roots", n), &n, |b, _| {
            b.iter(|| {
                let mut outline = Outline::new(model.clone());
                let _ = outline.visible_rows();
            });
        });
    }

    for &n in &[1_000_usize, 10_000] {
        let model = deep_chain_model(n);
        let expansion = ExpansionState::new();
        let mut outline = Outline::from_parts(model, expansion);
        let _ = outline.replace_expanded(expanded_keys(n));
        group.bench_with_input(BenchmarkId::new("deep_chain_expanded", n), &n, |b, _| {
            b.iter(|| {
                outline.mark_dirty();
                let _ = outline.visible_rows();
            });
        });
    }

    let balanced_nodes = 97_656_usize;
    let balanced_model = complete_k_ary_tree_model(balanced_nodes, 5);
    let mut balanced_outline = Outline::new(balanced_model.clone());
    let _ = balanced_outline.replace_expanded(expanded_keys(balanced_nodes));
    group.bench_with_input(
        BenchmarkId::new("balanced_5ary_expanded", balanced_nodes),
        &balanced_nodes,
        |b, _| {
            b.iter(|| {
                balanced_outline.mark_dirty();
                let _ = balanced_outline.visible_rows();
            });
        },
    );

    let mut partially_expanded_outline = Outline::new(balanced_model.clone());
    let _ = partially_expanded_outline
        .replace_expanded(expanded_keys_through_depth(&balanced_model, 4));
    group.bench_with_input(
        BenchmarkId::new("balanced_5ary_expand_depth4", balanced_nodes),
        &balanced_nodes,
        |b, _| {
            b.iter(|| {
                partially_expanded_outline.mark_dirty();
                let _ = partially_expanded_outline.visible_rows();
            });
        },
    );

    group.finish();
}

fn bench_inspector_navigation(c: &mut Criterion) {
    let mut group = c.benchmark_group("inspector/navigation");

    for &n in &[10_000_usize, 100_000] {
        let model = wide_roots_model(n);
        let mut inspector = Inspector::new(model, InspectorConfig::fixed_rows(18.0, 540.0));
        let _ = inspector.focus_first();

        group.bench_with_input(BenchmarkId::new("focus_next_wide_roots", n), &n, |b, _| {
            b.iter(|| {
                let moved = black_box(inspector.focus_next());
                black_box(inspector.focus().copied());
                if !moved {
                    black_box(inspector.focus_first());
                    black_box(inspector.focus().copied());
                }
            });
        });
    }

    let balanced_nodes = 97_656_usize;
    let balanced_model = complete_k_ary_tree_model(balanced_nodes, 5);
    let mut inspector = Inspector::new(
        balanced_model.clone(),
        InspectorConfig::fixed_rows(18.0, 540.0),
    );
    let _ = inspector.expand(0);
    let _ = inspector.focus_first();
    group.bench_with_input(
        BenchmarkId::new("focus_next_balanced_5ary_expand_root", balanced_nodes),
        &balanced_nodes,
        |b, _| {
            b.iter(|| {
                let moved = black_box(inspector.focus_next());
                black_box(inspector.focus().copied());
                if !moved {
                    black_box(inspector.focus_first());
                    black_box(inspector.focus().copied());
                }
            });
        },
    );

    group.finish();
}

fn bench_inspector_mutation_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("inspector/mutation_cycle");

    let balanced_nodes = 97_656_usize;
    let balanced_model = complete_k_ary_tree_model(balanced_nodes, 5);
    let target = first_node_at_depth_with_children(&balanced_model, 3)
        .expect("balanced tree should have an expandable node at depth 3");

    group.bench_with_input(
        BenchmarkId::new("expand_range_collapse_balanced_5ary_depth3", balanced_nodes),
        &balanced_nodes,
        |b, _| {
            b.iter_batched(
                || {
                    let mut inspector = partially_expanded_balanced_inspector(&balanced_model, 3);
                    let focused = inspector.set_focus(Some(target));
                    assert!(
                        focused,
                        "target should be visible in the initial projection"
                    );
                    let selected = inspector.select_only_focused();
                    assert!(selected, "target selection should initialize");
                    inspector
                },
                |mut inspector| {
                    black_box(inspector.expand(target));
                    for _ in 0..8 {
                        black_box(inspector.extend_selection_next());
                    }
                    black_box(inspector.visible_len());
                    black_box(inspector.realized_range());
                    black_box(inspector.collapse(target));
                    black_box(inspector.focus().copied());
                    black_box(inspector.realized_range());
                },
                BatchSize::SmallInput,
            );
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_outline_rebuild,
    bench_inspector_navigation,
    bench_inspector_mutation_cycle
);
criterion_main!(benches);
