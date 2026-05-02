// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Selector inputs and selector predicates for style matching.
//!
//! Selectors match an ordered root-to-subject path of steps. Each
//! [`SelectorStep`] is matched against one [`SelectorInputs`] snapshot, with
//! [`SelectorCombinator`] values describing how adjacent steps relate.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::iter::FromIterator;

/// Bucketed selector specificity: `(pseudos, classes, part_tag, type_tag)`.
///
/// The fields are ordered highest-weight-first so that derived `Ord`
/// gives correct CSS-like lexicographic ordering: pseudoclass count
/// outranks class count, which outranks part tag presence, which outranks
/// type tag presence.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Specificity(pub u32, pub u32, pub u32, pub u32);

/// A stable identifier for an element "type" in selectors.
///
/// This is application-defined (e.g. `Button`, `Text`, `SliderThumb`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeTag(pub u32);

/// A stable identifier for an embedder-defined style subject.
///
/// This is application-defined. UI layers can use it for element parts such as
/// `Button::icon`, `Toggle::track`, or `Slider::thumb`; non-UI embedders can
/// use it for any addressable subject they want to style. The name is
/// intentionally not tied to any particular widget, slot, or template system.
///
/// `PartTag` values are not globally namespaced by this crate. Embedders should
/// usually anchor part selectors under an owner [`TypeTag`] so unrelated
/// widgets can reuse local part IDs without accidentally matching each other.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartTag(pub u32);

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
    /// Optional part tag within the element.
    pub part_tag: Option<PartTag>,
    /// Sorted, unique class IDs.
    pub classes: &'a [ClassId],
    /// Sorted, unique pseudoclass IDs.
    pub pseudos: &'a [PseudoClassId],
}

impl SelectorInputs<'static> {
    /// Empty selector inputs (no type, classes, or pseudos).
    pub const EMPTY: Self = Self {
        type_tag: None,
        part_tag: None,
        classes: &[],
        pseudos: &[],
    };

    /// Constructs selector inputs with only a type tag.
    #[must_use]
    pub const fn typed(type_tag: TypeTag) -> Self {
        Self {
            type_tag: Some(type_tag),
            part_tag: None,
            classes: &[],
            pseudos: &[],
        }
    }

    /// Constructs selector inputs with only an owner-local part tag.
    #[must_use]
    pub const fn part(part_tag: PartTag) -> Self {
        Self {
            type_tag: None,
            part_tag: Some(part_tag),
            classes: &[],
            pseudos: &[],
        }
    }

    /// Constructs selector inputs with a type tag and owner-local part tag.
    #[must_use]
    pub const fn typed_part(type_tag: TypeTag, part_tag: PartTag) -> Self {
        Self {
            type_tag: Some(type_tag),
            part_tag: Some(part_tag),
            classes: &[],
            pseudos: &[],
        }
    }
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
        Self::with_part(type_tag, None, classes, pseudos)
    }

    /// Constructs selector inputs with a type tag and pseudoclasses.
    ///
    /// This is the common shape for owner state such as `Toggle:checked`.
    ///
    /// # Panics (debug only)
    ///
    /// Panics in debug builds if `pseudos` is not sorted and deduplicated.
    #[must_use]
    pub fn typed_with_pseudos(type_tag: TypeTag, pseudos: &'a [PseudoClassId]) -> Self {
        Self::with_part(Some(type_tag), None, &[], pseudos)
    }

    /// Constructs selector inputs with an owner-local part tag.
    ///
    /// # Panics (debug only)
    ///
    /// Panics in debug builds if the slices are not sorted and deduplicated.
    #[must_use]
    pub fn with_part(
        type_tag: Option<TypeTag>,
        part_tag: Option<PartTag>,
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
            part_tag,
            classes,
            pseudos,
        }
    }
}

/// An owned selector-input snapshot with sorted, deduplicated class and pseudo sets.
///
/// Use this when inputs are assembled from unsorted application data or need to
/// be stored before matching. Borrowed [`SelectorInputs`] remains the cheapest
/// call-site shape when the embedder already owns sorted stable slices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectorInputsOwned {
    type_tag: Option<TypeTag>,
    part_tag: Option<PartTag>,
    classes: Box<[ClassId]>,
    pseudos: Box<[PseudoClassId]>,
}

