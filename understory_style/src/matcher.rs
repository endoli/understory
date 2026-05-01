// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Incremental matching over style subject paths.
//!
//! The matcher exposes a compact [`MatchState`] so embedders can walk their own
//! tree of style subjects without giving `understory_style` access to widget or
//! template nodes. The first implementation supports exact root-to-subject
//! paths; cached transitions and broader NFA internals can be added behind this
//! API later.

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::{Ref, RefCell};

use invalidation::ChannelSet;
use understory_property::{Property, PropertyId, PropertyRegistry};

use crate::selector::{
    ClassId, PseudoClassId, Selector, SelectorInputs, Specificity, TargetTag, TypeTag,
};
use crate::style::{Style, StyleValueRef};
use crate::stylesheet::StyleOrigin;

const DEAD_PROGRESS: u16 = u16::MAX;

/// Compact handle to a matcher state during a root-to-leaf subject walk.
///
/// A state represents selector progress after entering a subject. Embedders
/// should store it alongside their own style subject if they need to compare or
/// reuse matching work.
///
/// `MatchState` values are scoped to the [`Matcher`] or [`StyleCascade`] that
/// produced them. Passing a state to another matcher or cascade is a logic
/// error. Debug builds detect this by carrying the producing matcher identity in
/// the handle; release builds keep the handle compact and rely on the same
/// invariant.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct MatchState {
    index: u32,
    #[cfg(debug_assertions)]
    matcher_id: usize,
}

/// A path selector and style payload stored in a [`Matcher`].
#[derive(Clone, Debug)]
pub struct MatchRule {
    selector: Selector,
    style: Style,
    origin: StyleOrigin,
    source_index: usize,
    order: u32,
}

impl MatchRule {
    /// Returns the rule's path selector.
    #[must_use]
    pub fn selector(&self) -> &Selector {
        &self.selector
    }

    /// Returns the rule's style payload.
    #[must_use]
    pub fn style(&self) -> &Style {
        &self.style
    }

    /// Returns the rule's cascade origin.
    #[must_use]
    pub fn origin(&self) -> StyleOrigin {
        self.origin
    }

    /// Returns the source index used for cascade ordering.
    #[must_use]
    pub fn source_index(&self) -> usize {
        self.source_index
    }

