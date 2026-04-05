// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dense, index-addressable outline models.

use crate::OutlineModel;

/// A dense outline node addressed by its `usize` position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutlineNode<T> {
    /// The node's payload.
    pub item: T,
    /// The first child of this node, if any.
    pub first_child: Option<usize>,
    /// The next sibling of this node, if any.
    pub next_sibling: Option<usize>,
}

impl<T> OutlineNode<T> {
    /// Creates a leaf node with no children or siblings.
    #[must_use]
    pub const fn new(item: T) -> Self {
        Self {
            item,
            first_child: None,
            next_sibling: None,
        }
    }

    /// Returns a copy of this node with `first_child` set.
    #[must_use]
    pub const fn with_first_child(mut self, first_child: Option<usize>) -> Self {
        self.first_child = first_child;
        self
    }

    /// Returns a copy of this node with `next_sibling` set.
    #[must_use]
    pub const fn with_next_sibling(mut self, next_sibling: Option<usize>) -> Self {
        self.next_sibling = next_sibling;
        self
    }
}

/// A simple slice-backed outline model.
///
/// `SliceOutline` is the reference implementation for this crate's core traits:
/// keys are dense `usize` indices, traversal follows `first_child` and
/// `next_sibling` links, and resolving an item borrows directly from the
/// underlying slice.
#[derive(Clone, Copy, Debug)]
pub struct SliceOutline<'a, T> {
    nodes: &'a [OutlineNode<T>],
    first_root: Option<usize>,
}

impl<'a, T> SliceOutline<'a, T> {
    /// Creates a new slice-backed outline with the given `first_root`.
    #[must_use]
    pub const fn new(nodes: &'a [OutlineNode<T>], first_root: Option<usize>) -> Self {
        Self { nodes, first_root }
    }

    /// Returns the underlying node slice.
    #[must_use]
    pub const fn nodes(&self) -> &'a [OutlineNode<T>] {
        self.nodes
    }
}

impl<'a, T> OutlineModel for SliceOutline<'a, T> {
    type Key = usize;
    type Item = &'a T;

    fn first_root_key(&self) -> Option<Self::Key> {
        self.first_root.filter(|key| self.contains_key(key))
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        *key < self.nodes.len()
    }

    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.nodes.get(*key).and_then(|node| node.next_sibling)
    }

    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
        self.nodes.get(*key).and_then(|node| node.first_child)
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        self.nodes.get(*key).map(|node| &node.item)
    }
}

#[cfg(test)]
mod tests {
    use crate::OutlineModel;

    use super::{OutlineNode, SliceOutline};

    #[test]
    fn slice_outline_traverses_roots_children_and_items() {
        let nodes = [
            OutlineNode::new("root").with_first_child(Some(1)),
            OutlineNode::new("child").with_next_sibling(Some(2)),
            OutlineNode::new("sibling"),
        ];
        let outline = SliceOutline::new(&nodes, Some(0));

        assert_eq!(outline.first_root_key(), Some(0));
        assert_eq!(outline.first_child_key(&0), Some(1));
        assert_eq!(outline.next_sibling_key(&1), Some(2));
        assert_eq!(outline.item(&2), Some(&"sibling"));
    }
}
