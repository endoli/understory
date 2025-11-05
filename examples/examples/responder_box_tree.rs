// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Box tree → responder adapter with a simple ASCII tree.
//!
//! This example shows how a box tree can feed the responder by resolving
//! hits, building a dispatch sequence, and deriving hover transitions.
//!
//! Run:
//! - `cargo run -p understory_examples --example responder_box_tree`

use std::collections::HashMap;

use kurbo::{Affine, Point, Rect};
use understory_box_tree::{LocalNode, NodeFlags, NodeId, QueryFilter, Tree};
use understory_responder::adapters::box_tree::{hits_for_rect, top_hit_for_point};
use understory_responder::hover::{HoverState, path_from_dispatch};
use understory_responder::router::Router;
use understory_responder::types::{ResolvedHit, WidgetLookup};

fn main() {
    // Build a small scene with two overlapping siblings.
    let mut bt = Tree::new();

    let root_local = LocalNode {
        local_bounds: Rect::new(0.0, 0.0, 400.0, 400.0),
        local_transform: Affine::IDENTITY,
        local_clip: None,
        z_index: 0,
        flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
    };
    let root = bt.insert(None, root_local);

    // Track a simple adjacency for printing (NodeId → children), and attach
    // human-friendly labels + geometry for clarity when printing.
    let mut edges: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut info: HashMap<NodeId, (String, Rect, i32)> = HashMap::new();
    info.insert(root, ("root".into(), Rect::new(0.0, 0.0, 400.0, 400.0), 0));

    // Child A: behind (z=0), at (50,50)-(150,150)
    let child_a = bt.insert(
        Some(root),
        LocalNode {
            local_bounds: Rect::new(50.0, 50.0, 150.0, 150.0),
            z_index: 0,
            ..Default::default()
        },
    );
    edges.entry(root).or_default().push(child_a);
    info.insert(
        child_a,
        ("A".into(), Rect::new(50.0, 50.0, 150.0, 150.0), 0),
    );

    // Child B: on top (z=5), overlapping A: (100,100)-(200,200)
    let child_b = bt.insert(
        Some(root),
        LocalNode {
            local_bounds: Rect::new(100.0, 100.0, 200.0, 200.0),
            z_index: 5,
            ..Default::default()
        },
    );
    edges.entry(root).or_default().push(child_b);
    info.insert(
        child_b,
        ("B".into(), Rect::new(100.0, 100.0, 200.0, 200.0), 5),
    );

    // Commit to compute world transforms/AABBs and update the spatial index.
    let _dmg = bt.commit();

    // Draw a tiny ASCII tree of what we built, with labels and rectangles.
    print_ascii_tree(root, &edges, &info);

    // Minimal lookup mapping nodes to "widget ids" (echo NodeId for demo).
    struct Lookup;
    impl WidgetLookup<NodeId> for Lookup {
        type WidgetId = NodeId;
        fn widget_of(&self, node: &NodeId) -> Option<Self::WidgetId> {
            Some(*node)
        }
    }

    let router: Router<NodeId, Lookup> = Router::new(Lookup);

    // Point inside the overlap region; top_hit should be child_b (higher z).
    let pt = Point::new(120.0, 120.0);
    let filter = QueryFilter {
        visible_only: true,
        pickable_only: true,
    };
    let hit: ResolvedHit<NodeId, ()> = top_hit_for_point(&bt, pt, filter).expect("expected a hit");
    println!("\nQuery point #1: ({:.1}, {:.1})", pt.x, pt.y);
    let dispatch = router.handle_with_hits(&[hit]);
    println!("\n== Dispatch (overlap @ 120,120) ==");
    for d in &dispatch {
        println!("  {:?}  node={:?}  widget={:?}", d.phase, d.node, d.widget);
    }

    // Derive a hover path and compute transitions using HoverState.
    let mut hover = HoverState::new();
    let path = path_from_dispatch(&dispatch);
    let first = hover.update_path(&path);
    println!("\n== Hover transitions (first) ==\n  {:?}", first);

    // Move point into only-child A region (e.g., 60,60) and re-route.
    let pt2 = Point::new(60.0, 60.0);
    let hit2 = top_hit_for_point(&bt, pt2, filter).expect("expected hit in A");
    println!("\nQuery point #2: ({:.1}, {:.1})", pt2.x, pt2.y);
    let dispatch2 = router.handle_with_hits(&[hit2]);
    println!("\n== Dispatch (point #2 @ {:.1},{:.1}) ==", pt2.x, pt2.y);
    for d in &dispatch2 {
        println!("  {:?}  node={:?}  widget={:?}", d.phase, d.node, d.widget);
    }
    let path2 = path_from_dispatch(&dispatch2);
    let second = hover.update_path(&path2);
    println!("\n== Hover transitions (second) ==\n  {:?}", second);

    // Visible set example: query a viewport.
    let viewport = Rect::new(0.0, 0.0, 300.0, 300.0);
    let visible_hits = hits_for_rect(
        &bt,
        viewport,
        QueryFilter {
            visible_only: true,
            pickable_only: false,
        },
    );
    println!(
        "\n== Visible nodes in viewport ==\n  viewport: ({:.1},{:.1})–({:.1},{:.1})\n  count: {}",
        viewport.x0,
        viewport.y0,
        viewport.x1,
        viewport.y1,
        visible_hits.len()
    );
}

fn print_ascii_tree(
    root: NodeId,
    edges: &HashMap<NodeId, Vec<NodeId>>,
    info: &HashMap<NodeId, (String, Rect, i32)>,
) {
    println!("Scene:");
    // Print root, then recurse into children.
    print_node("", root, info);
    fn go(
        node: NodeId,
        edges: &HashMap<NodeId, Vec<NodeId>>,
        info: &HashMap<NodeId, (String, Rect, i32)>,
        prefix: &str,
    ) {
        if let Some(kids) = edges.get(&node) {
            let len = kids.len();
            for (i, &k) in kids.iter().enumerate() {
                let last = i + 1 == len;
                let branch = if last { "└── " } else { "├── " };
                print_node(&format!("{}{}", prefix, branch), k, info);
                let next_prefix = if last {
                    format!("{}    ", prefix)
                } else {
                    format!("{}│   ", prefix)
                };
                go(k, edges, info, &next_prefix);
            }
        }
    }
    go(root, edges, info, "");
}

fn print_node(prefix: &str, id: NodeId, info: &HashMap<NodeId, (String, Rect, i32)>) {
    if let Some((name, rect, z)) = info.get(&id) {
        println!(
            "{}{} {:?}  rect=({:.0},{:.0})–({:.0},{:.0})  z={}",
            prefix, name, id, rect.x0, rect.y0, rect.x1, rect.y1, z
        );
    } else {
        println!("{}{:?}", prefix, id);
    }
}
