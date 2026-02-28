// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Selector inputs and selector predicates for style matching.
//!
//! This module intentionally starts small: selectors are single-element
//! predicates over a [`SelectorInputs`] snapshot (no combinators).

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::iter::FromIterator;

/// Bucketed selector specificity: `(pseudos, classes, type_tag)`.
///
/// The fields are ordered highest-weight-first so that derived `Ord`
/// gives correct CSS-like lexicographic ordering: pseudoclass count
/// outranks class count, which outranks type tag presence.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Specificity(pub u32, pub u32, pub u32);

/// A stable identifier for an element "type" in selectors.
///
/// This is application-defined (e.g. `Button`, `Text`, `SliderThumb`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeTag(pub u32);

/// A stable identifier for a user-defined class (e.g. `.primary`).
///
/// Class IDs are application-defined and intentionally unbounded.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassId(pub u32);

/// A stable identifier for a pseudoclass (e.g. `:hover`, `:focus`).
///
/// Pseudoclass IDs are application-defined and intentionally unbounded.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PseudoClassId(pub u32);

/// An owned, sorted, deduplicated set of IDs.
///
/// This representation is optimized for "small sets" and unbounded vocabularies:
/// membership is O(log n), subset checks are O(n+m) via merge walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdSet<T>(Box<[T]>);

impl<T> Default for IdSet<T> {
    fn default() -> Self {
        Self(Vec::new().into_boxed_slice())
    }
}

impl<T> IdSet<T>
where
    T: Copy + Ord,
{
    /// Constructs a set from an iterator, sorting and deduplicating.
    #[must_use]
    pub fn from_ids(iter: impl IntoIterator<Item = T>) -> Self {
        let mut ids: Vec<T> = iter.into_iter().collect();
        ids.sort();
        ids.dedup();
        Self(ids.into_boxed_slice())
    }

    /// Returns `true` if the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of IDs in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns the set as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// Returns `true` if this set contains the given ID.
    #[must_use]
    pub fn contains(&self, id: T) -> bool {
        self.0.binary_search(&id).is_ok()
    }

    /// Returns `true` if this set is a subset of `other`.
    #[must_use]
    pub fn is_subset_of_slice(&self, other: &[T]) -> bool {
        is_subset(self.as_slice(), other)
    }
}

impl<T> FromIterator<T> for IdSet<T>
where
    T: Copy + Ord,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::from_ids(iter)
    }
}

/// A borrowed snapshot of selector inputs for a single element.
///
/// The `classes` and `pseudos` slices must be sorted and deduplicated.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SelectorInputs<'a> {
    /// Optional type tag for the element.
    pub type_tag: Option<TypeTag>,
    /// Sorted, unique class IDs.
    pub classes: &'a [ClassId],
    /// Sorted, unique pseudoclass IDs.
    pub pseudos: &'a [PseudoClassId],
}

impl SelectorInputs<'static> {
    /// Empty selector inputs (no type, classes, or pseudos).
    pub const EMPTY: Self = Self {
        type_tag: None,
        classes: &[],
        pseudos: &[],
    };
}

impl<'a> SelectorInputs<'a> {
    /// Constructs selector inputs from borrowed slices.
    ///
    /// # Panics (debug only)
    ///
    /// Panics in debug builds if the slices are not sorted and deduplicated.
    #[must_use]
    pub fn new(
        type_tag: Option<TypeTag>,
        classes: &'a [ClassId],
        pseudos: &'a [PseudoClassId],
    ) -> Self {
        debug_assert!(
            is_sorted_unique(classes),
            "`classes` must be sorted and unique"
        );
        debug_assert!(
            is_sorted_unique(pseudos),
            "`pseudos` must be sorted and unique"
        );
        Self {
            type_tag,
            classes,
            pseudos,
        }
    }
}

/// A selector predicate over [`SelectorInputs`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selector {
    /// Optional type tag predicate.
    pub type_tag: Option<TypeTag>,
    /// Required class IDs.
    pub required_classes: IdSet<ClassId>,
    /// Required pseudoclass IDs.
    pub required_pseudos: IdSet<PseudoClassId>,
}

