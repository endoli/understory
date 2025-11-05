// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Hover transitions from a dispatch sequence.
//!
//! This example derives enter/leave events from two successive routes by
//! computing the least common ancestor (LCA) between paths.
//!
//! Run:
//! - `cargo run -p understory_examples --example responder_hover`

use understory_responder::hover::{HoverEvent, HoverState, path_from_dispatch};
use understory_responder::router::Router;
use understory_responder::types::{DepthKey, Localizer, ParentLookup, ResolvedHit, WidgetLookup};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
struct Node(u32);

struct Lookup;
impl WidgetLookup<Node> for Lookup {
    type WidgetId = u32;
    fn widget_of(&self, n: &Node) -> Option<u32> {
        Some(n.0)
    }
}

struct Parents;
impl ParentLookup<Node> for Parents {
    fn parent_of(&self, node: &Node) -> Option<Node> {
        match node.0 {
            3 => Some(Node(2)),
            2 => Some(Node(1)),
            4 => Some(Node(1)),
            _ => None,
        }
    }
}

fn main() {
    let router: Router<Node, Lookup, Parents> = Router::with_parent(Lookup, Parents);

    // First hover at path 1→2→3
    let hits1 = vec![ResolvedHit {
        node: Node(3),
        path: None,
        depth_key: DepthKey::Z(10),
        localizer: Localizer::default(),
        meta: (),
    }];
    let path1 = path_from_dispatch(&router.handle_with_hits::<()>(&hits1));

    // Second hover moves to sibling branch: 1→4
    let hits2 = vec![ResolvedHit {
        node: Node(4),
        path: None,
        depth_key: DepthKey::Z(12),
        localizer: Localizer::default(),
        meta: (),
    }];
    let path2 = path_from_dispatch(&router.handle_with_hits::<()>(&hits2));

    let mut hover: HoverState<Node> = HoverState::new();
    let ev1 = hover.update_path(&path1);
    println!("== Hover (first) ==\n  {:?}", ev1);
    let ev2 = hover.update_path(&path2);
    println!("== Hover (second) ==\n  {:?}", ev2);

    assert_eq!(
        ev1,
        vec![
            HoverEvent::Enter(Node(1)),
            HoverEvent::Enter(Node(2)),
            HoverEvent::Enter(Node(3))
        ]
    );
    assert_eq!(
        ev2,
        vec![
            HoverEvent::Leave(Node(3)),
            HoverEvent::Leave(Node(2)),
            HoverEvent::Enter(Node(4))
        ]
    );
}