    /// Returns the rule's insertion order within its matcher.
    #[must_use]
    pub fn order(&self) -> u32 {
        self.order
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StateData {
    progress: Box<[u16]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SubjectKey {
    type_tag: Option<TypeTag>,
    target_tag: Option<TargetTag>,
    classes: Box<[ClassId]>,
    pseudos: Box<[PseudoClassId]>,
}

impl SubjectKey {
    fn from_inputs(inputs: &SelectorInputs<'_>) -> Self {
        Self {
            type_tag: inputs.type_tag,
            target_tag: inputs.target_tag,
            classes: inputs.classes.into(),
            pseudos: inputs.pseudos.into(),
        }
    }

    fn matches(&self, inputs: &SelectorInputs<'_>) -> bool {
        self.type_tag == inputs.type_tag
            && self.target_tag == inputs.target_tag
            && self.classes.as_ref() == inputs.classes
            && self.pseudos.as_ref() == inputs.pseudos
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransitionEntry {
    parent: MatchState,
    inputs: SubjectKey,
    child: MatchState,
}

#[derive(Debug, Default)]
struct MatcherData {
    rules: Vec<MatchRule>,
    states: RefCell<Vec<StateData>>,
    transitions_by_parent: RefCell<Vec<Vec<TransitionEntry>>>,
}

/// Compiled path matcher for style rules.
///
/// `Matcher` is independent of any UI tree. The embedder starts from
/// [`Matcher::root_state`] and calls [`Matcher::enter_subject`] as it walks each
/// style subject from root to leaf.
#[derive(Clone, Debug, Default)]
pub struct Matcher {
    inner: Rc<MatcherData>,
}

impl Matcher {
    /// Returns the number of rules in this matcher.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.rules.len()
    }

    /// Returns `true` if this matcher has no rules.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.rules.is_empty()
    }

    /// Returns the initial state before entering any subject.
    #[must_use]
    pub fn root_state(&self) -> MatchState {
        self.make_state(0)
    }

    /// Advances selector progress by entering one style subject.
    ///
    /// The returned state is scoped to the entered subject. Sibling subjects
    /// should each be entered from the same parent state so progress does not
    /// leak between branches.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `parent` was produced by a different
    /// [`Matcher`] or [`StyleCascade`]. Passing a cross-matcher state is always
    /// a logic error, even in release builds.
    #[must_use]
    pub fn enter_subject(&self, parent: MatchState, inputs: &SelectorInputs<'_>) -> MatchState {
        self.assert_valid_state(parent);
        if let Some(child) = self.cached_transition(parent, inputs) {
            return child;
        }

        let parent_index = state_index(parent);
        let progress = {
            let states = self.inner.states.borrow();
            let parent_progress = &states[parent_index].progress;
            self.advance_progress(parent_progress, inputs)
        };
        let child = self.intern_state(progress);
        self.cache_transition(parent, inputs, child);
        child
    }

    /// Returns the rules that match the given state.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `state` was produced by a different
    /// [`Matcher`] or [`StyleCascade`]. Passing a cross-matcher state is always
    /// a logic error, even in release builds.
    #[must_use]
    pub fn matching_rules(&self, state: MatchState) -> RuleCursor<'_> {
        self.assert_valid_state(state);
        let state_index = state_index(state);
        RuleCursor {
            rules: &self.inner.rules,
            states: self.inner.states.borrow(),
            state_index,
            rule_index: 0,
        }
    }

    /// Returns the best matching style entry for a property at this state.
    #[must_use]
    pub fn get_entry_ref<T: Clone + 'static>(
        &self,
        state: MatchState,
        property: Property<T>,
    ) -> Option<StyleValueRef<'_, T>> {
        type Key = (StyleOrigin, Specificity, usize, u32);
        let mut best: Option<(Key, StyleValueRef<'_, T>)> = None;

        for rule in self.matching_rules(state) {
            let Some(value) = rule.style.value_ref(property) else {
                continue;
            };
            let key: Key = (
                rule.origin,
                rule.selector.specificity(),
                rule.source_index,
                rule.order,
            );
            match best {
                None => best = Some((key, value)),
                Some((best_key, _)) if key > best_key => best = Some((key, value)),
                Some(_) => {}
            }
        }

        best.map(|(_, value)| value)
    }

    /// Returns the best matching concrete style value for a property at this state.
    #[must_use]
    pub fn get_value_ref<T: Clone + 'static>(
        &self,
        state: MatchState,
        property: Property<T>,
    ) -> Option<&T> {
        match self.get_entry_ref(state, property)? {
            StyleValueRef::Value(value) => Some(value),
            StyleValueRef::Resource(_) => None,
        }
    }

    fn advance_progress(&self, parent_progress: &[u16], inputs: &SelectorInputs<'_>) -> Box<[u16]> {
        let progress = self
            .inner
            .rules
            .iter()
            .zip(parent_progress.iter().copied())
            .map(|(rule, progress)| advance_rule(rule, progress, inputs))
            .collect::<Vec<_>>();
        progress.into_boxed_slice()
    }

    fn intern_state(&self, progress: Box<[u16]>) -> MatchState {
        let mut states = self.inner.states.borrow_mut();
        if let Some(index) = states.iter().position(|state| state.progress == progress) {
            return self.make_state(index);
        }

        let index = states.len();
        states.push(StateData { progress });
        self.inner
            .transitions_by_parent
            .borrow_mut()
            .push(Vec::new());
        self.make_state(index)
    }

    fn cached_transition(
        &self,
        parent: MatchState,
        inputs: &SelectorInputs<'_>,
    ) -> Option<MatchState> {
        self.inner
            .transitions_by_parent
            .borrow()
            .get(state_index(parent))?
            .iter()
            .find(|entry| entry.parent == parent && entry.inputs.matches(inputs))
            .map(|entry| entry.child)
    }

    fn cache_transition(&self, parent: MatchState, inputs: &SelectorInputs<'_>, child: MatchState) {
        let mut transitions_by_parent = self.inner.transitions_by_parent.borrow_mut();
        transitions_by_parent[state_index(parent)].push(TransitionEntry {
            parent,
            inputs: SubjectKey::from_inputs(inputs),
            child,
        });
    }