impl SelectorInputsOwned {
    /// Constructs owned selector inputs without a part tag.
    #[must_use]
    pub fn new(
        type_tag: Option<TypeTag>,
        classes: impl IntoIterator<Item = ClassId>,
        pseudos: impl IntoIterator<Item = PseudoClassId>,
    ) -> Self {
        Self::with_part(type_tag, None, classes, pseudos)
    }

    /// Constructs owned selector inputs with an owner-local part tag.
    #[must_use]
    pub fn with_part(
        type_tag: Option<TypeTag>,
        part_tag: Option<PartTag>,
        classes: impl IntoIterator<Item = ClassId>,
        pseudos: impl IntoIterator<Item = PseudoClassId>,
    ) -> Self {
        Self {
            type_tag,
            part_tag,
            classes: IdSet::from_ids(classes).0,
            pseudos: IdSet::from_ids(pseudos).0,
        }
    }

    /// Borrows this owned snapshot as selector inputs.
    #[must_use]
    pub fn as_inputs(&self) -> SelectorInputs<'_> {
        SelectorInputs::with_part(self.type_tag, self.part_tag, &self.classes, &self.pseudos)
    }

    /// Returns the optional type tag.
    #[must_use]
    pub fn type_tag(&self) -> Option<TypeTag> {
        self.type_tag
    }

    /// Returns the optional part tag.
    #[must_use]
    pub fn part_tag(&self) -> Option<PartTag> {
        self.part_tag
    }

    /// Returns sorted, deduplicated class IDs.
    #[must_use]
    pub fn classes(&self) -> &[ClassId] {
        &self.classes
    }

    /// Returns sorted, deduplicated pseudoclass IDs.
    #[must_use]
    pub fn pseudos(&self) -> &[PseudoClassId] {
        &self.pseudos
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
///     PseudoClassId, Selector, SelectorStep, PartTag, TypeTag,
/// };
///
/// const TOGGLE: TypeTag = TypeTag(1);
/// const TRACK: PartTag = PartTag(2);
/// const CHECKED: PseudoClassId = PseudoClassId(3);
///
/// let selector = Selector::from([
///     SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
///     SelectorStep::part_tag(TRACK),
/// ]);
/// assert_eq!(selector.len(), 2);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectorStep {
    /// Optional type tag predicate.
    pub type_tag: Option<TypeTag>,
    /// Optional part tag predicate.
    pub part_tag: Option<PartTag>,
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

    /// Constructs a step that requires the given part tag.
    #[must_use]
    pub fn part_tag(part_tag: PartTag) -> Self {
        Self {
            part_tag: Some(part_tag),
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

    /// Adds or replaces the part tag requirement for this step.
    #[must_use]
    pub fn with_part_tag(mut self, part_tag: PartTag) -> Self {
        self.part_tag = Some(part_tag);
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
        if let Some(required) = self.part_tag
            && inputs.part_tag != Some(required)
        {
            return false;
        }
        self.required_classes.is_subset_of_slice(inputs.classes)
            && self.required_pseudos.is_subset_of_slice(inputs.pseudos)
    }

    /// Returns a bucketed specificity score.
    ///
    /// The returned [`Specificity`] orders `(pseudos, classes, part_tag, type_tag)`,
    /// so pseudoclass count outranks class count, which outranks part tag
    /// presence, which outranks type tag presence.
    #[must_use]
    pub fn specificity(&self) -> Specificity {
        let type_score = u32::from(self.type_tag.is_some());
        let part_score = u32::from(self.part_tag.is_some());
        let classes = u32::try_from(self.required_classes.len()).unwrap_or(u32::MAX);
        let pseudos = u32::try_from(self.required_pseudos.len()).unwrap_or(u32::MAX);
        Specificity(pseudos, classes, part_score, type_score)
    }
}

/// Relationship between adjacent selector steps.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum SelectorCombinator {
    /// The next step must match the immediate child subject.
    #[default]
    Child,
    /// The next step may match any later descendant subject below the previous step.
    Descendant,
}

/// Why a selector did not match a subject path.
///
/// This is a local diagnostic for the current child/descendant path grammar. It
/// reports the first point where the matcher could not continue; it is not a
/// global proof that no alternate browser-CSS-style match exists.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SelectorMismatch {
    /// The selector has no steps but the subject path is not empty.
    EmptySelectorWithNonEmptyPath,
    /// The subject path ended before `step_index` could be matched.
    MissingSubject {
        /// Selector step that needed a subject.
        step_index: usize,
        /// Path index that was expected to exist.
        path_index: usize,
    },
    /// A selector step did not match the subject inputs at the path index.
    StepMismatch {
        /// Selector step that failed.
        step_index: usize,
        /// Subject path index that failed.
        path_index: usize,
    },
    /// A complete selector matched before the subject path ended.
    TrailingSubjects {
        /// Number of selector steps consumed.
        step_count: usize,
        /// First trailing subject path index.
        path_index: usize,
    },
    /// A descendant combinator could not find a later subject matching the next step.
    MissingDescendant {
        /// Selector step searched for below the previous matched step.
        step_index: usize,
        /// First descendant path index that was searched.
        after_path_index: usize,
    },
}

