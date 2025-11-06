// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Visible-window example using box tree rectangle intersection.
//!
//! Run:
//! - `cargo run -p understory_examples --example box_tree_visible_list`

use kurbo::Rect;
use understory_box_tree::{LocalNode, QueryFilter, Tree};

const ROW_H: f64 = 20.0;
const WIDTH: f64 = 200.0;

fn main() {
    let mut tree = Tree::new();
    let root = tree.insert(
        None,
        LocalNode {
            local_bounds: Rect::new(0.0, 0.0, WIDTH, 100000.0),
            ..Default::default()
        },
    );

    let rows = 1000_usize;
    let mut ids = Vec::with_capacity(rows);
    for i in 0..rows {
        let y0 = i as f64 * ROW_H;
        let node = tree.insert(
            Some(root),
            LocalNode {
                local_bounds: Rect::new(0.0, y0, WIDTH, y0 + ROW_H),
                z_index: 0,
                ..Default::default()
            },
        );
        ids.push(node);
    }
    let _ = tree.commit();

    let filter = QueryFilter {
        visible_only: true,
        pickable_only: false,
    };

    // Simulate a few scroll positions by changing the viewport rectangle
    for scroll in [0.0, 30.0, 200.0, 600.0] {
        let viewport = Rect::new(0.0, scroll, WIDTH, scroll + 100.0);
        let visible: Vec<_> = tree.intersect_rect(viewport, filter).collect();
        let indices: Vec<_> = visible
            .into_iter()
            .filter_map(|id| ids.iter().position(|x| *x == id))
            .collect();
        println!("scroll={scroll:.1} -> visible indices: {:?}", indices);
    }
}
