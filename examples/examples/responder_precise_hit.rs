// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Broad + narrow phase hit testing: box tree + `understory_precise_hit`.
//!
//! This example shows how to combine:
//! - `understory_box_tree` for broad-phase AABB culling and z-order,
//! - `understory_precise_hit` for precise local hit testing on geometry,
//! - `understory_responder` for routing via `ResolvedHit`.
//!
//! Run:
//! - `cargo run -p understory_examples --example responder_precise_hit`

use std::collections::HashMap;

use kurbo::{Affine, Circle, Point, Rect, Vec2};
use understory_box_tree::{LocalNode, NodeFlags, NodeId, QueryFilter, Tree};
use understory_precise_hit::{HitParams, HitScore, PreciseHitTest};
use understory_responder::adapters::hit2d::{KeyHit, resolved_hits_from_precise};
use understory_responder::dispatcher;
use understory_responder::router::Router;
use understory_responder::types::{Outcome, ResolvedHit, WidgetLookup};

/// Simple scene data: each node has either a rect or a circle for precise hits.
#[derive(Clone, Copy, Debug)]
enum Shape {
    Rect(Rect),
    Circle(Circle),
}

/// Implement precise hit testing by delegating to the underlying geometry.
impl PreciseHitTest for Shape {
    fn hit_test_local(&self, pt: Point, params: &HitParams) -> Option<HitScore> {
        match self {
            Shape::Rect(r) => r.hit_test_local(pt, params),
            Shape::Circle(c) => c.hit_test_local(pt, params),
        }
    }
}

fn main() {
    // Build a small box tree with two children having different shapes.
    let mut tree = Tree::new();

    let root = tree.insert(
        None,
        LocalNode {
            local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
            flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
            ..Default::default()
        },
    );

    // Node A: axis-aligned rect.
    let rect = Rect::new(20.0, 40.0, 120.0, 140.0);
    let node_a = tree.insert(
        Some(root),
        LocalNode {
            local_bounds: rect,
            z_index: 0,
            ..Default::default()
        },
    );

    // Node B: circle translated to the right, same AABB size for illustration.
    let circle = Circle::new((0.0, 0.0), 40.0);
    let node_b = tree.insert(
        Some(root),
        LocalNode {
            local_bounds: Rect::new(140.0, 40.0, 220.0, 140.0),
            local_transform: Affine::translate(Vec2::new(180.0, 90.0)),
            z_index: 5,
            ..Default::default()
        },
    );

    // Map NodeId → precise shape in local coordinates.
    let mut shapes: HashMap<NodeId, Shape> = HashMap::new();
    shapes.insert(node_a, Shape::Rect(rect));
    shapes.insert(node_b, Shape::Circle(circle));

    let _damage = tree.commit();

    // Minimal lookup: echo NodeId as WidgetId.
    struct Lookup;
    impl WidgetLookup<NodeId> for Lookup {
        type WidgetId = NodeId;
        fn widget_of(&self, node: &NodeId) -> Option<Self::WidgetId> {
            Some(*node)
        }
    }

    let router: Router<NodeId, Lookup> = Router::new(Lookup);

    // Query a few points and show how coarse box hits are refined by geometry.
    let params = HitParams {
        fill_tolerance: 2.0,
        ..HitParams::default()
    };
    for (label, pt) in [
        ("rect A interior", Point::new(30.0, 80.0)),
        ("circle B interior", Point::new(200.0, 90.0)),
        (
            "coarse hit only (inside B AABB, outside circle)",
            Point::new(150.0, 50.0),
        ),
    ] {
        println!("\n== Query: {} @ ({:.1}, {:.1}) ==", label, pt.x, pt.y);

        // Broad phase: candidate NodeIds whose AABB contains the world-space point.
        let filter = QueryFilter::new().visible().pickable();
        let broad_hits: Vec<NodeId> = tree
            .intersect_rect(Rect::from_points(pt, pt), filter)
            .collect();

        println!("Broad-phase candidates: {:?}", broad_hits);

        // Narrow phase: transform point into each node's local space and run precise hit.
        let mut precise_candidates = Vec::new();
        for id in &broad_hits {
            if let Some(shape) = shapes.get(id)
                && let Some(world_to_local) = tree.world_transform(*id).map(|tf| tf.inverse())
            {
                let local_pt = world_to_local * pt;
                precise_candidates.push((*id, *shape, local_pt));
            }
        }

        let mut key_hits: Vec<KeyHit<NodeId>> = Vec::new();
        for (id, shape, local_pt) in precise_candidates {
            if let Some(score) = shape.hit_test_local(local_pt, &params) {
                key_hits.push(understory_responder::adapters::hit2d::KeyHit {
                    key: id,
                    distance: score.distance,
                });
            }
        }

        println!("Narrow-phase hits: {:?}", key_hits);

        // Wrap into ResolvedHit and feed to the router.
        let resolved: Vec<ResolvedHit<NodeId, ()>> = resolved_hits_from_precise(&key_hits);
        let dispatch = router.handle_with_hits(&resolved);

        let _ = dispatcher::run(&dispatch, &mut (), |d, _| {
            println!("  {:?}  node={:?} widget={:?}", d.phase, d.node, d.widget);
            Outcome::Continue
        });
    }
}
