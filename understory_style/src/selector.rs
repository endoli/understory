// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Selector inputs and selector predicates for style matching.
//!
//! Selectors match an ordered root-to-subject path of steps. Each
//! [`SelectorStep`] is matched against one [`SelectorInputs`] snapshot.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::iter::FromIterator;

/// Bucketed selector specificity: `(pseudos, classes, target_tag, type_tag)`.
///
/// The fields are ordered highest-weight-first so that derived `Ord`
/// gives correct CSS-like lexicographic ordering: pseudoclass count
/// outranks class count, which outranks target tag presence, which outranks
/// type tag presence.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Specificity(pub u32, pub u32, pub u32, pub u32);

/// A stable identifier for an element "type" in selectors.
///
/// This is application-defined (e.g. `Button`, `Text`, `SliderThumb`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeTag(pub u32);

/// A stable identifier for a style target within an element.
///
/// This is application-defined. UI layers can use it for sub-targets such as
/// `Button::icon`, `Toggle::track`, or `Slider::thumb`; non-UI embedders can
/// use it for any owner-local target they want to style.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetTag(pub u32);

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
    /// Optional target tag within the element.
    pub target_tag: Option<TargetTag>,
    /// Sorted, unique class IDs.
    pub classes: &'a [ClassId],
    /// Sorted, unique pseudoclass IDs.
    pub pseudos: &'a [PseudoClassId],
}

impl SelectorInputs<'static> {
    /// Empty selector inputs (no type, classes, or pseudos).
    pub const EMPTY: Self = Self {
        type_tag: None,
        target_tag: None,
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
        Self::with_target(type_tag, None, classes, pseudos)
    }

    /// Constructs selector inputs with an owner-local target tag.
    ///
    /// # Panics (debug only)
    ///
    /// Panics in debug builds if the slices are not sorted and deduplicated.
    #[must_use]
    pub fn with_target(
        type_tag: Option<TypeTag>,
        target_tag: Option<TargetTag>,
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
            target_tag,
            classes,
            pseudos,
        }
    }
}

/// One selector step in a root-to-subject path.
///
/// A step is matched against one [`SelectorInputs`] snapshot. It can be
/// composed with other steps in a [`Selector`] to express owner-to-part
/// paths.
///
/// ```rust
/// use understory_style::{
///     PseudoClassId, Selector, SelectorStep, TargetTag, TypeTag,
/// };
///
/// const TOGGLE: TypeTag = TypeTag(1);
/// const TRACK: TargetTag = TargetTag(2);
/// const CHECKED: PseudoClassId = PseudoClassId(3);
///
/// let selector = Selector::from([
///     SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
///     SelectorStep::target_tag(TRACK),
/// ]);
/// assert_eq!(selector.len(), 2);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectorStep {
    /// Optional type tag predicate.
    pub type_tag: Option<TypeTag>,
    /// Optional target tag predicate.
    pub target_tag: Option<TargetTag>,
    /// Required class IDs.
    pub required_classes: IdSet<ClassId>,
    /// Required pseudoclass IDs.
    pub required_pseudos: IdSet<PseudoClassId>,
}

impl SelectorStep {
    /// Constructs a step that requires the given type tag.
    #[must_use]
    pub fn type_tag(type_tag: TypeTag) -> Self {
        Self {
            type_tag: Some(type_tag),
            ..Self::default()
        }
    }

    /// Constructs a step that requires the given target tag.
    #[must_use]
    pub fn target_tag(target_tag: TargetTag) -> Self {
        Self {
            target_tag: Some(target_tag),
            ..Self::default()
        }
    }

    /// Constructs a step that requires the given class.
    #[must_use]
    pub fn class(class_id: ClassId) -> Self {
        Self {
            required_classes: IdSet::from_ids([class_id]),
            ..Self::default()
        }
    }

    /// Constructs a step that requires the given pseudoclass.
    #[must_use]
    pub fn pseudo(pseudo_id: PseudoClassId) -> Self {
        Self {
            required_pseudos: IdSet::from_ids([pseudo_id]),
            ..Self::default()
        }
    }