/// A declarative selector over a root-to-subject path.
///
/// Plain step paths use [`SelectorCombinator::Child`] between every step, so
/// existing `Selector::from([step_a, step_b])` calls remain exact child paths.
/// Use [`Selector::from_segments`] or [`Selector::from_steps_with_combinators`]
/// when a later step should match through a descendant relationship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    steps: Box<[SelectorStep]>,
    combinators: Box<[SelectorCombinator]>,
}

/// Builder for mixed child/descendant selector paths.
///
/// This is intentionally small: it only expresses the current path grammar and
/// does not add sibling, nth, or parent-query semantics.
#[derive(Clone, Debug)]
pub struct SelectorBuilder {
    steps: Vec<SelectorStep>,
    combinators: Vec<SelectorCombinator>,
}

impl SelectorBuilder {
    /// Starts a selector with its first step.
    #[must_use]
    pub fn new(first: SelectorStep) -> Self {
        Self {
            steps: Vec::from([first]),
            combinators: Vec::new(),
        }
    }

    /// Appends an immediate child step.
    #[must_use]
    pub fn child(mut self, step: SelectorStep) -> Self {
        self.combinators.push(SelectorCombinator::Child);
        self.steps.push(step);
        self
    }

    /// Appends a descendant step.
    #[must_use]
    pub fn descendant(mut self, step: SelectorStep) -> Self {
        self.combinators.push(SelectorCombinator::Descendant);
        self.steps.push(step);
        self
    }

    /// Builds the selector.
    #[must_use]
    pub fn build(self) -> Selector {
        Selector {
            steps: self.steps.into_boxed_slice(),
            combinators: self.combinators.into_boxed_slice(),
        }
    }
}

impl Selector {
    /// Constructs a selector from ordered root-to-subject steps.
    ///
    /// Empty path selectors are allowed as data, but they never match a
    /// non-empty subject path.
    #[must_use]
    pub fn from_steps(steps: impl IntoIterator<Item = SelectorStep>) -> Self {
        let steps: Vec<SelectorStep> = steps.into_iter().collect();
        let combinators = child_combinators(steps.len());
        Self {
            steps: steps.into_boxed_slice(),
            combinators,
        }
    }

    /// Constructs an exact child-path selector from ordered steps.
    ///
    /// This is an alias for [`Selector::from_steps`] with a shorter
    /// author-facing name.
    #[must_use]
    pub fn path(steps: impl IntoIterator<Item = SelectorStep>) -> Self {
        Self::from_steps(steps)
    }

    /// Constructs a single-subject path selector.
    #[must_use]
    pub fn single(step: SelectorStep) -> Self {
        Self {
            steps: Box::new([step]),
            combinators: Box::default(),
        }
    }

    /// Constructs a two-step child selector.
    #[must_use]
    pub fn child(parent: SelectorStep, child: SelectorStep) -> Self {
        Self::from_steps([parent, child])
    }