    fn assert_valid_state(&self, state: MatchState) {
        #[cfg(debug_assertions)]
        {
            debug_assert_eq!(
                state.matcher_id,
                self.matcher_id(),
                "MatchState belongs to a different Matcher or StyleCascade"
            );
        }

        debug_assert!(
            state_index(state) < self.inner.states.borrow().len(),
            "MatchState index is out of bounds for this Matcher"
        );
    }

    fn make_state(&self, index: usize) -> MatchState {
        make_state(index, self.matcher_id())
    }

    fn matcher_id(&self) -> usize {
        #[cfg(debug_assertions)]
        {
            Rc::as_ptr(&self.inner).cast::<()>() as usize
        }
        #[cfg(not(debug_assertions))]
        {
            0
        }
    }
}

/// Builder for constructing a [`Matcher`].
#[derive(Debug, Default)]
pub struct MatcherBuilder {
    rules: Vec<MatchRule>,
    next_order: u32,
}

impl MatcherBuilder {
    /// Creates an empty matcher builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a path rule to the matcher.
    #[must_use]
    pub fn rule(mut self, selector: impl Into<Selector>, style: Style) -> Self {
        let order = self.next_order;
        self.next_order = self.next_order.saturating_add(1);
        self.rules.push(MatchRule {
            selector: selector.into(),
            style,
            origin: StyleOrigin::Sheet,
            source_index: 0,
            order,
        });
        self
    }

    /// Builds the matcher.
    #[must_use]
    pub fn build(self) -> Matcher {
        build_matcher(self.rules)
    }
}

#[derive(Clone, Debug)]
struct DirectStyleSource {
    origin: StyleOrigin,
    style: Style,
    source_index: usize,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CascadeEntryKey {
    origin: StyleOrigin,
    specificity: Specificity,
    source_index: usize,
    order: u32,
}

#[derive(Debug, Default)]
struct StyleCascadeData {
    direct_styles: Vec<DirectStyleSource>,
    matcher: Matcher,
}

/// Properties whose winning style source changed between two matcher states.
///
/// This change set is conservative: it compares the winning style entry source
/// for each candidate property, not the concrete typed values. If two different
/// rules produce the same value, the property is still reported as changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleChangeSet {
    property_ids: Box<[PropertyId]>,
}

impl StyleChangeSet {
    /// Returns `true` if no properties changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.property_ids.is_empty()
    }

    /// Returns the number of changed properties.
    #[must_use]
    pub fn len(&self) -> usize {
        self.property_ids.len()
    }

    /// Returns the changed property IDs.
    #[must_use]
    pub fn property_ids(&self) -> &[PropertyId] {
        &self.property_ids
    }

    /// Returns the union of invalidation channels affected by these properties.
    #[must_use]
    pub fn affected_channels(&self, registry: &PropertyRegistry) -> ChannelSet {
        let mut channels = ChannelSet::empty();
        for property_id in self.property_ids() {
            channels |= registry.affects_channels(*property_id);
        }
        channels
    }
}

/// A path-aware style cascade.
///
/// The cascade preserves origin, specificity, source order, and rule order.
/// Rule selectors are matched by walking style subjects through
/// [`StyleCascade::enter_subject`].
#[derive(Clone, Debug, Default)]
pub struct StyleCascade {
    inner: Rc<StyleCascadeData>,
}

