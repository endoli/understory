// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Damage summary types returned from commit.

use kurbo::Rect;

/// A batched set of changes derived from [`crate::Tree::commit`].
#[derive(Clone, Debug, Default)]
pub struct Damage {
    /// World-space rectangles that should be repainted.
    pub dirty_rects: alloc::vec::Vec<Rect>,
}

impl Damage {
    /// Returns the union of all damage rects.
    pub fn union_rect(&self) -> Option<Rect> {
        let mut it = self.dirty_rects.iter().copied();
        let first = it.next()?;
        Some(it.fold(first, |acc, r| acc.union(r)))
    }
}
