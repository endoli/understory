// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_selection --heading-base-level=0

//! Understory Selection: selection management primitives.
//!
//! This crate focuses on the _bookkeeping_ of a selection: the set of selected
//! keys plus common higher-level concepts such as a primary item and an anchor
//! used for range extension. It does **not** know anything about how your items
//! are laid out or ordered; callers decide how to map user input (click, toggle,
//! lasso) into concrete sets of keys.
//!
//! The core type is [`Selection`], a small, generic container that tracks:
//! - The set of selected keys.
//! - An optional **primary** key (typically the most recently interacted-with item).
//! - An optional **anchor** key (used as a reference point for shift-click/range selection).
//! - A monotonically increasing **revision** counter that bumps when the
//!   selection changes.
//!
//! The container is intentionally opinionated and compact:
//! - Keys live in a small `Vec<K>` with uniqueness enforced by equality.
//! - No hashing or ordering constraints are imposed on `K`, making it easy to integrate
//!   with existing ID types such as generational handles from a scene tree.
//! - The API exposes simple operations that mirror common UI gestures like
//!   “replace with a single item”, “toggle one item”, and “replace/extend with a batch”.
//!
//! ## Minimal example
//!
//! ```rust
//! use understory_selection::Selection;
//!
//! // Using u32 as a stand-in for an application-specific ID.
//! let mut selection = Selection::<u32>::new();
//!
//! // Simple click: replace selection with a single item.
//! selection.select_only(10);
//! assert_eq!(selection.primary(), Some(&10));
//!
//! // Ctrl-click: toggle a single item.
//! selection.toggle(10);
//! assert!(selection.is_empty());
//!
//! // Lasso or range gesture: compute the affected IDs elsewhere and
//! // then replace the current selection with that batch.
//! selection.replace_with([1, 2, 3]);
//! assert_eq!(selection.len(), 3);
//! ```
//!
//! ## Concepts
//!
//! [`Selection`] models three related pieces of state:
//!
//! - **Selection contents**: a set of keys, stored as a small `Vec<K>` with no duplicates.
//! - **Primary**: an optional distinguished key, typically the most recently interacted-with
//!   item. Many UIs use this as the “focus” of keyboard actions or the reference for
//!   commands like “delete selection”.
//! - **Anchor**: an optional reference key used as a starting point for range extension
//!   (for example, shift-click in a list). The crate does not know how items are ordered;
//!   callers are expected to compute ranges based on their own data structures and then
//!   call methods like [`Selection::replace_with`] or [`Selection::extend_with`].
//!
//! The container is agnostic to the domain: it works equally well for list selections,
//! canvas/infinite-surface editors, or any other place where you want to track a set of
//! selected items plus a primary/anchor.
//!
//! ## List-style click helpers
//!
//! Higher layers typically map pointer + modifier input into selection changes. For a
//! simple list with `click` / `ctrl+click` / `shift+click` semantics, you might write
//! a helper like this:
//!
//! ```rust
//! use understory_selection::Selection;
//!
//! #[derive(Default, Copy, Clone)]
//! struct Modifiers {
//!     ctrl: bool,
//!     shift: bool,
//! }
//!
//! fn handle_click(
//!     selection: &mut Selection<u32>,
//!     clicked: u32,
//!     mods: Modifiers,
//!     items_in_order: &[u32],
//! ) {
//!     if !mods.ctrl && !mods.shift {
//!         // Plain click: replace selection with a single item.
//!         selection.select_only(clicked);
//!         return;
//!     }
//!
//!     if mods.ctrl && !mods.shift {
//!         // Ctrl-click: toggle membership, keep anchor stable.
//!         selection.toggle(clicked);
//!         return;
//!     }
//!
//!     if mods.shift {
//!         // Shift-click: treat anchor as the pivot, build a range between
//!         // anchor and the clicked item according to the list ordering, and
//!         // replace the current selection with that range.
//!         let anchor = selection
//!             .anchor()
//!             .copied()
//!             .unwrap_or(clicked);
//!
//!         let index_of = |value: u32| {
//!             items_in_order
//!                 .iter()
//!                 .position(|&id| id == value)
//!                 .expect("anchor and clicked must be in items_in_order")
//!         };
//!
//!         let a = index_of(anchor);
//!         let b = index_of(clicked);
//!         let (start, end) = if a <= b { (a, b) } else { (b, a) };
//!
//!         let range = items_in_order[start..=end].iter().copied();
//!         selection.replace_with(range);
//!     }
//! }
//!
//! let items = [10_u32, 20, 30, 40];
//! let mut sel = Selection::new();
//!
//! // Click on 20.
//! handle_click(&mut sel, 20, Modifiers::default(), &items);
//! assert_eq!(sel.items(), &[20]);
//!
//! // Shift-click on 40: select the range 20..=40.
//! handle_click(
//!     &mut sel,
//!     40,
//!     Modifiers { ctrl: false, shift: true },
//!     &items,
//! );
//! assert_eq!(sel.items(), &[20, 30, 40]);
//! ```
//!
//! This crate is `no_std` and uses `alloc`.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

