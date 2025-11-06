// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Focus routing and transitions.
//!
//! Demonstrate dispatching to focus via `dispatch_for` and computing focus
//! enter/leave transitions via `FocusState`.
//!
//! Run:
//! - `cargo run -p understory_examples --example responder_focus`

use understory_responder::focus::{FocusEvent, FocusState};
use understory_responder::router::Router;
use understory_responder::types::{ParentLookup, WidgetLookup};

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

    // Focus path: 1→2→3
    let dispatch1 = router.dispatch_for::<()>(Node(3));
    println!("== Focus dispatch (to 3) ==");
    for d in &dispatch1 {
        println!("  {:?}  node={:?}  widget={:?}", d.phase, d.node, d.widget);
    }

    // Compute FocusState transitions between focus paths.
    let mut focus: FocusState<Node> = FocusState::new();
    let ev1 = focus.update_path(&[Node(1), Node(2), Node(3)]);
    println!("== Focus transitions (first) ==\n  {:?}", ev1);

    // Move focus to sibling branch: 1→4
    let dispatch2 = router.dispatch_for::<()>(Node(4));
    println!("== Focus dispatch (to 4) ==");
    for d in &dispatch2 {
        println!("  {:?}  node={:?}  widget={:?}", d.phase, d.node, d.widget);
    }
    let ev2 = focus.update_path(&[Node(1), Node(4)]);
    println!("== Focus transitions (second) ==\n  {:?}", ev2);

    assert_eq!(
        ev1,
        vec![
            FocusEvent::Enter(Node(1)),
            FocusEvent::Enter(Node(2)),
            FocusEvent::Enter(Node(3))
        ]
    );
    assert_eq!(
        ev2,
        vec![
            FocusEvent::Leave(Node(3)),
            FocusEvent::Leave(Node(2)),
            FocusEvent::Enter(Node(4))
        ]
    );
}
