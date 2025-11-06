// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Index basics.
//!
//! Insert, update, commit damage, and query.
//!
//! Run:
//! - `cargo run -p understory_examples --example index_basics`

use understory_index::{Aabb2D, Index};

fn main() {
    let mut idx: Index<i64, u32> = Index::new();
    let k1 = idx.insert(Aabb2D::new(0, 0, 10, 10), 1);
    let _k2 = idx.insert(Aabb2D::new(5, 5, 15, 15), 2);
    let _ = idx.commit();

    // Move box 1
    idx.update(k1, Aabb2D::new(20, 0, 30, 10));
    let dmg = idx.commit();
    println!(
        "damage: added={:?}, removed={:?}, moved={:?}",
        dmg.added, dmg.removed, dmg.moved
    );

    // Query a point
    let hits: Vec<_> = idx.query_point(6, 6).collect();
    println!("hits at (6,6): {:?}", hits);
}