    /// Adds or replaces the type tag requirement for this step.
    #[must_use]
    pub fn with_type_tag(mut self, type_tag: TypeTag) -> Self {
        self.type_tag = Some(type_tag);
        self
    }

    /// Adds or replaces the target tag requirement for this step.
    #[must_use]
    pub fn with_target_tag(mut self, target_tag: TargetTag) -> Self {
        self.target_tag = Some(target_tag);
        self
    }

    /// Adds a required class to this step.
    #[must_use]
    pub fn with_class(self, class_id: ClassId) -> Self {
        self.with_classes([class_id])
    }

    /// Adds required classes to this step.
    #[must_use]
    pub fn with_classes(mut self, class_ids: impl IntoIterator<Item = ClassId>) -> Self {
        let mut required_classes = self.required_classes.as_slice().to_vec();
        required_classes.extend(class_ids);
        self.required_classes = IdSet::from_ids(required_classes);
        self
    }

    /// Adds a required pseudoclass to this step.
    #[must_use]
    pub fn with_pseudo(self, pseudo_id: PseudoClassId) -> Self {
        self.with_pseudos([pseudo_id])
    }

    /// Adds required pseudoclasses to this step.
    #[must_use]
    pub fn with_pseudos(mut self, pseudo_ids: impl IntoIterator<Item = PseudoClassId>) -> Self {
        let mut required_pseudos = self.required_pseudos.as_slice().to_vec();
        required_pseudos.extend(pseudo_ids);
        self.required_pseudos = IdSet::from_ids(required_pseudos);
        self
    }

    /// Returns `true` if this step matches the given subject inputs.
    #[must_use]
    pub fn matches(&self, inputs: &SelectorInputs<'_>) -> bool {
        if let Some(required) = self.type_tag
            && inputs.type_tag != Some(required)
        {
            return false;
        }
        if let Some(required) = self.target_tag
            && inputs.target_tag != Some(required)
        {
            return false;
        }
        self.required_classes.is_subset_of_slice(inputs.classes)
            && self.required_pseudos.is_subset_of_slice(inputs.pseudos)
    }

    /// Returns a bucketed specificity score.
    ///
    /// The returned [`Specificity`] orders `(pseudos, classes, target_tag, type_tag)`,
    /// so pseudoclass count outranks class count, which outranks target tag
    /// presence, which outranks type tag presence.
    #[must_use]
    pub fn specificity(&self) -> Specificity {
        let type_score = u32::from(self.type_tag.is_some());
        let target_score = u32::from(self.target_tag.is_some());
        let classes = u32::try_from(self.required_classes.len()).unwrap_or(u32::MAX);
        let pseudos = u32::try_from(self.required_pseudos.len()).unwrap_or(u32::MAX);
        Specificity(pseudos, classes, target_score, type_score)
    }
}

/// A declarative selector over an exact root-to-subject path.
///
/// The first implementation deliberately supports exact child paths only:
/// every step corresponds to one entered subject. Descendant, sibling, and
/// child-index selectors are intentionally out of scope for this type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    steps: Box<[SelectorStep]>,
}

impl Selector {
    /// Constructs a selector from ordered root-to-subject steps.
    ///
    /// Empty path selectors are allowed as data, but they never match a
    /// non-empty subject path.
    #[must_use]
    pub fn from_steps(steps: impl IntoIterator<Item = SelectorStep>) -> Self {
        let steps: Vec<SelectorStep> = steps.into_iter().collect();
        Self {
            steps: steps.into_boxed_slice(),
        }
    }

    /// Constructs a single-subject path selector.
    #[must_use]
    pub fn single(step: SelectorStep) -> Self {
        Self {
            steps: Box::new([step]),
        }
    }

    /// Returns the ordered steps in this path selector.
    #[must_use]
    pub fn steps(&self) -> &[SelectorStep] {
        &self.steps
    }

