// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Box tree basics.
//!
//! Build a small tree, move a node, commit damage, and hit-test.
//!
//! Run:
//! - `cargo run -p understory_examples --example box_tree_basics`

use kurbo::{Affine, Point, Rect, Vec2};
use understory_box_tree::{LocalNode, QueryFilter, Tree};

fn main() {
    // Build a small tree
    let mut tree = Tree::new();
    let root = tree.insert(
        None,
        LocalNode {
            local_bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
            ..Default::default()
        },
    );
    let a = tree.insert(
        Some(root),
        LocalNode {
            local_bounds: Rect::new(10.0, 10.0, 60.0, 60.0),
            z_index: 0,
            ..Default::default()
        },
    );
    let b = tree.insert(
        Some(root),
        LocalNode {
            local_bounds: Rect::new(40.0, 40.0, 120.0, 120.0),
            z_index: 10,
            ..Default::default()
        },
    );

    let _damage0 = tree.commit();

    // Move node A to the right and compute damage
    tree.set_local_transform(a, Affine::translate(Vec2::new(20.0, 0.0)));
    let damage = tree.commit();
    println!("damage rects: {:?}", damage.dirty_rects);

    // Hit-test prefers the higher z-index (node B)
    let filter = QueryFilter {
        visible_only: true,
        pickable_only: true,
    };
    let hit = tree.hit_test_point(Point::new(50.0, 50.0), filter).unwrap();
    println!("hit node: {:?}", hit.node);
    assert_eq!(hit.node, b, "hit-test should prefer higher z-index node B");
}
