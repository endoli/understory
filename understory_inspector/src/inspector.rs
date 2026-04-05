// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Inspector controller.

use alloc::vec::Vec;
use core::ops::Range;

use understory_outline::{Outline, VisibleRow};
use understory_selection::Selection;
use understory_virtual_list::{FixedExtentModel, VirtualList};

use crate::{InspectorConfig, InspectorModel};

/// Host-side controller for inspecting an outline-backed hierarchy.
///
/// `Inspector` owns:
/// - an [`Outline`] projection,
/// - a fixed-row [`VirtualList`] controller,
/// - the current focused key,
/// - and a [`Selection`] used for single/range selection semantics.
///
/// It deliberately does not own rendering, icons, badges, columns, or
/// domain-specific commands.
#[derive(Clone, Debug)]
pub struct Inspector<M>
where
    M: InspectorModel,
{
    outline: Outline<M>,
    list: VirtualList<FixedExtentModel<f64>>,
    config: InspectorConfig,
    focus: Option<M::Key>,
    focus_index: Option<usize>,
    selection: Selection<M::Key>,
}

impl<M> Inspector<M>
where
    M: InspectorModel,
{
    /// Creates a new inspector over `model` with `config`.
    #[must_use]
    pub fn new(model: M, config: InspectorConfig) -> Self {
        let config = InspectorConfig::new(
            config.row_extent.max(0.0),
            config.viewport_extent.max(0.0),
            config.overscan_before.max(0.0),
            config.overscan_after.max(0.0),
            config.scroll_align,
        );
        let mut outline = Outline::new(model);
        let len = outline.visible_len();
        let mut list = VirtualList::new(
            FixedExtentModel::new(len, config.row_extent),
            config.viewport_extent,
            0.0,
        );
        list.set_overscan(config.overscan_before, config.overscan_after);

        let mut this = Self {
            outline,
            list,
            config,
            focus: None,
            focus_index: None,
            selection: Selection::new(),
        };
        this.sync();
        this
    }

    /// Returns a shared reference to the underlying model.
    #[must_use]
    pub fn model(&self) -> &M {
        self.outline.model()
    }

    /// Returns a mutable reference to the underlying model and marks the
    /// outline projection dirty.
    ///
    /// Call [`Self::sync`] after mutating the model so focus, selection, and
    /// virtualization are reconciled against the new visible rows.
    pub fn model_mut(&mut self) -> &mut M {
        self.outline.model_mut()
    }

    /// Marks the outline projection dirty.
    ///
    /// Call this when the model changes through interior mutability or any
    /// path that bypasses [`Self::model_mut`].
    pub fn mark_dirty(&mut self) {
        self.outline.mark_dirty();
    }

    /// Returns the currently focused key, if any.
    #[must_use]
    pub fn focus(&self) -> Option<&M::Key> {
        self.focus.as_ref()
    }

    /// Returns the current selection state.
    #[must_use]
    pub fn selection(&self) -> &Selection<M::Key> {
        &self.selection
    }

    /// Returns a mutable reference to the selection state.
    ///
    /// Call [`Self::sync`] after mutating selection if hidden keys should be
    /// pruned against the current visible rows.
    pub fn selection_mut(&mut self) -> &mut Selection<M::Key> {
        &mut self.selection
    }

    /// Returns the current visible-row projection.
    pub fn visible_rows(&mut self) -> &[VisibleRow<M::Key>] {
        self.outline.visible_rows()
    }

    /// Returns the number of currently visible rows.
    pub fn visible_len(&mut self) -> usize {
        self.outline.visible_len()
    }

    /// Returns the visible index for `key`, if it is currently projected.
    pub fn index_of_key(&mut self, key: &M::Key) -> Option<usize> {
        self.outline.index_of_key(key)
    }

    /// Resolves the item associated with `key`.
    pub fn item(&self, key: &M::Key) -> Option<M::Item> {
        self.outline.item(key)
    }

    /// Returns the realized visible-row range for the current viewport and overscan.
    #[must_use]
    pub fn realized_range(&mut self) -> Range<usize> {
        let strip = self.list.visible_strip();
        strip.start..strip.end
    }

    /// Returns the current scroll offset.
    #[must_use]
    pub fn scroll_offset(&self) -> f64 {
        self.list.scroll_offset()
    }

    /// Returns the configured viewport extent.
    #[must_use]
    pub fn viewport_extent(&self) -> f64 {
        self.list.viewport_extent()
    }

    /// Returns the overscan extent applied before the viewport.
    #[must_use]
    pub fn overscan_before(&self) -> f64 {
        self.list.overscan_before()
    }

    /// Returns the overscan extent applied after the viewport.
    #[must_use]
    pub fn overscan_after(&self) -> f64 {
        self.list.overscan_after()
    }

    /// Sets the scroll offset.
    pub fn set_scroll_offset(&mut self, offset: f64) {
        self.list.set_scroll_offset(offset);
        self.list.clamp_scroll_to_content();
    }

    /// Sets the viewport extent.
    pub fn set_viewport_extent(&mut self, extent: f64) {
        self.list.set_viewport_extent(extent);
        self.list.clamp_scroll_to_content();
    }

    /// Rebuilds the outline projection if needed and reconciles focus,
    /// selection, and virtual-list length.
    pub fn sync(&mut self) {
        let visible_len = self.outline.visible_len();
        self.list.model_mut().set_len(visible_len);
        self.reconcile_focus();
        self.prune_selection_to_visible();
        self.scroll_focus_into_view();
        self.list.clamp_scroll_to_content();
    }

    /// Sets the focused key if it is currently visible.
    ///
    /// Passing `None` clears focus.
    ///
    /// Returns `true` if the focus changed.
    pub fn set_focus(&mut self, key: Option<M::Key>) -> bool {
        match key {
            Some(key) => {
                let Some(index) = self.find_visible_index(&key, None) else {
                    return false;
                };
                if self.focus.as_ref() == Some(&key) {
                    return false;
                }
                self.focus = Some(key);
                self.focus_index = Some(index);
                self.scroll_focus_into_view();
                true
            }
            None => {
                if self.focus.is_none() {
                    return false;
                }
                self.focus = None;
                self.focus_index = None;
                true
            }
        }
    }

    /// Focuses the first visible row, if any.
    pub fn focus_first(&mut self) -> bool {
        let Some(key) = self.visible_row_key(0) else {
            return false;
        };
        if self.focus.as_ref() == Some(&key) && self.focus_index == Some(0) {
            return false;
        }
        self.focus = Some(key);
        self.focus_index = Some(0);
        self.scroll_focus_into_view();
        true
    }

    /// Focuses the previous visible row.
    ///
    /// If no row is currently focused, this focuses the last visible row.
    pub fn focus_prev(&mut self) -> bool {
        self.step_focus(Direction::Backward)
    }

    /// Focuses the next visible row.
    ///
    /// If no row is currently focused, this focuses the first visible row.
    pub fn focus_next(&mut self) -> bool {
        self.step_focus(Direction::Forward)
    }

    /// Expands `key` if it currently has children.
    pub fn expand(&mut self, key: M::Key) -> bool {
        if !self.key_has_children(&key) {
            return false;
        }
        let changed = self.outline.set_expanded(key, true);
        if changed {
            self.sync();
        }
        changed
    }

    /// Collapses `key`.
    pub fn collapse(&mut self, key: M::Key) -> bool {
        let changed = self.outline.set_expanded(key, false);
        if changed {
            self.sync();
        }
        changed
    }

    /// Toggles expansion for `key` if it currently has children.
    pub fn toggle(&mut self, key: M::Key) -> bool {
        if !self.key_has_children(&key) {
            return false;
        }
        let _ = self.outline.toggle_expanded(key);
        self.sync();
        true
    }

    /// Expands the focused key if it currently has children.
    pub fn expand_focused(&mut self) -> bool {
        let Some(key) = self.focus.clone() else {
            return false;
        };
        self.expand(key)
    }

    /// Collapses the focused key.
    pub fn collapse_focused(&mut self) -> bool {
        let Some(key) = self.focus.clone() else {
            return false;
        };
        self.collapse(key)
    }

    /// Toggles the focused key if it currently has children.
    pub fn toggle_focused(&mut self) -> bool {
        let Some(key) = self.focus.clone() else {
            return false;
        };
        self.toggle(key)
    }

    /// Replaces selection with the focused key, setting both primary and anchor.
    pub fn select_only_focused(&mut self) -> bool {
        let Some(key) = self.focus.clone() else {
            return false;
        };
        let revision = self.selection.revision();
        self.selection.select_only(key);
        self.selection.revision() != revision
    }

    /// Extends selection to the next visible row using the current anchor.
    pub fn extend_selection_next(&mut self) -> bool {
        self.extend_selection(Direction::Forward)
    }

    /// Extends selection to the previous visible row using the current anchor.
    pub fn extend_selection_prev(&mut self) -> bool {
        self.extend_selection(Direction::Backward)
    }

    /// Scrolls the focused row into view using the configured alignment.
    pub fn scroll_focus_into_view(&mut self) {
        if let Some(index) = self.focus_index
            && let Some(offset) = self
                .list
                .target_offset_for_index(index, self.config.scroll_align)
        {
            self.list.set_scroll_offset(offset);
        }
    }

    fn extend_selection(&mut self, direction: Direction) -> bool {
        if self.focus.is_none() {
            let changed = match direction {
                Direction::Forward => self.focus_first(),
                Direction::Backward => self.step_focus(Direction::Backward),
            };
            if changed {
                let _ = self.select_only_focused();
            }
            return changed;
        }

        if self.selection.anchor().is_none() {
            let _ = self.select_only_focused();
        }

        let Some(target_index) = self.adjacent_index(direction) else {
            return false;
        };

        self.replace_selection_range_to(target_index);
        self.focus = self.visible_row_key(target_index);
        self.focus_index = Some(target_index);
        self.scroll_focus_into_view();
        true
    }

    fn replace_selection_range_to(&mut self, target_index: usize) {
        let Some(target) = self.visible_row_key(target_index) else {
            return;
        };

        let anchor = self
            .selection
            .anchor()
            .cloned()
            .unwrap_or_else(|| target.clone());
        let anchor_hint = self.focus_index.map(|index| index.min(target_index));
        let Some(a) = self.find_visible_index(&anchor, anchor_hint) else {
            self.selection.select_only(target);
            return;
        };
        let b = target_index;

        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        let range_keys = self
            .outline
            .visible_rows()
            .iter()
            .skip(start)
            .take(end - start + 1)
            .map(|row| row.key.clone());
        self.selection.replace_with(range_keys);
    }

    fn reconcile_focus(&mut self) {
        let visible_len = self.outline.visible_len();
        if visible_len == 0 {
            self.focus = None;
            self.focus_index = None;
            return;
        }

        let Some(current) = self.focus.clone() else {
            self.focus_index = None;
            return;
        };

        if let Some(index) = self.focus_index
            && self
                .outline
                .visible_row(index)
                .is_some_and(|row| row.key == current)
        {
            return;
        }

        if let Some(index) = self.find_visible_index(&current, self.focus_index) {
            self.focus_index = Some(index);
            return;
        }

        if let Some(parent) = self.outline.model().parent_key(&current)
            && let Some(index) = self.find_parent_index(&parent)
        {
            self.focus = Some(parent);
            self.focus_index = Some(index);
            return;
        }

        self.focus = self.visible_row_key(0);
        self.focus_index = self.focus.as_ref().map(|_| 0);
    }

    fn prune_selection_to_visible(&mut self) {
        let selected = self.selection.items().to_vec();
        let kept = selected
            .iter()
            .filter(|key| self.find_visible_index(key, None).is_some())
            .cloned()
            .collect::<Vec<_>>();
        self.selection.replace_with(kept);
    }

    fn key_has_children(&self, key: &M::Key) -> bool {
        self.outline
            .model()
            .first_child_key(key)
            .filter(|child| self.outline.model().contains_key(child))
            .is_some()
    }

    fn step_focus(&mut self, direction: Direction) -> bool {
        let Some(index) = self.adjacent_index(direction) else {
            return false;
        };
        let Some(key) = self.visible_row_key(index) else {
            return false;
        };
        if self.focus.as_ref() == Some(&key) && self.focus_index == Some(index) {
            return false;
        }
        self.focus = Some(key);
        self.focus_index = Some(index);
        self.scroll_focus_into_view();
        true
    }

    fn adjacent_index(&mut self, direction: Direction) -> Option<usize> {
        let len = self.outline.visible_len();
        if len == 0 {
            return None;
        }

        match direction {
            Direction::Forward => match self.focus_index {
                Some(index) if index + 1 < len => Some(index + 1),
                Some(_) => None,
                None => Some(0),
            },
            Direction::Backward => match self.focus_index {
                Some(index) if index > 0 => Some(index - 1),
                Some(_) => None,
                None => Some(len - 1),
            },
        }
    }

    fn visible_row_key(&mut self, index: usize) -> Option<M::Key> {
        self.outline.visible_row(index).map(|row| row.key.clone())
    }

    fn find_visible_index(&mut self, key: &M::Key, hint: Option<usize>) -> Option<usize> {
        if let Some(index) = hint
            && let Some(row) = self.outline.visible_row(index)
            && &row.key == key
        {
            return Some(index);
        }

        if let Some(index) = hint {
            let rows = self.outline.visible_rows();
            for (offset, row) in rows.iter().enumerate().skip(index.saturating_add(1)) {
                if &row.key == key {
                    return Some(offset);
                }
            }
            for (offset, row) in rows.iter().enumerate().take(index) {
                if &row.key == key {
                    return Some(offset);
                }
            }
            None
        } else {
            self.outline.index_of_key(key)
        }
    }

    fn find_parent_index(&mut self, parent: &M::Key) -> Option<usize> {
        let rows = self.outline.visible_rows();
        let Some(hint) = self.focus_index else {
            return rows.iter().position(|row| &row.key == parent);
        };

        let start = hint.min(rows.len());
        for index in (0..start).rev() {
            if rows[index].key == *parent {
                return Some(index);
            }
        }

        rows.iter().position(|row| &row.key == parent)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Forward,
    Backward,
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use crate::InspectorModel;

    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum Key {
        Root,
        ChildA,
        ChildB,
        Tail,
    }

    struct TestModel;

    impl understory_outline::OutlineModel for TestModel {
        type Key = Key;
        type Item = &'static str;

        fn first_root_key(&self) -> Option<Self::Key> {
            Some(Key::Root)
        }

        fn contains_key(&self, _key: &Self::Key) -> bool {
            true
        }

        fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
            match key {
                Key::Root => Some(Key::Tail),
                Key::ChildA => Some(Key::ChildB),
                Key::ChildB | Key::Tail => None,
            }
        }

        fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
            match key {
                Key::Root => Some(Key::ChildA),
                Key::ChildA | Key::ChildB | Key::Tail => None,
            }
        }

        fn item(&self, key: &Self::Key) -> Option<Self::Item> {
            Some(match key {
                Key::Root => "Root",
                Key::ChildA => "Child A",
                Key::ChildB => "Child B",
                Key::Tail => "Tail",
            })
        }
    }

    impl InspectorModel for TestModel {
        fn parent_key(&self, key: &Self::Key) -> Option<Self::Key> {
            match key {
                Key::ChildA | Key::ChildB => Some(Key::Root),
                Key::Root | Key::Tail => None,
            }
        }
    }

    fn sample_inspector() -> Inspector<TestModel> {
        let config = InspectorConfig::fixed_rows(10.0, 20.0);
        Inspector::new(TestModel, config)
    }

    #[test]
    fn starts_unfocused_and_collapsed() {
        let mut inspector = sample_inspector();
        let rows = inspector
            .visible_rows()
            .iter()
            .map(|row| row.key)
            .collect::<Vec<_>>();

        assert!(inspector.focus().is_none());
        assert_eq!(rows, vec![Key::Root, Key::Tail]);
    }

    #[test]
    fn focus_next_follows_visible_row_order() {
        let mut inspector = sample_inspector();
        assert!(inspector.focus_next());
        assert_eq!(inspector.focus(), Some(&Key::Root));

        assert!(inspector.expand(Key::Root));
        assert!(inspector.focus_next());
        assert_eq!(inspector.focus(), Some(&Key::ChildA));

        assert!(inspector.focus_next());
        assert_eq!(inspector.focus(), Some(&Key::ChildB));
    }

    #[test]
    fn collapse_reconciles_focus_to_parent() {
        let mut inspector = sample_inspector();
        assert!(inspector.expand(Key::Root));
        let _ = inspector.set_focus(Some(Key::ChildB));
        inspector
            .selection_mut()
            .replace_with([Key::Root, Key::ChildA, Key::ChildB]);

        assert!(inspector.collapse(Key::Root));
        assert_eq!(inspector.focus(), Some(&Key::Root));
        assert_eq!(inspector.selection().items(), &[Key::Root]);
    }

    #[test]
    fn extend_selection_next_uses_visible_anchor_range() {
        let mut inspector = sample_inspector();
        assert!(inspector.expand(Key::Root));
        assert!(inspector.focus_first());
        assert!(inspector.select_only_focused());

        assert!(inspector.extend_selection_next());
        assert_eq!(inspector.focus(), Some(&Key::ChildA));
        assert_eq!(inspector.selection().items(), &[Key::Root, Key::ChildA]);

        assert!(inspector.extend_selection_next());
        assert_eq!(
            inspector.selection().items(),
            &[Key::Root, Key::ChildA, Key::ChildB]
        );
    }

    #[test]
    fn realized_range_tracks_scroll_after_focus_movement() {
        let mut inspector = Inspector::new(
            TestModel,
            InspectorConfig::new(
                10.0,
                10.0,
                0.0,
                0.0,
                understory_virtual_list::ScrollAlign::Nearest,
            ),
        );
        assert!(inspector.expand(Key::Root));
        assert!(inspector.set_focus(Some(Key::Tail)));

        assert_eq!(inspector.realized_range(), 3..4);
        assert_eq!(inspector.scroll_offset(), 30.0);
    }
}