impl StyleCascade {
    /// Returns `true` if this cascade has no direct styles or path rules.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.direct_styles.is_empty() && self.inner.matcher.is_empty()
    }

    /// Returns the initial matcher state before entering any subject.
    #[must_use]
    pub fn root_state(&self) -> MatchState {
        self.inner.matcher.root_state()
    }

    /// Advances the cascade matcher by entering one style subject.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `parent` was produced by a different
    /// [`StyleCascade`] or its underlying [`Matcher`]. Passing a cross-cascade
    /// state is always a logic error, even in release builds.
    #[must_use]
    pub fn enter_subject(&self, parent: MatchState, inputs: &SelectorInputs<'_>) -> MatchState {
        self.inner.matcher.enter_subject(parent, inputs)
    }

    /// Returns the best Style-layer entry for a property at this state.
    ///
    /// Ordering is deterministic:
    /// 1. Higher [`StyleOrigin`] wins.
    /// 2. Higher selector specificity wins (direct styles have specificity 0).
    /// 3. Later sources win.
    /// 4. Later rules win within one pushed rule group.
    #[must_use]
    pub fn get_entry_ref<T: Clone + 'static>(
        &self,
        state: MatchState,
        property: Property<T>,
    ) -> Option<StyleValueRef<'_, T>> {
        type Key = (StyleOrigin, Specificity, usize, u32);
        let mut best: Option<(Key, StyleValueRef<'_, T>)> = None;

        for source in &self.inner.direct_styles {
            let Some(value) = source.style.value_ref(property) else {
                continue;
            };
            let key: Key = (
                source.origin,
                Specificity::default(),
                source.source_index,
                0,
            );
            match best {
                None => best = Some((key, value)),
                Some((best_key, _)) if key > best_key => best = Some((key, value)),
                Some(_) => {}
            }
        }

        for rule in self.inner.matcher.matching_rules(state) {
            let Some(value) = rule.style.value_ref(property) else {
                continue;
            };
            let key: Key = (
                rule.origin,
                rule.selector.specificity(),
                rule.source_index,
                rule.order,
            );
            match best {
                None => best = Some((key, value)),
                Some((best_key, _)) if key > best_key => best = Some((key, value)),
                Some(_) => {}
            }
        }

        best.map(|(_, value)| value)
    }

    /// Returns the best concrete Style-layer value for a property at this state.
    #[must_use]
    pub fn get_value_ref<T: Clone + 'static>(
        &self,
        state: MatchState,
        property: Property<T>,
    ) -> Option<&T> {
        match self.get_entry_ref(state, property)? {
            StyleValueRef::Value(value) => Some(value),
            StyleValueRef::Resource(_) => None,
        }
    }

    /// Returns properties whose winning style source differs between states.
    ///
    /// The comparison is scoped to this cascade. It is intended for embedders
    /// that re-enter a subject after its selector inputs change and need to
    /// invalidate only the dependency-property channels affected by style.
    ///
    /// This is a source-based comparison, not a value-equality comparison. If
    /// the winning rule or direct style changes, the property is reported even
    /// when both sources currently produce equal concrete values.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if either state was produced by a different
    /// [`StyleCascade`] or its underlying [`Matcher`]. Passing cross-cascade
    /// states is always a logic error, even in release builds.
    #[must_use]
    pub fn changed_properties(&self, old: MatchState, new: MatchState) -> StyleChangeSet {
        let candidates = self.candidate_property_ids(old, new);
        let changed = candidates
            .into_iter()
            .filter(|property_id| {
                self.winning_entry_key(old, *property_id)
                    != self.winning_entry_key(new, *property_id)
            })
            .collect::<Vec<_>>();

        StyleChangeSet {
            property_ids: changed.into_boxed_slice(),
        }
    }

    /// Returns the invalidation channels affected by changed style properties.
    #[must_use]
    pub fn changed_channels(
        &self,
        registry: &PropertyRegistry,
        old: MatchState,
        new: MatchState,
    ) -> ChannelSet {
        self.changed_properties(old, new)
            .affected_channels(registry)
    }

    fn candidate_property_ids(&self, old: MatchState, new: MatchState) -> Vec<PropertyId> {
        let mut property_ids = Vec::new();

        for source in &self.inner.direct_styles {
            property_ids.extend(source.style.property_ids());
        }
        for rule in self.inner.matcher.matching_rules(old) {
            property_ids.extend(rule.style.property_ids());
        }
        for rule in self.inner.matcher.matching_rules(new) {
            property_ids.extend(rule.style.property_ids());
        }

        property_ids.sort_unstable();
        property_ids.dedup();
        property_ids
    }

    fn winning_entry_key(
        &self,
        state: MatchState,
        property_id: PropertyId,
    ) -> Option<CascadeEntryKey> {
        let mut best = None;

        for source in &self.inner.direct_styles {
            if !source.style.contains_id(property_id) {
                continue;
            }
            let key = CascadeEntryKey {
                origin: source.origin,
                specificity: Specificity::default(),
                source_index: source.source_index,
                order: 0,
            };
            if best.is_none_or(|best_key| key > best_key) {
                best = Some(key);
            }
        }

        for rule in self.inner.matcher.matching_rules(state) {
            if !rule.style.contains_id(property_id) {
                continue;
            }
            let key = CascadeEntryKey {
                origin: rule.origin,
                specificity: rule.selector.specificity(),
                source_index: rule.source_index,
                order: rule.order,
            };
            if best.is_none_or(|best_key| key > best_key) {
                best = Some(key);
            }
        }

        best
    }
}