    /// Constructs a selector from steps and explicit adjacent combinators.
    ///
    /// `combinators.len()` must be one less than `steps.len()`, except that an
    /// empty selector must have no combinators.
    ///
    /// # Panics
    ///
    /// Panics if the number of combinators does not match the number of
    /// adjacent step pairs.
    #[must_use]
    pub fn from_steps_with_combinators(
        steps: impl IntoIterator<Item = SelectorStep>,
        combinators: impl IntoIterator<Item = SelectorCombinator>,
    ) -> Self {
        let steps: Vec<SelectorStep> = steps.into_iter().collect();
        let combinators: Vec<SelectorCombinator> = combinators.into_iter().collect();
        assert_eq!(
            combinators.len(),
            steps.len().saturating_sub(1),
            "selector combinator count must be one less than step count"
        );
        Self {
            steps: steps.into_boxed_slice(),
            combinators: combinators.into_boxed_slice(),
        }
    }

    /// Constructs a selector from a first step and combinator-prefixed tail.
    ///
    /// This is the most readable constructor for mixed child and descendant
    /// paths:
    ///
    /// ```rust
    /// use understory_style::{
    ///     Selector, SelectorCombinator, SelectorStep, PartTag, TypeTag,
    /// };
    ///
    /// const TOGGLE: TypeTag = TypeTag(1);
    /// const THUMB: PartTag = PartTag(2);
    ///
    /// let selector = Selector::from_segments(
    ///     SelectorStep::type_tag(TOGGLE),
    ///     [(SelectorCombinator::Descendant, SelectorStep::part_tag(THUMB))],
    /// );
    /// assert_eq!(selector.combinators(), &[SelectorCombinator::Descendant]);
    /// ```
    #[must_use]
    pub fn from_segments(
        first: SelectorStep,
        tail: impl IntoIterator<Item = (SelectorCombinator, SelectorStep)>,
    ) -> Self {
        let mut steps = Vec::from([first]);
        let mut combinators = Vec::new();
        for (combinator, step) in tail {
            combinators.push(combinator);
            steps.push(step);
        }
        Self {
            steps: steps.into_boxed_slice(),
            combinators: combinators.into_boxed_slice(),
        }
    }

    /// Constructs a two-step descendant selector.
    ///
    /// This is a convenience for the common "owner styles any matching nested
    /// part" case. Use [`Selector::from_segments`] when the selector needs
    /// more than two steps or a mix of child and descendant combinators.
    ///
    /// ```rust
    /// use understory_style::{PartTag, Selector, SelectorCombinator, SelectorStep, TypeTag};
    ///
    /// const ROW: TypeTag = TypeTag(1);
    /// const TEXT: PartTag = PartTag(2);
    ///
    /// let selector = Selector::descendant(
    ///     SelectorStep::type_tag(ROW),
    ///     SelectorStep::part_tag(TEXT),
    /// );
    /// assert_eq!(selector.combinators(), &[SelectorCombinator::Descendant]);
    /// ```
    #[must_use]
    pub fn descendant(ancestor: SelectorStep, descendant: SelectorStep) -> Self {
        Self::from_segments(ancestor, [(SelectorCombinator::Descendant, descendant)])
    }

    /// Starts a mixed child/descendant selector builder.
    #[must_use]
    pub fn builder(first: SelectorStep) -> SelectorBuilder {
        SelectorBuilder::new(first)
    }

    /// Returns the ordered steps in this path selector.
    #[must_use]
    pub fn steps(&self) -> &[SelectorStep] {
        &self.steps
    }

    /// Returns the combinators between adjacent selector steps.
    ///
    /// The combinator at index `i` relates `steps()[i]` to `steps()[i + 1]`.
    #[must_use]
    pub fn combinators(&self) -> &[SelectorCombinator] {
        &self.combinators
    }