/// A small selection container tracking a set of keys plus primary/anchor and a revision.
///
/// `Selection` does not impose hashing or ordering constraints on `T`; it only
/// requires equality for most mutation and query methods. Internally it stores keys
/// in a small `Vec<T>` and enforces uniqueness by scanning for existing entries.
///
/// This keeps the type easy to integrate with existing ID types (for example,
/// generational handles from a scene or box tree) without forcing them to be `Ord`
/// or `Hash`.
#[derive(Clone, Debug, Default)]
pub struct Selection<T> {
    items: Vec<T>,
    primary: Option<usize>,
    anchor: Option<usize>,
    revision: u64,
}

impl<T> Selection<T> {
    /// Creates an empty selection.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: Vec::new(),
            primary: None,
            anchor: None,
            revision: 0,
        }
    }

    /// Returns `true` if the selection is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns the number of selected keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns a slice of all selected keys in their internal order.
    ///
    /// The order is stable within a single `Selection` instance but should not
    /// be relied upon for application semantics; callers are free to interpret it
    /// however they find convenient.
    #[must_use]
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Returns an iterator over the selected keys.
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.items.iter()
    }

    /// Returns a reference to the primary key, if any.
    ///
    /// The primary is typically the most recently interacted-with item in the selection.
    #[must_use]
    pub fn primary(&self) -> Option<&T> {
        self.primary.map(|idx| &self.items[idx])
    }

    /// Returns a reference to the anchor key, if any.
    ///
    /// The anchor is often used as the starting point for range extension (for example,
    /// shift-click in a list). The crate does not compute ranges; callers are expected
    /// to derive them from their own data structures.
    #[must_use]
    pub fn anchor(&self) -> Option<&T> {
        self.anchor.map(|idx| &self.items[idx])
    }

    /// Returns the current revision counter.
    ///
    /// The revision is a monotonically increasing counter local to this `Selection`
    /// instance. It is bumped only when a mutation changes the semantic contents:
    /// selected items, primary, or anchor. No-op calls (for example, selecting the
    /// already-selected singleton) leave it unchanged.
    ///
    /// This is useful for observers that want a cheap “did anything actually change?”
    /// marker without comparing the full contents.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Removes all keys from the selection and clears primary/anchor.
    pub fn clear(&mut self) {
        if self.items.is_empty() && self.primary.is_none() && self.anchor.is_none() {
            return;
        }

        self.items.clear();
        self.primary = None;
        self.anchor = None;
        self.bump_revision();
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

impl<T> Selection<T>
where
    T: PartialEq,
{
    /// Returns `true` if the selection currently contains `key`.
    #[must_use]
    pub fn contains(&self, key: &T) -> bool {
        self.position_of(key).is_some()
    }

    /// Replaces the selection with a single key, setting both primary and anchor.
    ///
    /// This is the typical mapping for a simple click without modifiers.
    pub fn select_only(&mut self, key: T) {
        if self.items.len() == 1
            && self.items.first() == Some(&key)
            && self.primary == Some(0)
            && self.anchor == Some(0)
        {
            return;
        }

        self.items.clear();
        self.items.push(key);
        self.primary = Some(0);
        self.anchor = Some(0);
        self.bump_revision();
    }

    /// Replaces the current selection with the provided batch of keys.
    ///
    /// - Duplicates in the input are ignored.
    /// - If the previous anchor key is still present, it remains the anchor.
    ///   Otherwise the first unique key becomes the anchor (if any keys are present).
    /// - The primary key defaults to the first unique key (if any keys are present).
    ///
    /// This method de-duplicates by scanning the accumulated output, so it is
    /// quadratic in the number of input keys. If you can guarantee the input has
    /// no duplicates (for example, a “Select All” operation over a list), prefer
    /// [`Selection::replace_with_unique`] for linear behavior.
    pub fn replace_with<I>(&mut self, keys: I)
    where
        I: IntoIterator<Item = T>,
    {
        let mut new_items: Vec<T> = Vec::new();
        for key in keys {
            if !new_items.iter().any(|existing| existing == &key) {
                new_items.push(key);
            }
        }
        self.replace_with_items(new_items);
    }

    /// Replaces the current selection with the provided batch of *unique* keys.
    ///
    /// This is a faster variant of [`Selection::replace_with`] for callers that can
    /// guarantee the input has no duplicates. It does **not** perform any de-duplication.
    /// This makes it a good fit for “Select All” style operations, where the caller
    /// typically produces one key per item.
    ///
    /// If the input may contain duplicates, use [`Selection::replace_with`] instead.
    /// If you want to grow the selection incrementally, see [`Selection::extend_with`]
    /// or [`Selection::add`].
    ///
    /// # Panics (debug only)
    ///
    /// Panics in debug builds if the input contains duplicates.
    pub fn replace_with_unique<I>(&mut self, keys: I)
    where
        I: IntoIterator<Item = T>,
    {
        let iter = keys.into_iter();
        let (lower, _) = iter.size_hint();
        let mut new_items: Vec<T> = Vec::with_capacity(lower);
        for key in iter {
            new_items.push(key);
        }

        #[cfg(debug_assertions)]
        debug_assert_unique(&new_items);
        self.replace_with_items(new_items);
    }

    /// Extends the selection with the provided batch of keys.
    ///
    /// - Existing keys remain selected.
    /// - New keys are appended; duplicates in the input are ignored.
    /// - The **primary** key is updated to the last unique key added, if any.
    /// - The **anchor** is left unchanged.
    pub fn extend_with<I>(&mut self, keys: I)
    where
        I: IntoIterator<Item = T>,
    {
        let mut last_added = None;
        for key in keys {
            if self.position_of(&key).is_none() {
                self.items.push(key);
                last_added = Some(self.items.len() - 1);
            }
        }

        if let Some(idx) = last_added {
            // Even if primary already points at `idx`, new items were added.
            self.primary = Some(idx);
            self.bump_revision();
        }
    }

    /// Adds `key` to the selection if it is not already present.
    ///
    /// - If `key` is newly added, it becomes the primary key.
    /// - The anchor is left unchanged.
    pub fn add(&mut self, key: T) {
        if let Some(idx) = self.position_of(&key) {
            if self.primary != Some(idx) {
                self.primary = Some(idx);
                self.bump_revision();
            }
        } else {
            self.items.push(key);
            self.primary = Some(self.items.len() - 1);
            self.bump_revision();
        }
    }

    /// Removes `key` from the selection if present.
    ///
    /// - If the removed key was primary or anchor, those roles are cleared.
    /// - If the selection becomes empty, both primary and anchor are cleared.
    pub fn remove(&mut self, key: &T) {
        if let Some(idx) = self.position_of(key) {
            self.remove_at(idx);
            self.bump_revision();
        }
    }

    /// Toggles `key` in the selection.
    ///
    /// - If `key` is not selected, it is added and becomes the primary key.
    /// - If `key` is already selected, it is removed. If this empties the selection,
    ///   both primary and anchor are cleared.
    pub fn toggle(&mut self, key: T) {
        if let Some(idx) = self.position_of(&key) {
            self.remove_at(idx);
            self.bump_revision();
        } else {
            self.items.push(key);
            self.primary = Some(self.items.len() - 1);
            self.bump_revision();
        }
    }

    /// Sets the primary key to `key` if it is already selected.
    pub fn set_primary(&mut self, key: &T) {
        if let Some(idx) = self.position_of(key)
            && self.primary != Some(idx)
        {
            self.primary = Some(idx);
            self.bump_revision();
        }
    }

    /// Sets the anchor key to `key` if it is already selected.
    pub fn set_anchor(&mut self, key: &T) {
        if let Some(idx) = self.position_of(key)
            && self.anchor != Some(idx)
        {
            self.anchor = Some(idx);
            self.bump_revision();
        }
    }

    /// Clears the anchor while leaving the selection and primary untouched.
    pub fn clear_anchor(&mut self) {
        if self.anchor.is_some() {
            self.anchor = None;
            self.bump_revision();
        }
    }

    /// Returns the position of `key` within the selection, if present.
    fn position_of(&self, key: &T) -> Option<usize> {
        self.items.iter().position(|k| k == key)
    }

    fn replace_with_items(&mut self, new_items: Vec<T>) {
        let new_primary = if new_items.is_empty() { None } else { Some(0) };

        // Preserve the previous anchor if its key is still present in the new set.
        let mut new_anchor = None;
        if let Some(old_anchor_idx) = self.anchor
            && let Some(old_key) = self.items.get(old_anchor_idx)
        {
            new_anchor = new_items.iter().position(|k| k == old_key);
        }
        if new_anchor.is_none() {
            new_anchor = new_primary;
        }

        if new_items == self.items && self.primary == new_primary && self.anchor == new_anchor {
            return;
        }

        self.items = new_items;
        self.primary = new_primary;
        self.anchor = new_anchor;
        self.bump_revision();
    }

    /// Removes the item at `idx`, updating primary and anchor accordingly.
    fn remove_at(&mut self, idx: usize) {
        self.items.remove(idx);

        let update_index = |slot: &mut Option<usize>| {
            if let Some(current) = *slot {
                if current == idx {
                    *slot = None;
                } else if current > idx {
                    *slot = Some(current - 1);
                }
            }
        };

        update_index(&mut self.primary);
        update_index(&mut self.anchor);

        if self.items.is_empty() {
            self.primary = None;
            self.anchor = None;
        }
    }
}

#[cfg(feature = "hashbrown")]
impl<T> Selection<T>
where
    T: core::hash::Hash + Eq,
{
    /// Replaces the current selection with the provided batch of keys, de-duplicating with hashing.
    ///
    /// This is an alternative to [`Selection::replace_with`] for larger inputs when `T` supports
    /// hashing. It preserves first-occurrence order while filtering duplicates.
    ///
    /// This can be a good fit when you frequently build selections from sources that may contain
    /// duplicates (for example, merging multiple streams of keys) and the quadratic behavior of
    /// [`Selection::replace_with`] becomes a bottleneck.
    ///
    /// For “Select All” style operations where you can guarantee uniqueness, prefer
    /// [`Selection::replace_with_unique`].
    pub fn replace_with_hashed<I>(&mut self, keys: I)
    where
        I: IntoIterator<Item = T>,
    {
        use core::hash::BuildHasher;
        use hashbrown::hash_map::Entry;
        use hashbrown::{DefaultHashBuilder, HashMap};

        let iter = keys.into_iter();
        let (lower, upper) = iter.size_hint();
        let cap = upper.unwrap_or(lower);

        let build_hasher = DefaultHashBuilder::default();
        let mut new_items: Vec<T> = Vec::with_capacity(cap);
        let mut seen: HashMap<u64, Bucket, DefaultHashBuilder> =
            HashMap::with_capacity_and_hasher(cap, build_hasher.clone());

        enum Bucket {
            // Most hashes map to a single key; keep this case allocation-free.
            One(usize),
            // Hashes can collide. When multiple distinct keys share the same 64-bit hash, we
            // must track *all* candidate indices and do equality checks against them to avoid
            // incorrectly treating distinct keys as duplicates (or vice versa).
            Many(Vec<usize>),
        }

        for key in iter {
            // We intentionally preserve *first-occurrence* order:
            // - `Selection::items()` is a `Vec<T>` with stable order within an instance.
            // - `primary`/`anchor` default to "first unique item", so order affects semantics.
            // Using a hash set directly would either scramble ordering or require extra
            // bookkeeping; instead we keep a `Vec<T>` for order and a hashed "seen" structure
            // for fast de-duplication.
            let hash = build_hasher.hash_one(&key);

            match seen.entry(hash) {
                Entry::Vacant(entry) => {
                    let idx = new_items.len();
                    new_items.push(key);
                    entry.insert(Bucket::One(idx));
                }
                Entry::Occupied(mut entry) => match entry.get_mut() {
                    Bucket::One(existing_idx) => {
                        if new_items[*existing_idx] == key {
                            continue;
                        }

                        let idx = new_items.len();
                        new_items.push(key);
                        *entry.get_mut() = Bucket::Many(Vec::from([*existing_idx, idx]));
                    }
                    Bucket::Many(existing_idxs) => {
                        if existing_idxs.iter().any(|&idx| new_items[idx] == key) {
                            continue;
                        }

                        let idx = new_items.len();
                        new_items.push(key);
                        existing_idxs.push(idx);
                    }
                },
            }
        }

        self.replace_with_items(new_items);
    }
}

#[cfg(debug_assertions)]
fn debug_assert_unique<T>(items: &[T])
where
    T: PartialEq,
{
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            debug_assert!(
                items[i] != items[j],
                "duplicate selection key at {i} and {j}"
            );
        }
    }
}