impl Selector {
    /// Returns `true` if this selector matches the given inputs.
    #[must_use]
    pub fn matches(&self, inputs: &SelectorInputs<'_>) -> bool {
        if let Some(required) = self.type_tag
            && inputs.type_tag != Some(required)
        {
            return false;
        }
        self.required_classes.is_subset_of_slice(inputs.classes)
            && self.required_pseudos.is_subset_of_slice(inputs.pseudos)
    }

    /// Returns a bucketed specificity score.
    ///
    /// The returned [`Specificity`] orders `(pseudos, classes, type_tag)`,
    /// so pseudoclass count outranks class count, which outranks type tag
    /// presence — matching CSS semantics.
    #[must_use]
    pub fn specificity(&self) -> Specificity {
        let type_score = u32::from(self.type_tag.is_some());
        let classes = u32::try_from(self.required_classes.len()).unwrap_or(u32::MAX);
        let pseudos = u32::try_from(self.required_pseudos.len()).unwrap_or(u32::MAX);
        Specificity(pseudos, classes, type_score)
    }
}

fn is_sorted_unique<T: Ord>(slice: &[T]) -> bool {
    slice
        .windows(2)
        .all(|w| w[0].cmp(&w[1]) == core::cmp::Ordering::Less)
}

fn is_subset<T: Ord>(needles: &[T], haystack: &[T]) -> bool {
    if needles.is_empty() {
        return true;
    }
    if haystack.is_empty() {
        return false;
    }

    let mut i = 0;
    let mut j = 0;
    while i < needles.len() && j < haystack.len() {
        match needles[i].cmp(&haystack[j]) {
            core::cmp::Ordering::Less => return false,
            core::cmp::Ordering::Equal => {
                i += 1;
                j += 1;
            }
            core::cmp::Ordering::Greater => j += 1,
        }
    }
    i == needles.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_set_from_iter_sorts_and_dedups() {
        let set = IdSet::from_ids([ClassId(3), ClassId(1), ClassId(3), ClassId(2)]);
        assert_eq!(set.as_slice(), &[ClassId(1), ClassId(2), ClassId(3)]);
        assert!(set.contains(ClassId(2)));
        assert!(!set.contains(ClassId(4)));
    }

    #[test]
    fn selector_matches_subsets() {
        let selector = Selector {
            type_tag: Some(TypeTag(1)),
            required_classes: IdSet::from_ids([ClassId(2), ClassId(3)]),
            required_pseudos: IdSet::from_ids([PseudoClassId(10)]),
        };

        let classes = [ClassId(1), ClassId(2), ClassId(3)];
        let pseudos = [PseudoClassId(10)];
        let inputs = SelectorInputs::new(Some(TypeTag(1)), &classes, &pseudos);
        assert!(selector.matches(&inputs));

        let missing = [ClassId(2)];
        let inputs_missing = SelectorInputs::new(Some(TypeTag(1)), &missing, &pseudos);
        assert!(!selector.matches(&inputs_missing));
    }

    #[test]
    fn selector_specificity_is_stable() {
        let selector = Selector {
            type_tag: Some(TypeTag(1)),
            required_classes: IdSet::from_ids([ClassId(2), ClassId(3)]),
            required_pseudos: IdSet::from_ids([PseudoClassId(10)]),
        };
        assert_eq!(selector.specificity(), Specificity(1, 2, 1));
    }

    #[test]
    fn specificity_class_beats_type() {
        let type_only = Selector {
            type_tag: Some(TypeTag(1)),
            required_classes: IdSet::default(),
            required_pseudos: IdSet::default(),
        };
        let class_only = Selector {
            type_tag: None,
            required_classes: IdSet::from_ids([ClassId(1)]),
            required_pseudos: IdSet::default(),
        };
        assert!(class_only.specificity() > type_only.specificity());
    }
}