    /// Returns the combinator that relates the previous step to `step_index`.
    ///
    /// Returns `None` for the first step because it has no predecessor.
    #[must_use]
    pub fn combinator_before(&self, step_index: usize) -> Option<SelectorCombinator> {
        step_index
            .checked_sub(1)
            .and_then(|index| self.combinators.get(index).copied())
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

    /// Returns `true` if this selector matches the subject path.
    ///
    /// Child combinators require adjacent path entries. Descendant combinators
    /// may match any later path entry below the previous step.
    #[must_use]
    pub fn matches_path(&self, path: &[SelectorInputs<'_>]) -> bool {
        if self.steps.is_empty() {
            return path.is_empty();
        }
        self.matches_path_from(0, 0, path)
    }

    /// Explains why this selector does not match a subject path.
    ///
    /// Returns `Ok(())` when [`Selector::matches_path`] would return `true`.
    pub fn diagnose_path(&self, path: &[SelectorInputs<'_>]) -> Result<(), SelectorMismatch> {
        if self.steps.is_empty() {
            return if path.is_empty() {
                Ok(())
            } else {
                Err(SelectorMismatch::EmptySelectorWithNonEmptyPath)
            };
        }
        self.diagnose_path_from(0, 0, path)
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

    fn matches_path_from(
        &self,
        step_index: usize,
        path_index: usize,
        path: &[SelectorInputs<'_>],
    ) -> bool {
        let Some(step) = self.steps.get(step_index) else {
            return true;
        };
        let Some(inputs) = path.get(path_index) else {
            return false;
        };
        if !step.matches(inputs) {
            return false;
        }

        let next_step = step_index + 1;
        if next_step == self.steps.len() {
            return path_index + 1 == path.len();
        }

        match self.combinators[step_index] {
            SelectorCombinator::Child => self.matches_path_from(next_step, path_index + 1, path),
            SelectorCombinator::Descendant => (path_index + 1..path.len())
                .any(|index| self.matches_path_from(next_step, index, path)),
        }
    }

    fn diagnose_path_from(
        &self,
        step_index: usize,
        path_index: usize,
        path: &[SelectorInputs<'_>],
    ) -> Result<(), SelectorMismatch> {
        let Some(step) = self.steps.get(step_index) else {
            return Ok(());
        };
        let Some(inputs) = path.get(path_index) else {
            return Err(SelectorMismatch::MissingSubject {
                step_index,
                path_index,
            });
        };
        if !step.matches(inputs) {
            return Err(SelectorMismatch::StepMismatch {
                step_index,
                path_index,
            });
        }

        let next_step = step_index + 1;
        if next_step == self.steps.len() {
            return if path_index + 1 == path.len() {
                Ok(())
            } else {
                Err(SelectorMismatch::TrailingSubjects {
                    step_count: self.steps.len(),
                    path_index: path_index + 1,
                })
            };
        }

        match self.combinators[step_index] {
            SelectorCombinator::Child => self.diagnose_path_from(next_step, path_index + 1, path),
            SelectorCombinator::Descendant => {
                let mut later_failure = None;
                for index in path_index + 1..path.len() {
                    let result = self.diagnose_path_from(next_step, index, path);
                    match result {
                        Ok(()) => return Ok(()),
                        Err(SelectorMismatch::StepMismatch { .. }) => {}
                        Err(error) => later_failure = Some(error),
                    }
                }
                Err(
                    later_failure.unwrap_or(SelectorMismatch::MissingDescendant {
                        step_index: next_step,
                        after_path_index: path_index + 1,
                    }),
                )
            }
        }
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

fn child_combinators(step_count: usize) -> Box<[SelectorCombinator]> {
    let len = step_count.saturating_sub(1);
    let mut combinators = Vec::with_capacity(len);
    combinators.resize(len, SelectorCombinator::Child);
    combinators.into_boxed_slice()
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
    fn owned_selector_inputs_sort_and_dedup() {
        let owned = SelectorInputsOwned::with_part(
            Some(TypeTag(1)),
            Some(PartTag(2)),
            [ClassId(3), ClassId(1), ClassId(3)],
            [PseudoClassId(4), PseudoClassId(2), PseudoClassId(4)],
        );

        assert_eq!(owned.type_tag(), Some(TypeTag(1)));
        assert_eq!(owned.part_tag(), Some(PartTag(2)));
        assert_eq!(owned.classes(), &[ClassId(1), ClassId(3)]);
        assert_eq!(owned.pseudos(), &[PseudoClassId(2), PseudoClassId(4)]);

        let inputs = owned.as_inputs();
        assert_eq!(inputs.classes, &[ClassId(1), ClassId(3)]);
        assert_eq!(inputs.pseudos, &[PseudoClassId(2), PseudoClassId(4)]);
    }

    #[test]
    fn selector_input_conveniences_match_explicit_inputs() {
        let pseudos = [PseudoClassId(4)];

        assert_eq!(
            SelectorInputs::typed(TypeTag(1)),
            SelectorInputs::new(Some(TypeTag(1)), &[], &[])
        );
        assert_eq!(
            SelectorInputs::part(PartTag(2)),
            SelectorInputs::with_part(None, Some(PartTag(2)), &[], &[])
        );
        assert_eq!(
            SelectorInputs::typed_part(TypeTag(1), PartTag(2)),
            SelectorInputs::with_part(Some(TypeTag(1)), Some(PartTag(2)), &[], &[])
        );
        assert_eq!(
            SelectorInputs::typed_with_pseudos(TypeTag(1), &pseudos),
            SelectorInputs::new(Some(TypeTag(1)), &[], &pseudos)
        );
    }

    #[test]
    fn selector_matches_subsets() {
        let selector = SelectorStep {
            type_tag: Some(TypeTag(1)),
            part_tag: None,
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
            part_tag: Some(PartTag(7)),
            required_classes: IdSet::from_ids([ClassId(2), ClassId(3)]),
            required_pseudos: IdSet::from_ids([PseudoClassId(10)]),
        };
        assert_eq!(selector.specificity(), Specificity(1, 2, 1, 1));
    }

    #[test]
    fn selector_step_builders_compose_predicates() {
        let selector = SelectorStep::type_tag(TypeTag(1))
            .with_part_tag(PartTag(2))
            .with_classes([ClassId(3), ClassId(4), ClassId(3)])
            .with_pseudo(PseudoClassId(5));

        let classes = [ClassId(3), ClassId(4)];
        let pseudos = [PseudoClassId(5)];
        let inputs =
            SelectorInputs::with_part(Some(TypeTag(1)), Some(PartTag(2)), &classes, &pseudos);

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
            part_tag: None,
            required_classes: IdSet::default(),
            required_pseudos: IdSet::default(),
        };
        let class_only = SelectorStep {
            type_tag: None,
            part_tag: None,
            required_classes: IdSet::from_ids([ClassId(1)]),
            required_pseudos: IdSet::default(),
        };
        assert!(class_only.specificity() > type_only.specificity());
    }

    #[test]
    fn selector_matches_optional_part() {
        let selector = SelectorStep {
            type_tag: Some(TypeTag(1)),
            part_tag: Some(PartTag(2)),
            required_classes: IdSet::default(),
            required_pseudos: IdSet::default(),
        };

        let matched = SelectorInputs::with_part(Some(TypeTag(1)), Some(PartTag(2)), &[], &[]);
        let wrong_part = SelectorInputs::with_part(Some(TypeTag(1)), Some(PartTag(3)), &[], &[]);
        let no_part = SelectorInputs::new(Some(TypeTag(1)), &[], &[]);

        assert!(selector.matches(&matched));
        assert!(!selector.matches(&wrong_part));
        assert!(!selector.matches(&no_part));
    }

    #[test]
    fn specificity_part_beats_type_but_not_class() {
        let type_only = SelectorStep {
            type_tag: Some(TypeTag(1)),
            part_tag: None,
            required_classes: IdSet::default(),
            required_pseudos: IdSet::default(),
        };
        let part_only = SelectorStep {
            type_tag: None,
            part_tag: Some(PartTag(1)),
            required_classes: IdSet::default(),
            required_pseudos: IdSet::default(),
        };
        let class_only = SelectorStep {
            type_tag: None,
            part_tag: None,
            required_classes: IdSet::from_ids([ClassId(1)]),
            required_pseudos: IdSet::default(),
        };

        assert!(part_only.specificity() > type_only.specificity());
        assert!(class_only.specificity() > part_only.specificity());
    }

    #[test]
    fn step_selector_builds_single_step_path() {
        let step = SelectorStep {
            type_tag: Some(TypeTag(1)),
            part_tag: Some(PartTag(2)),
            required_classes: IdSet::from_ids([ClassId(3)]),
            required_pseudos: IdSet::from_ids([PseudoClassId(4)]),
        };

        let classes = [ClassId(3)];
        let pseudos = [PseudoClassId(4)];
        let inputs =
            SelectorInputs::with_part(Some(TypeTag(1)), Some(PartTag(2)), &classes, &pseudos);
        let path = Selector::from(step.clone());

        assert_eq!(path.len(), 1);
        assert_eq!(path.specificity(), step.specificity());
        assert!(path.matches_path(&[inputs]));
    }

    #[test]
    fn selector_converts_from_step_array_and_slice() {
        let steps = [
            SelectorStep::type_tag(TypeTag(1)),
            SelectorStep::part_tag(PartTag(2)),
        ];

        let from_array = Selector::from(steps.clone());
        let from_slice = Selector::from(steps.as_slice());

        assert_eq!(from_array.steps(), from_slice.steps());
        assert_eq!(from_array.len(), 2);
    }

    #[test]
    fn path_and_child_constructors_are_child_paths() {
        let steps = [
            SelectorStep::type_tag(TypeTag(1)),
            SelectorStep::part_tag(PartTag(2)),
        ];

        let from_path = Selector::path(steps.clone());
        let from_child = Selector::child(steps[0].clone(), steps[1].clone());

        assert_eq!(from_path.steps(), from_child.steps());
        assert_eq!(from_child.combinators(), &[SelectorCombinator::Child]);
    }

    #[test]
    fn path_selector_matches_nested_part_path() {
        const TOGGLE: TypeTag = TypeTag(1);
        const TRACK: PartTag = PartTag(10);
        const THUMB: PartTag = PartTag(11);

        let selector = Selector::from([
            SelectorStep::type_tag(TOGGLE),
            SelectorStep::part_tag(TRACK),
            SelectorStep::part_tag(THUMB),
        ]);

        let root = SelectorInputs::typed(TOGGLE);
        let track = SelectorInputs::part(TRACK);
        let thumb = SelectorInputs::part(THUMB);
        let direct_thumb = SelectorInputs::part(THUMB);

        assert!(selector.matches_path(&[root, track, thumb]));
        assert!(!selector.matches_path(&[root, direct_thumb]));
    }

    #[test]
    fn empty_selector_only_matches_empty_path() {
        let selector = Selector::from_steps([]);
        let root = SelectorInputs::new(Some(TypeTag(1)), &[], &[]);

        assert!(selector.matches_path(&[]));
        assert!(!selector.matches_path(&[root]));
    }

    #[test]
    fn descendant_selector_skips_intermediate_subjects() {
        const TOGGLE: TypeTag = TypeTag(1);
        const TRACK: PartTag = PartTag(10);
        const THUMB: PartTag = PartTag(11);

        let selector = Selector::from_segments(
            SelectorStep::type_tag(TOGGLE),
            [(
                SelectorCombinator::Descendant,
                SelectorStep::part_tag(THUMB),
            )],
        );

        let root = SelectorInputs::typed(TOGGLE);
        let track = SelectorInputs::part(TRACK);
        let thumb = SelectorInputs::part(THUMB);

        assert!(selector.matches_path(&[root, track, thumb]));
        assert!(selector.matches_path(&[root, thumb]));
        assert!(!selector.matches_path(&[root, track]));
        assert_eq!(selector.combinators(), &[SelectorCombinator::Descendant]);
    }

    #[test]
    fn descendant_constructor_builds_two_step_selector() {
        const ROW: TypeTag = TypeTag(1);
        const TEXT: PartTag = PartTag(10);
        const BADGE: PartTag = PartTag(11);

        let selector =
            Selector::descendant(SelectorStep::type_tag(ROW), SelectorStep::part_tag(TEXT));

        let root = SelectorInputs::typed(ROW);
        let badge = SelectorInputs::part(BADGE);
        let text = SelectorInputs::part(TEXT);

        assert_eq!(selector.combinators(), &[SelectorCombinator::Descendant]);
        assert!(selector.matches_path(&[root, badge, text]));
    }

    #[test]
    fn selector_builder_mixes_child_and_descendant_steps() {
        const ROW: TypeTag = TypeTag(1);
        const CONTENT: PartTag = PartTag(10);
        const TEXT: PartTag = PartTag(11);

        let selector = Selector::builder(SelectorStep::type_tag(ROW))
            .descendant(SelectorStep::part_tag(CONTENT))
            .child(SelectorStep::part_tag(TEXT))
            .build();

        assert_eq!(
            selector.combinators(),
            &[SelectorCombinator::Descendant, SelectorCombinator::Child]
        );

        let root = SelectorInputs::typed(ROW);
        let wrapper = SelectorInputs::with_part(None, Some(PartTag(99)), &[], &[]);
        let content = SelectorInputs::part(CONTENT);
        let text = SelectorInputs::part(TEXT);

        assert!(selector.matches_path(&[root, wrapper, content, text]));
    }

    #[test]
    fn selector_diagnoses_path_mismatches() {
        const ROW: TypeTag = TypeTag(1);
        const CONTENT: PartTag = PartTag(10);
        const TEXT: PartTag = PartTag(11);

        let selector = Selector::child(SelectorStep::type_tag(ROW), SelectorStep::part_tag(TEXT));
        let root = SelectorInputs::typed(ROW);
        let content = SelectorInputs::part(CONTENT);

        assert_eq!(
            selector.diagnose_path(&[root, content]),
            Err(SelectorMismatch::StepMismatch {
                step_index: 1,
                path_index: 1,
            })
        );

        let descendant =
            Selector::descendant(SelectorStep::type_tag(ROW), SelectorStep::part_tag(TEXT));
        assert_eq!(
            descendant.diagnose_path(&[root, content]),
            Err(SelectorMismatch::MissingDescendant {
                step_index: 1,
                after_path_index: 1,
            })
        );
    }

    #[test]
    fn explicit_child_combinator_requires_adjacency() {
        const TOGGLE: TypeTag = TypeTag(1);
        const TRACK: PartTag = PartTag(10);
        const THUMB: PartTag = PartTag(11);

        let selector = Selector::from_steps_with_combinators(
            [
                SelectorStep::type_tag(TOGGLE),
                SelectorStep::part_tag(THUMB),
            ],
            [SelectorCombinator::Child],
        );

        let root = SelectorInputs::typed(TOGGLE);
        let track = SelectorInputs::part(TRACK);
        let thumb = SelectorInputs::part(THUMB);

        assert!(selector.matches_path(&[root, thumb]));
        assert!(!selector.matches_path(&[root, track, thumb]));
    }

    #[test]
    fn owner_pseudos_match_through_ancestor_step() {
        const TOGGLE: TypeTag = TypeTag(1);
        const TRACK: PartTag = PartTag(10);
        const CHECKED: PseudoClassId = PseudoClassId(20);

        let owner_checked_track = Selector::from([
            SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
            SelectorStep::part_tag(TRACK),
        ]);
        let track_checked = Selector::from([
            SelectorStep::type_tag(TOGGLE),
            SelectorStep::part_tag(TRACK).with_pseudo(CHECKED),
        ]);

        let pseudos = [CHECKED];
        let root = SelectorInputs::typed_with_pseudos(TOGGLE, &pseudos);
        let track = SelectorInputs::part(TRACK);

        assert!(owner_checked_track.matches_path(&[root, track]));
        assert!(!track_checked.matches_path(&[root, track]));
    }

    #[test]
    fn path_specificity_sums_steps_deterministically() {
        let selector = Selector::from([
            SelectorStep::type_tag(TypeTag(1)).with_pseudo(PseudoClassId(1)),
            SelectorStep::part_tag(PartTag(2)).with_classes([ClassId(1), ClassId(2)]),
        ]);

        assert_eq!(selector.specificity(), Specificity(1, 2, 1, 1));
    }
}
