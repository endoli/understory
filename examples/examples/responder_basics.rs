// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Router basics.
//!
//! This minimal example ranks hits by depth, reconstructs a path via parents
//! when needed, and emits the capture → target → bubble dispatch sequence.
//!
//! Run:
//! - `cargo run -p understory_examples --example responder_basics`

use understory_responder::router::Router;
use understory_responder::types::{DepthKey, Localizer, ParentLookup, ResolvedHit, WidgetLookup};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
struct Node(u32);

struct Lookup;
impl WidgetLookup<Node> for Lookup {
    type WidgetId = u32;
    fn widget_of(&self, node: &Node) -> Option<Self::WidgetId> {
        Some(node.0)
    }
}

struct Parents;
impl ParentLookup<Node> for Parents {
    fn parent_of(&self, node: &Node) -> Option<Node> {
        match node.0 {
            3 => Some(Node(2)),
            2 => Some(Node(1)),
            _ => None,
        }
    }
}

fn main() {
    let router: Router<Node, Lookup, Parents> = Router::with_parent(Lookup, Parents);

    // Two hits: Node(3) has higher Z and wins over Node(9).
    let hits = vec![
        ResolvedHit {
            node: Node(9),
            path: Some(vec![Node(9)]),
            depth_key: DepthKey::Z(5),
            localizer: Localizer::default(),
            meta: (),
        },
        ResolvedHit {
            node: Node(3),
            path: None,
            depth_key: DepthKey::Z(10),
            localizer: Localizer::default(),
            meta: (),
        },
    ];

    let out = router.handle_with_hits::<()>(&hits);
    println!("== Dispatch (capture → target → bubble) ==");
    for d in out {
        println!("  {:?}  node={:?}  widget={:?}", d.phase, d.node, d.widget);
    }
}
