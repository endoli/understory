// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Damage summary types returned from commit.

use alloc::vec::Vec;
use kurbo::Rect;

/// A batched set of changes derived from [`crate::Tree::commit`].
///
/// `Damage` is intentionally coarse: it summarizes regions that may have
/// changed between the previous and current commit, sufficient to bound
/// repaint or visibility work. Rectangles may overlap and are not deduplicated;
/// callers can merge or simplify them if needed. It is not guaranteed to be a
/// minimal cover.
#[derive(Clone, Debug, Default)]
pub struct Damage {
    /// World-space rectangles that should be repainted or re-evaluated.
    ///
    /// These include previous and new bounds for nodes whose world-space
    /// rectangles changed, plus any spatial-index damage. Callers can use
    /// this to bound paint or query traversals.
    pub dirty_rects: Vec<Rect>,
}

impl Damage {
    /// Returns the union of all damage rects.
    pub fn union_rect(&self) -> Option<Rect> {
        let mut it = self.dirty_rects.iter().copied();
        let first = it.next()?;
        Some(it.fold(first, |acc, r| acc.union(r)))
    }
}