/// Builder for constructing a [`StyleCascade`].
#[derive(Debug, Default)]
pub struct StyleCascadeBuilder {
    direct_styles: Vec<DirectStyleSource>,
    rules: Vec<MatchRule>,
    next_source_index: usize,
}

impl StyleCascadeBuilder {
    /// Creates an empty style cascade builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a direct style source.
    #[must_use]
    pub fn push_style(mut self, origin: StyleOrigin, style: Style) -> Self {
        let source_index = self.take_source_index();
        self.direct_styles.push(DirectStyleSource {
            origin,
            style,
            source_index,
        });
        self
    }

    /// Adds a one-rule path stylesheet source.
    #[must_use]
    pub fn push_rule(
        self,
        origin: StyleOrigin,
        selector: impl Into<Selector>,
        style: Style,
    ) -> Self {
        self.push_rules(origin, [(selector, style)])
    }

    /// Adds a path stylesheet source containing ordered rules.
    #[must_use]
    pub fn push_rules<S>(
        mut self,
        origin: StyleOrigin,
        rules: impl IntoIterator<Item = (S, Style)>,
    ) -> Self
    where
        S: Into<Selector>,
    {
        let source_index = self.take_source_index();
        for (order, (selector, style)) in rules.into_iter().enumerate() {
            self.rules.push(MatchRule {
                selector: selector.into(),
                style,
                origin,
                source_index,
                order: u32::try_from(order).unwrap_or(u32::MAX),
            });
        }
        self
    }

    /// Builds the style cascade.
    #[must_use]
    pub fn build(self) -> StyleCascade {
        StyleCascade {
            inner: Rc::new(StyleCascadeData {
                direct_styles: self.direct_styles,
                matcher: build_matcher(self.rules),
            }),
        }
    }

    fn take_source_index(&mut self) -> usize {
        let source_index = self.next_source_index;
        self.next_source_index = self.next_source_index.saturating_add(1);
        source_index
    }
}

/// Iterator over matching rules for a [`MatchState`].
#[derive(Debug)]
pub struct RuleCursor<'a> {
    rules: &'a [MatchRule],
    states: Ref<'a, Vec<StateData>>,
    state_index: usize,
    rule_index: usize,
}

impl<'a> Iterator for RuleCursor<'a> {
    type Item = &'a MatchRule;

    fn next(&mut self) -> Option<Self::Item> {
        let progress = &self.states[self.state_index].progress;
        while self.rule_index < self.rules.len() {
            let index = self.rule_index;
            self.rule_index += 1;
            if usize::from(progress[index]) == self.rules[index].selector.len() {
                return Some(&self.rules[index]);
            }
        }
        None
    }
}

fn advance_rule(rule: &MatchRule, progress: u16, inputs: &SelectorInputs<'_>) -> u16 {
    if progress == DEAD_PROGRESS {
        return DEAD_PROGRESS;
    }

    let progress = usize::from(progress);
    let steps = rule.selector.steps();
    if progress >= steps.len() {
        return DEAD_PROGRESS;
    }

    if steps[progress].matches(inputs) {
        u16::try_from(progress + 1).unwrap_or(DEAD_PROGRESS)
    } else {
        DEAD_PROGRESS
    }
}

fn state_index(state: MatchState) -> usize {
    usize::try_from(state.index).expect("match state index must fit in usize")
}

