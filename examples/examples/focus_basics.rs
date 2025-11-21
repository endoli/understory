// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Focus policy basics.
//!
//! Demonstrate driving focus over a tiny 2D layout using `understory_focus`.
//!
//! Run:
//! - `cargo run -p understory_examples --example focus_basics`

use kurbo::Rect;
use understory_focus::{DefaultPolicy, FocusEntry, FocusPolicy, FocusSpace, Navigation, WrapMode};

fn main() {
    // Three buttons laid out left-to-right.
    let entries = vec![
        FocusEntry {
            id: "left",
            rect: Rect::new(0.0, 0.0, 80.0, 40.0),
            order: None,
            group: None,
            enabled: true,
            scope_depth: 0,
        },
        FocusEntry {
            id: "center",
            rect: Rect::new(90.0, 0.0, 170.0, 40.0),
            order: None,
            group: None,
            enabled: true,
            scope_depth: 0,
        },
        FocusEntry {
            id: "right",
            rect: Rect::new(180.0, 0.0, 260.0, 40.0),
            order: None,
            group: None,
            enabled: true,
            scope_depth: 0,
        },
    ];

    let space = FocusSpace { nodes: &entries };
    let policy = DefaultPolicy {
        wrap: WrapMode::Scope,
    };

    let mut current = "left";
    println!("Start focus at: {current}");

    for nav in [
        Navigation::Right, // left -> center (directional)
        Navigation::Left,  // center -> left (directional)
        Navigation::Next,  // left -> center (linear)
        Navigation::Next,  // center -> right (linear)
        Navigation::Prev,  // right -> center (linear backward)
        Navigation::Prev,  // center -> left (linear backward)
    ] {
        if let Some(next) = policy.next(current, nav, &space) {
            println!("{nav:?}: {current} -> {next}");
            current = next;
        } else {
            println!("{nav:?}: {current} (no change)");
        }
    }
}
