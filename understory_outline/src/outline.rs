// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Visible-row projection controller.

use alloc::vec::Vec;
use hashbrown::HashMap;

use crate::{ExpansionState, OutlineModel};

/// Metadata about one currently visible outline row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleRow<K> {
    /// Stable key identifying this row.
    pub key: K,
    /// Indentation depth in the current visible projection.
    pub depth: usize,
    /// Whether the row currently has at least one child.
    pub has_children: bool,
    /// Whether the row is currently expanded.
    pub is_expanded: bool,
}

/// A controller that caches the visible row projection for an [`OutlineModel`].
///
/// `Outline` owns a model plus explicit expansion state. Hosts query
/// [`Outline::visible_rows`] or related helpers to obtain the flattened visible
/// row sequence and can then render or virtualize that sequence as needed.
#[derive(Clone, Debug)]
pub struct Outline<M>
where
    M: OutlineModel,
{
    model: M,
    expansion: ExpansionState<M::Key>,
    rows: Vec<VisibleRow<M::Key>>,
    row_indices: HashMap<M::Key, usize>,
    stack: Vec<(M::Key, usize)>,
    dirty: bool,
}

impl<M> Outline<M>
where
    M: OutlineModel,
{
    /// Creates a new outline with an empty expansion state.
    #[must_use]
    pub fn new(model: M) -> Self {
        Self::from_parts(model, ExpansionState::new())
    }

    /// Creates a new outline from an explicit model and expansion state.
    #[must_use]
    pub fn from_parts(model: M, expansion: ExpansionState<M::Key>) -> Self {
        Self {
            model,
            expansion,
            rows: Vec::new(),
            row_indices: HashMap::new(),
            stack: Vec::new(),
            dirty: true,
        }
    }

    /// Returns a shared reference to the underlying model.
    #[must_use]
    pub const fn model(&self) -> &M {
        &self.model
    }

    /// Returns a mutable reference to the underlying model and marks the
    /// visible projection dirty.
    pub fn model_mut(&mut self) -> &mut M {
        self.mark_dirty();
        &mut self.model
    }

    /// Consumes the outline and returns the underlying model.
    #[must_use]
    pub fn into_model(self) -> M {
        self.model
    }

    /// Consumes the outline and returns the model and expansion state.
    #[must_use]
    pub fn into_parts(self) -> (M, ExpansionState<M::Key>) {
        (self.model, self.expansion)
    }

    /// Marks the cached visible projection dirty.
    ///
    /// Call this when the underlying model changes through interior mutability
    /// or any other mechanism that bypasses [`Outline::model_mut`].
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Returns a shared reference to the current expansion state.
    #[must_use]
    pub const fn expansion(&self) -> &ExpansionState<M::Key> {
        &self.expansion
    }

    /// Returns `true` if `key` is currently expanded.
    #[must_use]
    pub fn is_expanded(&self, key: &M::Key) -> bool
    where
        M::Key: Eq,
    {
        self.expansion.is_expanded(key)
    }

    /// Sets the expansion state for `key`.
    ///
    /// Returns `true` if the semantic state changed.
    pub fn set_expanded(&mut self, key: M::Key, expanded: bool) -> bool
    where
        M::Key: Eq,
    {
        let changed = self.expansion.set_expanded(key, expanded);
        if changed {
            self.dirty = true;
        }
        changed
    }

    /// Toggles the expansion state for `key`.
    ///
    /// Returns the new expanded state.
    pub fn toggle_expanded(&mut self, key: M::Key) -> bool
    where
        M::Key: Clone + Eq,
    {
        let expanded = self.expansion.toggle(key);
        self.dirty = true;
        expanded
    }

    /// Replaces the expanded set with `keys`.
    ///
    /// Returns `true` if the semantic state changed.
    pub fn replace_expanded<I>(&mut self, keys: I) -> bool
    where
        I: IntoIterator<Item = M::Key>,
        M::Key: Clone + Eq,
    {
        let changed = self.expansion.replace_with(keys);
        if changed {
            self.dirty = true;
        }
        changed
    }

    /// Clears all expanded keys.
    ///
    /// Returns `true` if any key was cleared.
    pub fn clear_expanded(&mut self) -> bool
    where
        M::Key: Eq,
    {
        let changed = self.expansion.clear();
        if changed {
            self.dirty = true;
        }
        changed
    }

    /// Returns the current visible row projection, rebuilding it if necessary.
    pub fn visible_rows(&mut self) -> &[VisibleRow<M::Key>] {
        self.rebuild_if_dirty();
        &self.rows
    }

    /// Returns the number of currently visible rows.
    pub fn visible_len(&mut self) -> usize {
        self.visible_rows().len()
    }

    /// Returns `true` if there are no visible rows.
    pub fn visible_is_empty(&mut self) -> bool {
        self.visible_rows().is_empty()
    }

    /// Returns the visible row at `index`, if any.
    pub fn visible_row(&mut self, index: usize) -> Option<&VisibleRow<M::Key>> {
        self.visible_rows().get(index)
    }

    /// Returns the visible index for `key`, if it is currently projected.
    pub fn index_of_key(&mut self, key: &M::Key) -> Option<usize> {
        self.rebuild_if_dirty();
        self.row_indices.get(key).copied()
    }

    /// Resolves the item for `key`.
    pub fn item(&self, key: &M::Key) -> Option<M::Item> {
        self.model.item(key)
    }

    /// Resolves the item at visible row `index`, if any.
    pub fn item_at_visible(&mut self, index: usize) -> Option<M::Item> {
        let key = self.visible_row(index)?.key.clone();
        self.model.item(&key)
    }

    fn rebuild_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        self.rebuild();
        self.dirty = false;
    }

    fn rebuild(&mut self) {
        self.rows.clear();
        self.row_indices.clear();
        self.stack.clear();

        if let Some(root) = self.model.first_root_key() {
            self.stack.push((root, 0));
        }

        while let Some((key, depth)) = self.stack.pop() {
            if !self.model.contains_key(&key) {
                continue;
            }

            if let Some(next_sibling) = self.model.next_sibling_key(&key)
                && self.model.contains_key(&next_sibling)
            {
                self.stack.push((next_sibling, depth));
            }

            let first_child = self
                .model
                .first_child_key(&key)
                .filter(|child| self.model.contains_key(child));
            let has_children = first_child.is_some();
            let is_expanded = has_children && self.expansion.is_expanded(&key);

            let visible_index = self.rows.len();
            self.rows.push(VisibleRow {
                key: key.clone(),
                depth,
                has_children,
                is_expanded,
            });
            self.row_indices.insert(key.clone(), visible_index);

            if is_expanded && let Some(first_child) = first_child {
                self.stack.push((first_child, depth + 1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::vec;
    use alloc::vec::Vec;

    use crate::{OutlineModel, OutlineNode, SliceOutline};

    use super::Outline;

    fn sample_outline() -> Outline<SliceOutline<'static, &'static str>> {
        let nodes = Box::leak(Box::new([
            OutlineNode::new("A")
                .with_first_child(Some(1))
                .with_next_sibling(Some(4)),
            OutlineNode::new("A1")
                .with_first_child(Some(3))
                .with_next_sibling(Some(2)),
            OutlineNode::new("A2"),
            OutlineNode::new("A1a"),
            OutlineNode::new("B"),
        ]));
        Outline::new(SliceOutline::new(nodes, Some(0)))
    }

    #[test]
    fn collapsed_outline_shows_roots_only() {
        let mut outline = sample_outline();
        let rows = outline.visible_rows();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].key, 0);
        assert_eq!(rows[1].key, 4);
        assert!(rows[0].has_children);
        assert!(!rows[0].is_expanded);
    }

    #[test]
    fn expansion_reveals_nested_rows_with_depth() {
        let mut outline = sample_outline();
        assert!(outline.set_expanded(0, true));
        assert!(outline.set_expanded(1, true));

        let rows = outline.visible_rows();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[2].depth, 2);
        assert_eq!(rows[3].depth, 1);
        assert_eq!(rows[4].depth, 0);
    }

    #[test]
    fn collapsing_parent_keeps_descendant_expansion_state() {
        let mut outline = sample_outline();
        assert!(outline.set_expanded(0, true));
        assert!(outline.set_expanded(1, true));
        assert!(outline.set_expanded(0, false));
        assert!(outline.expansion().is_expanded(&1));

        assert!(outline.set_expanded(0, true));
        let keys: Vec<_> = outline.visible_rows().iter().map(|row| row.key).collect();
        assert_eq!(keys, vec![0, 1, 3, 2, 4]);
    }

    #[test]
    fn index_and_item_lookup_follow_visible_projection() {
        let mut outline = sample_outline();
        assert!(outline.set_expanded(0, true));

        assert_eq!(outline.index_of_key(&2), Some(2));
        assert_eq!(outline.item_at_visible(1), Some(&"A1"));
        assert_eq!(outline.item_at_visible(99), None);
    }

    #[test]
    fn mark_dirty_refreshes_projection_after_model_mutation() {
        #[derive(Clone, Debug)]
        struct MutableModel {
            next_root: Option<usize>,
        }

        impl OutlineModel for MutableModel {
            type Key = usize;
            type Item = &'static str;

            fn first_root_key(&self) -> Option<Self::Key> {
                Some(0)
            }

            fn contains_key(&self, key: &Self::Key) -> bool {
                *key <= 1
            }

            fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
                match *key {
                    0 => self.next_root,
                    _ => None,
                }
            }

            fn first_child_key(&self, _key: &Self::Key) -> Option<Self::Key> {
                None
            }

            fn item(&self, key: &Self::Key) -> Option<Self::Item> {
                match *key {
                    0 => Some("A"),
                    1 => Some("B"),
                    _ => None,
                }
            }
        }

        let mut outline = Outline::new(MutableModel { next_root: Some(1) });
        assert_eq!(outline.visible_len(), 2);

        outline.model_mut().next_root = None;
        assert_eq!(outline.visible_len(), 1);
    }
}
