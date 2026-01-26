// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Tests for the `understory_selection` crate.
//!
//! These exercises the core `Selection<T>` API, with a focus on how contents,
//! primary/anchor roles, and the revision counter interact.

use understory_selection::Selection;

#[test]
fn empty_selection_basics() {
    let sel = Selection::<u32>::new();
    assert!(sel.is_empty());
    assert_eq!(sel.len(), 0);
    assert_eq!(sel.primary(), None);
    assert_eq!(sel.anchor(), None);
    assert_eq!(sel.revision(), 0);
}

#[test]
fn select_only_sets_primary_anchor_and_bumps_revision() {
    let mut sel = Selection::new();
    sel.select_only(1);

    assert_eq!(sel.items(), &[1]);
    assert_eq!(sel.primary(), Some(&1));
    assert_eq!(sel.anchor(), Some(&1));
    assert_eq!(sel.revision(), 1);

    // No-op: selecting the same singleton again should not change revision.
    sel.select_only(1);
    assert_eq!(sel.revision(), 1);
}

#[test]
fn clear_empties_and_bumps_revision_only_on_change() {
    let mut sel = Selection::new();
    sel.clear();
    assert_eq!(sel.revision(), 0);

    sel.select_only(1);
    assert_eq!(sel.revision(), 1);

    sel.clear();
    assert!(sel.is_empty());
    assert_eq!(sel.primary(), None);
    assert_eq!(sel.anchor(), None);
    assert_eq!(sel.revision(), 2);
}

#[test]
fn replace_with_dedups_and_preserves_anchor_when_possible() {
    let mut sel = Selection::new();

    sel.replace_with([1, 2, 2, 3]);
    assert_eq!(sel.items(), &[1, 2, 3]);
    assert_eq!(sel.primary(), Some(&1));
    assert_eq!(sel.anchor(), Some(&1));

    // Set anchor explicitly to 2.
    sel.set_anchor(&2);
    let rev_after_anchor = sel.revision();
    assert_eq!(sel.anchor(), Some(&2));

    // Replace with a set that still contains 2: anchor should remain 2.
    sel.replace_with([2, 3, 4]);
    assert_eq!(sel.items(), &[2, 3, 4]);
    assert_eq!(sel.anchor(), Some(&2));
    assert!(sel.revision() > rev_after_anchor);

    // Replace with a set that does not contain the old anchor: anchor falls back to first.
    sel.replace_with([10, 11]);
    assert_eq!(sel.items(), &[10, 11]);
    assert_eq!(sel.primary(), Some(&10));
    assert_eq!(sel.anchor(), Some(&10));
}

#[test]
fn replace_with_unique_matches_replace_with_for_unique_inputs() {
    let mut a = Selection::new();
    let mut b = Selection::new();

    a.replace_with([1, 2, 3]);
    b.replace_with_unique([1, 2, 3]);

    assert_eq!(a.items(), b.items());
    assert_eq!(a.primary(), b.primary());
    assert_eq!(a.anchor(), b.anchor());
    assert_eq!(a.revision(), b.revision());
}

#[cfg(feature = "hashbrown")]
#[test]
fn replace_with_hashed_matches_replace_with_for_mixed_inputs() {
    let mut a = Selection::new();
    let mut b = Selection::new();

    a.replace_with([1_u32, 2, 2, 3, 1]);
    b.replace_with_hashed([1_u32, 2, 2, 3, 1]);

    assert_eq!(a.items(), b.items());
    assert_eq!(a.primary(), b.primary());
    assert_eq!(a.anchor(), b.anchor());
    assert_eq!(a.revision(), b.revision());
}

#[test]
fn extend_with_adds_items_and_does_not_move_anchor() {
    let mut sel = Selection::new();
    sel.replace_with([1, 2]);
    sel.set_anchor(&1);
    let rev_before = sel.revision();

    sel.extend_with([2, 3, 3, 4]);
    assert_eq!(sel.items(), &[1, 2, 3, 4]);
    assert_eq!(sel.anchor(), Some(&1));
    assert!(sel.revision() > rev_before);
}

#[test]
fn add_and_remove_update_primary_and_revision() {
    let mut sel = Selection::new();
    sel.add(1);
    sel.add(2);
    assert_eq!(sel.items(), &[1, 2]);
    assert_eq!(sel.primary(), Some(&2));

    let rev_before = sel.revision();
    // Adding an already-selected key should only move primary.
    sel.add(1);
    assert_eq!(sel.primary(), Some(&1));
    assert!(sel.revision() > rev_before);

    // Removing a non-existent key is a no-op.
    let rev_before_remove = sel.revision();
    sel.remove(&99);
    assert_eq!(sel.revision(), rev_before_remove);

    // Removing an existing key updates contents and revision.
    sel.remove(&1);
    assert_eq!(sel.items(), &[2]);
    assert!(sel.revision() > rev_before_remove);
}

#[test]
fn toggle_adds_and_removes_with_revision() {
    let mut sel = Selection::new();

    sel.toggle(1);
    assert_eq!(sel.items(), &[1]);
    assert_eq!(sel.primary(), Some(&1));
    let rev_after_add = sel.revision();

    sel.toggle(1);
    assert!(sel.items().is_empty());
    assert!(sel.primary().is_none());
    assert!(sel.anchor().is_none());
    assert!(sel.revision() > rev_after_add);
}

#[test]
fn set_primary_and_anchor_are_noops_when_unchanged() {
    let mut sel = Selection::new();
    sel.replace_with([1, 2, 3]);
    sel.set_primary(&2);
    sel.set_anchor(&1);
    let rev_after_init = sel.revision();

    // Setting to the same values should not bump revision.
    sel.set_primary(&2);
    sel.set_anchor(&1);
    assert_eq!(sel.revision(), rev_after_init);
}

#[test]
fn clear_anchor_only_changes_when_anchor_is_some() {
    let mut sel = Selection::new();
    sel.replace_with([1, 2]);
    sel.set_anchor(&2);
    let rev_with_anchor = sel.revision();

    sel.clear_anchor();
    assert!(sel.anchor().is_none());
    assert!(sel.revision() > rev_with_anchor);

    let rev_without_anchor = sel.revision();
    sel.clear_anchor();
    assert_eq!(sel.revision(), rev_without_anchor);
}