    /// Returns the number of steps in this path selector.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Returns `true` if this path selector has no steps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Returns `true` if this selector exactly matches the subject path.
    #[must_use]
    pub fn matches_path(&self, path: &[SelectorInputs<'_>]) -> bool {
        self.steps.len() == path.len()
            && self
                .steps
                .iter()
                .zip(path.iter())
                .all(|(step, inputs)| step.matches(inputs))
    }

    /// Returns the aggregate specificity for this selector path.
    ///
    /// The current exact-path grammar sums each step's contribution using
    /// saturating arithmetic. This keeps ordering deterministic without
    /// importing broader browser CSS semantics.
    #[must_use]
    pub fn specificity(&self) -> Specificity {
        self.steps
            .iter()
            .map(SelectorStep::specificity)
            .fold(Specificity::default(), add_specificity)
    }
}

impl From<SelectorStep> for Selector {
    fn from(step: SelectorStep) -> Self {
        Self::single(step)
    }
}

impl<const N: usize> From<[SelectorStep; N]> for Selector {
    fn from(steps: [SelectorStep; N]) -> Self {
        Self::from_steps(steps)
    }
}

impl From<&[SelectorStep]> for Selector {
    fn from(steps: &[SelectorStep]) -> Self {
        Self::from_steps(steps.iter().cloned())
    }
}

fn add_specificity(left: Specificity, right: Specificity) -> Specificity {
    Specificity(
        left.0.saturating_add(right.0),
        left.1.saturating_add(right.1),
        left.2.saturating_add(right.2),
        left.3.saturating_add(right.3),
    )
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
        let selector = SelectorStep {
            type_tag: Some(TypeTag(1)),
            target_tag: None,
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
        let selector = SelectorStep {
            type_tag: Some(TypeTag(1)),
            target_tag: Some(TargetTag(7)),
            required_classes: IdSet::from_ids([ClassId(2), ClassId(3)]),
            required_pseudos: IdSet::from_ids([PseudoClassId(10)]),
        };
        assert_eq!(selector.specificity(), Specificity(1, 2, 1, 1));
    }

    #[test]
    fn selector_step_builders_compose_predicates() {
        let selector = SelectorStep::type_tag(TypeTag(1))
            .with_target_tag(TargetTag(2))
            .with_classes([ClassId(3), ClassId(4), ClassId(3)])
            .with_pseudo(PseudoClassId(5));

        let classes = [ClassId(3), ClassId(4)];
        let pseudos = [PseudoClassId(5)];
        let inputs =
            SelectorInputs::with_target(Some(TypeTag(1)), Some(TargetTag(2)), &classes, &pseudos);

        assert!(selector.matches(&inputs));
        assert_eq!(
            selector.required_classes.as_slice(),
            &[ClassId(3), ClassId(4)]
        );
        assert_eq!(selector.required_pseudos.as_slice(), &[PseudoClassId(5)]);
    }

    #[test]
    fn specificity_class_beats_type() {
        let type_only = SelectorStep {
            type_tag: Some(TypeTag(1)),
            target_tag: None,
            required_classes: IdSet::default(),
            required_pseudos: IdSet::default(),
        };
        let class_only = SelectorStep {
            type_tag: None,
            target_tag: None,
            required_classes: IdSet::from_ids([ClassId(1)]),
            required_pseudos: IdSet::default(),
        };
        assert!(class_only.specificity() > type_only.specificity());
    }

    #[test]
    fn selector_matches_optional_target() {
        let selector = SelectorStep {
            type_tag: Some(TypeTag(1)),
            target_tag: Some(TargetTag(2)),
            required_classes: IdSet::default(),
            required_pseudos: IdSet::default(),
        };

        let matched = SelectorInputs::with_target(Some(TypeTag(1)), Some(TargetTag(2)), &[], &[]);
        let wrong_target =
            SelectorInputs::with_target(Some(TypeTag(1)), Some(TargetTag(3)), &[], &[]);
        let no_target = SelectorInputs::new(Some(TypeTag(1)), &[], &[]);

        assert!(selector.matches(&matched));
        assert!(!selector.matches(&wrong_target));
        assert!(!selector.matches(&no_target));
    }

    #[test]
    fn specificity_target_beats_type_but_not_class() {
        let type_only = SelectorStep {
            type_tag: Some(TypeTag(1)),
            target_tag: None,
            required_classes: IdSet::default(),
            required_pseudos: IdSet::default(),
        };
        let target_only = SelectorStep {
            type_tag: None,
            target_tag: Some(TargetTag(1)),
            required_classes: IdSet::default(),
            required_pseudos: IdSet::default(),
        };
        let class_only = SelectorStep {
            type_tag: None,
            target_tag: None,
            required_classes: IdSet::from_ids([ClassId(1)]),
            required_pseudos: IdSet::default(),
        };

        assert!(target_only.specificity() > type_only.specificity());
        assert!(class_only.specificity() > target_only.specificity());
    }

    #[test]
    fn step_selector_builds_single_step_path() {
        let step = SelectorStep {
            type_tag: Some(TypeTag(1)),
            target_tag: Some(TargetTag(2)),
            required_classes: IdSet::from_ids([ClassId(3)]),
            required_pseudos: IdSet::from_ids([PseudoClassId(4)]),
        };

        let classes = [ClassId(3)];
        let pseudos = [PseudoClassId(4)];
        let inputs =
            SelectorInputs::with_target(Some(TypeTag(1)), Some(TargetTag(2)), &classes, &pseudos);
        let path = Selector::from(step.clone());

        assert_eq!(path.len(), 1);
        assert_eq!(path.specificity(), step.specificity());
        assert!(path.matches_path(&[inputs]));
    }

    #[test]
    fn selector_converts_from_step_array_and_slice() {
        let steps = [
            SelectorStep::type_tag(TypeTag(1)),
            SelectorStep::target_tag(TargetTag(2)),
        ];

        let from_array = Selector::from(steps.clone());
        let from_slice = Selector::from(steps.as_slice());

        assert_eq!(from_array.steps(), from_slice.steps());
        assert_eq!(from_array.len(), 2);
    }

    #[test]
    fn path_selector_matches_nested_target_path() {
        const TOGGLE: TypeTag = TypeTag(1);
        const TRACK: TargetTag = TargetTag(10);
        const THUMB: TargetTag = TargetTag(11);

        let selector = Selector::from([
            SelectorStep::type_tag(TOGGLE),
            SelectorStep::target_tag(TRACK),
            SelectorStep::target_tag(THUMB),
        ]);

        let root = SelectorInputs::new(Some(TOGGLE), &[], &[]);
        let track = SelectorInputs::with_target(None, Some(TRACK), &[], &[]);
        let thumb = SelectorInputs::with_target(None, Some(THUMB), &[], &[]);
        let direct_thumb = SelectorInputs::with_target(None, Some(THUMB), &[], &[]);

        assert!(selector.matches_path(&[root, track, thumb]));
        assert!(!selector.matches_path(&[root, direct_thumb]));
    }

    #[test]
    fn owner_pseudos_match_through_ancestor_step() {
        const TOGGLE: TypeTag = TypeTag(1);
        const TRACK: TargetTag = TargetTag(10);
        const CHECKED: PseudoClassId = PseudoClassId(20);

        let owner_checked_track = Selector::from([
            SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
            SelectorStep::target_tag(TRACK),
        ]);
        let track_checked = Selector::from([
            SelectorStep::type_tag(TOGGLE),
            SelectorStep::target_tag(TRACK).with_pseudo(CHECKED),
        ]);

        let pseudos = [CHECKED];
        let root = SelectorInputs::new(Some(TOGGLE), &[], &pseudos);
        let track = SelectorInputs::with_target(None, Some(TRACK), &[], &[]);

        assert!(owner_checked_track.matches_path(&[root, track]));
        assert!(!track_checked.matches_path(&[root, track]));
    }

    #[test]
    fn path_specificity_sums_steps_deterministically() {
        let selector = Selector::from([
            SelectorStep::type_tag(TypeTag(1)).with_pseudo(PseudoClassId(1)),
            SelectorStep::target_tag(TargetTag(2)).with_classes([ClassId(1), ClassId(2)]),
        ]);

        assert_eq!(selector.specificity(), Specificity(1, 2, 1, 1));
    }
}