fn make_state(index: usize, matcher_id: usize) -> MatchState {
    MatchState {
        index: u32::try_from(index).expect("too many matcher states"),
        #[cfg(debug_assertions)]
        matcher_id,
    }
}

fn build_matcher(rules: Vec<MatchRule>) -> Matcher {
    let root_progress = vec![0; rules.len()].into_boxed_slice();
    Matcher {
        inner: Rc::new(MatcherData {
            rules,
            states: RefCell::new(vec![StateData {
                progress: root_progress,
            }]),
            transitions_by_parent: RefCell::new(vec![Vec::new()]),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IdSet, PseudoClassId, SelectorStep, StyleBuilder, TargetTag, TypeTag};
    use invalidation::Channel;
    use understory_property::{PropertyMetadataBuilder, PropertyRegistry};

    const TOGGLE: TypeTag = TypeTag(1);
    const TRACK: TargetTag = TargetTag(10);
    const THUMB: TargetTag = TargetTag(11);
    const CONTENT: TargetTag = TargetTag(12);
    const CHECKED: PseudoClassId = PseudoClassId(20);
    const LAYOUT: Channel = Channel::new(0);
    const PAINT: Channel = Channel::new(1);

    #[test]
    fn matcher_advances_root_to_nested_part() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0_u32).build());

        let matcher = MatcherBuilder::new()
            .rule(
                Selector::single(SelectorStep {
                    type_tag: Some(TOGGLE),
                    ..SelectorStep::default()
                }),
                StyleBuilder::new().set(width, 10_u32).build(),
            )
            .rule(
                Selector::from_steps([
                    SelectorStep {
                        type_tag: Some(TOGGLE),
                        ..SelectorStep::default()
                    },
                    SelectorStep {
                        target_tag: Some(TRACK),
                        ..SelectorStep::default()
                    },
                ]),
                StyleBuilder::new().set(width, 20_u32).build(),
            )
            .rule(
                Selector::from_steps([
                    SelectorStep {
                        type_tag: Some(TOGGLE),
                        ..SelectorStep::default()
                    },
                    SelectorStep {
                        target_tag: Some(TRACK),
                        ..SelectorStep::default()
                    },
                    SelectorStep {
                        target_tag: Some(THUMB),
                        ..SelectorStep::default()
                    },
                ]),
                StyleBuilder::new().set(width, 30_u32).build(),
            )
            .build();

        let root = matcher.enter_subject(
            matcher.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &[]),
        );
        let track = matcher.enter_subject(
            root,
            &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
        );
        let thumb = matcher.enter_subject(
            track,
            &SelectorInputs::with_target(None, Some(THUMB), &[], &[]),
        );

        assert_eq!(matcher.get_value_ref(root, width), Some(&10));
        assert_eq!(matcher.get_value_ref(track, width), Some(&20));
        assert_eq!(matcher.get_value_ref(thumb, width), Some(&30));
    }

    #[test]
    fn sibling_paths_do_not_leak_state() {
        let matcher = MatcherBuilder::new()
            .rule(
                Selector::from_steps([
                    SelectorStep {
                        type_tag: Some(TOGGLE),
                        ..SelectorStep::default()
                    },
                    SelectorStep {
                        target_tag: Some(TRACK),
                        ..SelectorStep::default()
                    },
                ]),
                StyleBuilder::new().build(),
            )
            .rule(
                Selector::from_steps([
                    SelectorStep {
                        type_tag: Some(TOGGLE),
                        ..SelectorStep::default()
                    },
                    SelectorStep {
                        target_tag: Some(CONTENT),
                        ..SelectorStep::default()
                    },
                ]),
                StyleBuilder::new().build(),
            )
            .build();

        let root = matcher.enter_subject(
            matcher.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &[]),
        );
        let track = matcher.enter_subject(
            root,
            &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
        );
        let content = matcher.enter_subject(
            root,
            &SelectorInputs::with_target(None, Some(CONTENT), &[], &[]),
        );

        assert_eq!(matcher.matching_rules(track).count(), 1);
        assert_eq!(matcher.matching_rules(content).count(), 1);

        let leaked = matcher.enter_subject(
            track,
            &SelectorInputs::with_target(None, Some(CONTENT), &[], &[]),
        );
        assert_eq!(matcher.matching_rules(leaked).count(), 0);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "MatchState belongs to a different Matcher or StyleCascade")]
    fn matcher_rejects_cross_matcher_states_in_debug_builds() {
        let first = MatcherBuilder::new().build();
        let second = MatcherBuilder::new().build();

        let _ = second.enter_subject(first.root_state(), &SelectorInputs::EMPTY);
    }

    #[test]
    fn owner_pseudo_affects_descendant_by_path() {
        let matcher = MatcherBuilder::new()
            .rule(
                Selector::from_steps([
                    SelectorStep {
                        type_tag: Some(TOGGLE),
                        required_pseudos: IdSet::from_ids([CHECKED]),
                        ..SelectorStep::default()
                    },
                    SelectorStep {
                        target_tag: Some(TRACK),
                        ..SelectorStep::default()
                    },
                ]),
                StyleBuilder::new().build(),
            )
            .build();

        let checked = [CHECKED];
        let checked_root = matcher.enter_subject(
            matcher.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &checked),
        );
        let unchecked_root = matcher.enter_subject(
            matcher.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &[]),
        );
        let checked_track = matcher.enter_subject(
            checked_root,
            &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
        );
        let unchecked_track = matcher.enter_subject(
            unchecked_root,
            &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
        );

        assert_eq!(matcher.matching_rules(checked_track).count(), 1);
        assert_eq!(matcher.matching_rules(unchecked_track).count(), 0);
    }

    #[test]
    fn path_cascade_applies_origin_over_specificity() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0_u32).build());

        let cascade = StyleCascadeBuilder::new()
            .push_rule(
                StyleOrigin::Sheet,
                Selector::from_steps([
                    SelectorStep {
                        type_tag: Some(TOGGLE),
                        ..SelectorStep::default()
                    },
                    SelectorStep {
                        target_tag: Some(TRACK),
                        required_pseudos: IdSet::from_ids([CHECKED]),
                        ..SelectorStep::default()
                    },
                ]),
                StyleBuilder::new().set(width, 20_u32).build(),
            )
            .push_style(
                StyleOrigin::Override,
                StyleBuilder::new().set(width, 50_u32).build(),
            )
            .build();

        let checked = [CHECKED];
        let root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &[]),
        );
        let track = cascade.enter_subject(
            root,
            &SelectorInputs::with_target(None, Some(TRACK), &[], &checked),
        );

        assert_eq!(cascade.get_value_ref(track, width), Some(&50));
    }

    #[test]
    fn path_cascade_uses_specificity_then_rule_order() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0_u32).build());

        let root_selector = Selector::single(SelectorStep {
            type_tag: Some(TOGGLE),
            ..SelectorStep::default()
        });
        let checked_root_selector = Selector::single(SelectorStep {
            type_tag: Some(TOGGLE),
            required_pseudos: IdSet::from_ids([CHECKED]),
            ..SelectorStep::default()
        });

        let cascade = StyleCascadeBuilder::new()
            .push_rules(
                StyleOrigin::Sheet,
                [
                    (
                        root_selector.clone(),
                        StyleBuilder::new().set(width, 10_u32).build(),
                    ),
                    (
                        checked_root_selector,
                        StyleBuilder::new().set(width, 20_u32).build(),
                    ),
                    (
                        root_selector,
                        StyleBuilder::new().set(width, 30_u32).build(),
                    ),
                ],
            )
            .build();

        let checked = [CHECKED];
        let root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &checked),
        );

        assert_eq!(cascade.get_value_ref(root, width), Some(&20));
    }

    #[test]
    fn path_cascade_uses_later_source_when_specificity_ties() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0_u32).build());
        let selector = Selector::single(SelectorStep {
            type_tag: Some(TOGGLE),
            ..SelectorStep::default()
        });

        let cascade = StyleCascadeBuilder::new()
            .push_rule(
                StyleOrigin::Sheet,
                selector.clone(),
                StyleBuilder::new().set(width, 10_u32).build(),
            )
            .push_rule(
                StyleOrigin::Sheet,
                selector,
                StyleBuilder::new().set(width, 20_u32).build(),
            )
            .build();

        let root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &[]),
        );

        assert_eq!(cascade.get_value_ref(root, width), Some(&20));
    }

    #[test]
    fn cascade_builder_accepts_selector_like_inputs() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0_u32).build());

        let cascade = StyleCascadeBuilder::new()
            .push_rule(
                StyleOrigin::Sheet,
                SelectorStep::type_tag(TOGGLE),
                StyleBuilder::new().set(width, 10_u32).build(),
            )
            .push_rule(
                StyleOrigin::Sheet,
                [
                    SelectorStep::type_tag(TOGGLE),
                    SelectorStep::target_tag(TRACK),
                ],
                StyleBuilder::new().set(width, 20_u32).build(),
            )
            .build();

        let root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &[]),
        );
        let track = cascade.enter_subject(
            root,
            &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
        );

        assert_eq!(cascade.get_value_ref(root, width), Some(&10));
        assert_eq!(cascade.get_value_ref(track, width), Some(&20));
    }

    #[test]
    fn style_changes_report_properties_and_channels() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register(
            "Width",
            PropertyMetadataBuilder::new(0_u32)
                .affects_channels(LAYOUT.into_set())
                .build(),
        );
        let background = registry.register(
            "Background",
            PropertyMetadataBuilder::new(0_u32)
                .affects_channels(PAINT.into_set())
                .build(),
        );

        let checked_track = Selector::from_steps([
            SelectorStep {
                type_tag: Some(TOGGLE),
                required_pseudos: IdSet::from_ids([CHECKED]),
                ..SelectorStep::default()
            },
            SelectorStep {
                target_tag: Some(TRACK),
                ..SelectorStep::default()
            },
        ]);
        let cascade = StyleCascadeBuilder::new()
            .push_rule(
                StyleOrigin::Sheet,
                checked_track.clone(),
                StyleBuilder::new().set(width, 20_u32).build(),
            )
            .push_rule(
                StyleOrigin::Sheet,
                checked_track,
                StyleBuilder::new().set(background, 0x00ff00_u32).build(),
            )
            .build();

        let checked = [CHECKED];
        let unchecked_root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &[]),
        );
        let checked_root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &checked),
        );
        let unchecked_track = cascade.enter_subject(
            unchecked_root,
            &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
        );
        let checked_track = cascade.enter_subject(
            checked_root,
            &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
        );

        let changes = cascade.changed_properties(unchecked_track, checked_track);
        assert_eq!(changes.property_ids(), &[width.id(), background.id()]);

        let channels = changes.affected_channels(&registry);
        assert!(channels.contains(LAYOUT));
        assert!(channels.contains(PAINT));
        assert_eq!(
            channels,
            cascade.changed_channels(&registry, unchecked_track, checked_track)
        );
    }

    #[test]
    fn style_changes_ignore_shadowed_rules() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register(
            "Width",
            PropertyMetadataBuilder::new(0_u32)
                .affects_channels(LAYOUT.into_set())
                .build(),
        );

        let checked_track = Selector::from_steps([
            SelectorStep {
                type_tag: Some(TOGGLE),
                required_pseudos: IdSet::from_ids([CHECKED]),
                ..SelectorStep::default()
            },
            SelectorStep {
                target_tag: Some(TRACK),
                ..SelectorStep::default()
            },
        ]);
        let cascade = StyleCascadeBuilder::new()
            .push_rule(
                StyleOrigin::Sheet,
                checked_track,
                StyleBuilder::new().set(width, 20_u32).build(),
            )
            .push_style(
                StyleOrigin::Override,
                StyleBuilder::new().set(width, 50_u32).build(),
            )
            .build();

        let checked = [CHECKED];
        let unchecked_root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &[]),
        );
        let checked_root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::new(Some(TOGGLE), &[], &checked),
        );
        let unchecked_track = cascade.enter_subject(
            unchecked_root,
            &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
        );
        let checked_track = cascade.enter_subject(
            checked_root,
            &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
        );

        let changes = cascade.changed_properties(unchecked_track, checked_track);
        assert!(changes.is_empty());
        assert!(changes.affected_channels(&registry).is_empty());
    }
}
