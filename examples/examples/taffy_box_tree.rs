// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Taffy ↔ box tree integration basics.
//!
//! This example shows how to:
//! - Build a simple Taffy layout tree.
//! - Attach nodes to an `understory_box_tree::Tree` via `TaffyBoxTree`.
//! - Run layout in Taffy and synchronize results into the box tree.
//! - Query world bounds and hit-test using the box tree.
//!
//! Run:
//! - `cargo run -p understory_examples --example taffy_box_tree`

use kurbo::Point;
use taffy::prelude::{AvailableSpace, Dimension, Size, Style, TaffyTree};
use understory_box_tree::{LocalNode, NodeFlags, QueryFilter};
use understory_taffy::{TaffyBoxTree, layout_to_rect};

fn main() {
    // Build a simple Taffy tree with a single root node that has an explicit size.
    let mut taffy: TaffyTree<()> = TaffyTree::new();
    let mut style = Style::DEFAULT;
    style.size.width = Dimension::length(100.0);
    style.size.height = Dimension::length(50.0);
    let root = taffy.new_leaf(style).unwrap();

    // Create the adapter and attach the root to the box tree.
    let mut adapter: TaffyBoxTree = TaffyBoxTree::new();
    let root_box = adapter.attach_node(
        root,
        None,
        LocalNode {
            // Mark the node visible and pickable so hit testing will see it.
            flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
            ..LocalNode::default()
        },
    );

    // Run layout in Taffy.
    taffy
        .compute_layout(
            root,
            Size {
                width: AvailableSpace::Definite(100.0),
                height: AvailableSpace::Definite(50.0),
            },
        )
        .unwrap();

    // Sync layout into the box tree and commit once.
    let damage = adapter.sync_layout_and_commit(&taffy).unwrap();
    println!("damage rects: {:?}", damage.dirty_rects);

    // Query world bounds for the box-tree node corresponding to the Taffy root.
    let tree = &adapter.tree;
    let bounds = tree.world_bounds(root_box).unwrap();
    println!("root world bounds: {:?}", bounds);

    // The box-tree bounds should match the layout reported by Taffy.
    let layout = taffy.layout(root).unwrap();
    let expected = layout_to_rect(layout);
    assert_eq!(bounds, expected);

    // Hit-test inside the root's bounds; the attached node should be hit.
    let filter = QueryFilter::new().visible().pickable();
    let hit = tree
        .hit_test_point(Point::new(10.0, 10.0), filter)
        .expect("expected hit inside root");
    println!("hit node: {:?}", hit.node);
    assert_eq!(hit.node, root_box);
}
