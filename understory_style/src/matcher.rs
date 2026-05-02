// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Incremental matching over style subject paths.
//!
//! The matcher exposes a compact [`MatchState`] so embedders can walk their own
//! tree of style subjects without giving `understory_style` access to widget or
//! template nodes. The matcher compiles child and descendant selector paths into
//! interned NFA states.

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::{Ref, RefCell};

use invalidation::ChannelSet;
use understory_property::{Property, PropertyId, PropertyRegistry};

use crate::selector::{
    ClassId, PartTag, PseudoClassId, Selector, SelectorCombinator, SelectorInputs, Specificity,
    TypeTag,
};
use crate::style::{Style, StyleValueRef};
use crate::stylesheet::StyleOrigin;

const LINEAR_LOOKUP_LIMIT: usize = 8;

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

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Cursor {
    rule_index: u32,
    step_index: u16,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CascadeEntryKey {
    origin: StyleOrigin,
    specificity: Specificity,
    source_index: usize,
    order: u32,
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

    fn cascade_key(&self) -> CascadeEntryKey {
        CascadeEntryKey {
            origin: self.origin,
            specificity: self.selector.specificity(),
            source_index: self.source_index,
            order: self.order,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StateData {
    active: Box<[Cursor]>,
    matched_rules: Box<[u32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SubjectKey {
    type_tag: Option<TypeTag>,
    part_tag: Option<PartTag>,
    classes: Box<[ClassId]>,
    pseudos: Box<[PseudoClassId]>,
}

impl SubjectKey {
    fn from_inputs(inputs: &SelectorInputs<'_>) -> Self {
        Self {
            type_tag: inputs.type_tag,
            part_tag: inputs.part_tag,
            classes: inputs.classes.into(),
            pseudos: inputs.pseudos.into(),
        }
    }

    fn cmp_inputs(&self, inputs: &SelectorInputs<'_>) -> core::cmp::Ordering {
        self.type_tag
            .cmp(&inputs.type_tag)
            .then_with(|| self.part_tag.cmp(&inputs.part_tag))
            .then_with(|| self.classes.as_ref().cmp(inputs.classes))
            .then_with(|| self.pseudos.as_ref().cmp(inputs.pseudos))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransitionEntry {
    inputs: SubjectKey,
    child: MatchState,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct StateLookupEntry {
    state_index: u32,
}

impl StateLookupEntry {
    fn new(state_index: usize) -> Self {
        Self {
            state_index: u32::try_from(state_index).expect("too many matcher states"),
        }
    }

    fn index(self) -> usize {
        usize::try_from(self.state_index).expect("match state index must fit in usize")
    }
}

#[derive(Debug, Default)]
struct MatcherData {
    rules: Vec<MatchRule>,
    states: RefCell<Vec<StateData>>,
    state_lookup: RefCell<Vec<StateLookupEntry>>,
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

        let state = {
            let states = self.inner.states.borrow();
            self.advance_state(&states[state_index(parent)], inputs)
        };
        let child = self.intern_state(state);
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
        let mut best = None;

        for rule in self.matching_rules(state) {
            let Some(value) = rule.style.value_ref(property) else {
                continue;
            };
            let key = rule.cascade_key();
            if best.as_ref().is_none_or(
                |(best_key, _): &(CascadeEntryKey, StyleValueRef<'_, T>)| key > *best_key,
            ) {
                best = Some((key, value));
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

    fn advance_state(&self, parent: &StateData, inputs: &SelectorInputs<'_>) -> StateData {
        let mut active = Vec::new();
        let mut matched_rules = Vec::new();

        for cursor in parent.active.iter().copied() {
            let rule_index = cursor_rule_index(cursor);
            let step_index = usize::from(cursor.step_index);
            let rule = &self.inner.rules[rule_index];
            let steps = rule.selector.steps();
            let retain_for_descendants =
                rule.selector.combinator_before(step_index) == Some(SelectorCombinator::Descendant);

            if steps[step_index].matches(inputs) {
                let next_step = step_index + 1;
                if next_step == steps.len() {
                    matched_rules.push(cursor.rule_index);
                } else {
                    active.push(make_cursor(rule_index, next_step));
                }
            }
            if retain_for_descendants {
                active.push(cursor);
            }
        }

        active.sort_unstable();
        active.dedup();
        matched_rules.sort_unstable();
        matched_rules.dedup();

        StateData {
            active: active.into_boxed_slice(),
            matched_rules: matched_rules.into_boxed_slice(),
        }
    }

    fn intern_state(&self, state: StateData) -> MatchState {
        let mut states = self.inner.states.borrow_mut();
        if states.len() <= LINEAR_LOOKUP_LIMIT {
            if let Some(index) = states.iter().position(|existing| existing == &state) {
                return self.make_state(index);
            }

            let index = states.len();
            states.push(state);
            let mut state_lookup = self.inner.state_lookup.borrow_mut();
            state_lookup.push(StateLookupEntry::new(index));
            if states.len() == LINEAR_LOOKUP_LIMIT + 1 {
                state_lookup
                    .sort_by(|left, right| states[left.index()].cmp(&states[right.index()]));
            }
            self.inner
                .transitions_by_parent
                .borrow_mut()
                .push(Vec::new());
            return self.make_state(index);
        }

        let mut state_lookup = self.inner.state_lookup.borrow_mut();
        let lookup_index = state_lookup.binary_search_by(|entry| states[entry.index()].cmp(&state));
        if let Ok(index) = lookup_index {
            return self.make_state(state_lookup[index].index());
        }

        let index = states.len();
        states.push(state);
        state_lookup.insert(
            lookup_index.expect_err("state lookup must be missing before insert"),
            StateLookupEntry::new(index),
        );
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
        let transitions_by_parent = self.inner.transitions_by_parent.borrow();
        let transitions = transitions_by_parent.get(state_index(parent))?;
        if transitions.len() <= LINEAR_LOOKUP_LIMIT {
            return transitions
                .iter()
                .find(|entry| entry.inputs.cmp_inputs(inputs).is_eq())
                .map(|entry| entry.child);
        }

        transitions
            .binary_search_by(|entry| entry.inputs.cmp_inputs(inputs))
            .ok()
            .map(|index| transitions[index].child)
    }

    fn cache_transition(&self, parent: MatchState, inputs: &SelectorInputs<'_>, child: MatchState) {
        let mut transitions_by_parent = self.inner.transitions_by_parent.borrow_mut();
        let transitions = &mut transitions_by_parent[state_index(parent)];
        if transitions.len() < LINEAR_LOOKUP_LIMIT {
            if let Some(entry) = transitions
                .iter_mut()
                .find(|entry| entry.inputs.cmp_inputs(inputs).is_eq())
            {
                entry.child = child;
            } else {
                transitions.push(TransitionEntry {
                    inputs: SubjectKey::from_inputs(inputs),
                    child,
                });
            }
            return;
        }

        let key = SubjectKey::from_inputs(inputs);
        if transitions.len() == LINEAR_LOOKUP_LIMIT {
            transitions.sort_by(|left, right| left.inputs.cmp(&right.inputs));
        }
        match transitions.binary_search_by(|entry| entry.inputs.cmp(&key)) {
            Ok(index) => transitions[index].child = child,
            Err(index) => transitions.insert(index, TransitionEntry { inputs: key, child }),
        }
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedEntryCache {
    property_id: PropertyId,
    key: Option<CascadeEntryKey>,
}

#[derive(Debug, Default)]
struct StyleCascadeData {
    direct_styles: Vec<DirectStyleSource>,
    matcher: Matcher,
    resolved_entry_cache: RefCell<Vec<Vec<ResolvedEntryCache>>>,
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

/// Result of re-entering one style subject and comparing it with its old state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubjectRestyle {
    state: MatchState,
    changed_properties: StyleChangeSet,
    changed_channels: ChannelSet,
}

impl SubjectRestyle {
    /// Returns the new matcher state for the subject.
    #[must_use]
    pub fn state(&self) -> MatchState {
        self.state
    }

    /// Returns properties whose winning style source changed.
    #[must_use]
    pub fn changed_properties(&self) -> &StyleChangeSet {
        &self.changed_properties
    }

    /// Returns the invalidation channels affected by the changed properties.
    #[must_use]
    pub fn changed_channels(&self) -> ChannelSet {
        self.changed_channels
    }
}

/// The style source that wins a property at a matched state.
#[derive(Copy, Clone, Debug)]
pub enum WinningStyleSource<'a> {
    /// A direct style source wins.
    Direct {
        /// The winning style origin.
        origin: StyleOrigin,
        /// Source index used for cascade ordering.
        source_index: usize,
        /// The winning direct style.
        style: &'a Style,
    },
    /// A selector rule wins.
    Rule(&'a MatchRule),
}

impl WinningStyleSource<'_> {
    /// Returns the winning source's cascade origin.
    #[must_use]
    pub fn origin(&self) -> StyleOrigin {
        match self {
            Self::Direct { origin, .. } => *origin,
            Self::Rule(rule) => rule.origin(),
        }
    }

    /// Returns the winning source index.
    #[must_use]
    pub fn source_index(&self) -> usize {
        match self {
            Self::Direct { source_index, .. } => *source_index,
            Self::Rule(rule) => rule.source_index(),
        }
    }

    /// Returns the winning style payload.
    #[must_use]
    pub fn style(&self) -> &Style {
        match self {
            Self::Direct { style, .. } => style,
            Self::Rule(rule) => rule.style(),
        }
    }

    /// Returns the winning selector rule, if the source is rule-based.
    #[must_use]
    pub fn rule(&self) -> Option<&MatchRule> {
        match self {
            Self::Direct { .. } => None,
            Self::Rule(rule) => Some(rule),
        }
    }
}

impl<'a> WinningStyleSource<'a> {
    fn cascade_key(&self) -> CascadeEntryKey {
        match self {
            Self::Direct {
                origin,
                source_index,
                ..
            } => CascadeEntryKey {
                origin: *origin,
                specificity: Specificity::default(),
                source_index: *source_index,
                order: 0,
            },
            Self::Rule(rule) => rule.cascade_key(),
        }
    }

    fn value_ref<T: Clone + 'static>(&self, property: Property<T>) -> Option<StyleValueRef<'a, T>> {
        match self {
            Self::Direct { style, .. } => style.value_ref(property),
            Self::Rule(rule) => rule.style.value_ref(property),
        }
    }

    fn contains_id(&self, property_id: PropertyId) -> bool {
        self.style().contains_id(property_id)
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

    /// Advances one subject and returns the changed properties and channels.
    ///
    /// This is a convenience for the common update path where an embedder has a
    /// previous subject state, a current parent state, and freshly computed
    /// selector inputs.
    #[must_use]
    pub fn restyle_subject(
        &self,
        registry: &PropertyRegistry,
        old_state: MatchState,
        parent: MatchState,
        inputs: &SelectorInputs<'_>,
    ) -> SubjectRestyle {
        let state = self.enter_subject(parent, inputs);
        let changed_properties = self.changed_properties(old_state, state);
        let changed_channels = changed_properties.affected_channels(registry);
        SubjectRestyle {
            state,
            changed_properties,
            changed_channels,
        }
    }

    /// Returns the selector rules that match the given state.
    ///
    /// This is a diagnostic view over the same matched-rule set used by style
    /// resolution.
    #[must_use]
    pub fn matching_rules(&self, state: MatchState) -> RuleCursor<'_> {
        self.inner.matcher.matching_rules(state)
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
        self.compute_winning_source(state, property.id())?
            .value_ref(property)
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

    /// Returns the style source that wins a property at this state.
    ///
    /// This is intended for diagnostics and inspection. It reports direct
    /// styles as well as selector rules.
    #[must_use]
    pub fn winning_source<T: Clone + 'static>(
        &self,
        state: MatchState,
        property: Property<T>,
    ) -> Option<WinningStyleSource<'_>> {
        self.compute_winning_source(state, property.id())
    }

    fn candidate_property_ids(&self, old: MatchState, new: MatchState) -> Vec<PropertyId> {
        let mut property_ids = Vec::new();

        self.extend_candidate_property_ids(old, &mut property_ids);
        self.extend_candidate_property_ids(new, &mut property_ids);

        property_ids.sort_unstable();
        property_ids.dedup();
        property_ids
    }

    fn extend_candidate_property_ids(&self, state: MatchState, property_ids: &mut Vec<PropertyId>) {
        for source in self.style_sources(state) {
            property_ids.extend(source.style().property_ids());
        }
    }

    fn winning_entry_key(
        &self,
        state: MatchState,
        property_id: PropertyId,
    ) -> Option<CascadeEntryKey> {
        let state_index = state_index(state);
        if let Some(key) = self.cached_winning_entry_key(state_index, property_id) {
            return key;
        }

        let key = self.compute_winning_entry_key(state, property_id);
        self.cache_winning_entry_key(state_index, property_id, key);
        key
    }

    fn cached_winning_entry_key(
        &self,
        state_index: usize,
        property_id: PropertyId,
    ) -> Option<Option<CascadeEntryKey>> {
        self.inner
            .resolved_entry_cache
            .borrow()
            .get(state_index)?
            .iter()
            .find(|entry| entry.property_id == property_id)
            .map(|entry| entry.key)
    }

    fn cache_winning_entry_key(
        &self,
        state_index: usize,
        property_id: PropertyId,
        key: Option<CascadeEntryKey>,
    ) {
        let mut cache = self.inner.resolved_entry_cache.borrow_mut();
        if cache.len() <= state_index {
            cache.resize_with(state_index + 1, Vec::new);
        }
        cache[state_index].push(ResolvedEntryCache { property_id, key });
    }

    fn compute_winning_entry_key(
        &self,
        state: MatchState,
        property_id: PropertyId,
    ) -> Option<CascadeEntryKey> {
        self.compute_winning_source(state, property_id)
            .map(|source| source.cascade_key())
    }

    fn compute_winning_source(
        &self,
        state: MatchState,
        property_id: PropertyId,
    ) -> Option<WinningStyleSource<'_>> {
        let mut best = None;

        for source in self.style_sources(state) {
            if !source.contains_id(property_id) {
                continue;
            }
            let key = source.cascade_key();
            if best.as_ref().is_none_or(
                |(best_key, _): &(CascadeEntryKey, WinningStyleSource<'_>)| key > *best_key,
            ) {
                best = Some((key, source));
            }
        }

        best.map(|(_, source)| source)
    }

    fn style_sources(&self, state: MatchState) -> impl Iterator<Item = WinningStyleSource<'_>> {
        let direct_styles =
            self.inner
                .direct_styles
                .iter()
                .map(|source| WinningStyleSource::Direct {
                    origin: source.origin,
                    source_index: source.source_index,
                    style: &source.style,
                });
        let rules = self
            .inner
            .matcher
            .matching_rules(state)
            .map(WinningStyleSource::Rule);
        direct_styles.chain(rules)
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
                resolved_entry_cache: RefCell::new(Vec::from([Vec::new()])),
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
        let matched_rules = &self.states[self.state_index].matched_rules;
        if self.rule_index == matched_rules.len() {
            return None;
        }

        let index =
            usize::try_from(matched_rules[self.rule_index]).expect("rule index must fit in usize");
        self.rule_index += 1;
        Some(&self.rules[index])
    }
}

fn state_index(state: MatchState) -> usize {
    usize::try_from(state.index).expect("match state index must fit in usize")
}

fn cursor_rule_index(cursor: Cursor) -> usize {
    usize::try_from(cursor.rule_index).expect("rule index must fit in usize")
}

fn make_cursor(rule_index: usize, step_index: usize) -> Cursor {
    Cursor {
        rule_index: u32::try_from(rule_index).expect("too many matcher rules"),
        step_index: u16::try_from(step_index).expect("selector path is too deep"),
    }
}

fn make_state(index: usize, matcher_id: usize) -> MatchState {
    let _ = matcher_id;
    MatchState {
        index: u32::try_from(index).expect("too many matcher states"),
        #[cfg(debug_assertions)]
        matcher_id,
    }
}

fn build_matcher(rules: Vec<MatchRule>) -> Matcher {
    let root_active = rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| !rule.selector.is_empty())
        .map(|(rule_index, _)| make_cursor(rule_index, 0))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    Matcher {
        inner: Rc::new(MatcherData {
            rules,
            states: RefCell::new(Vec::from([StateData {
                active: root_active,
                matched_rules: Box::default(),
            }])),
            state_lookup: RefCell::new(Vec::from([StateLookupEntry::new(0)])),
            transitions_by_parent: RefCell::new(Vec::from([Vec::new()])),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        IdSet, PartTag, PseudoClassId, SelectorCombinator, SelectorStep, StyleBuilder, TypeTag,
    };
    use invalidation::Channel;
    use understory_property::{PropertyMetadataBuilder, PropertyRegistry};

    const TOGGLE: TypeTag = TypeTag(1);
    const TRACK: PartTag = PartTag(10);
    const THUMB: PartTag = PartTag(11);
    const CONTENT: PartTag = PartTag(12);
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
                        part_tag: Some(TRACK),
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
                        part_tag: Some(TRACK),
                        ..SelectorStep::default()
                    },
                    SelectorStep {
                        part_tag: Some(THUMB),
                        ..SelectorStep::default()
                    },
                ]),
                StyleBuilder::new().set(width, 30_u32).build(),
            )
            .build();

        let root = matcher.enter_subject(matcher.root_state(), &SelectorInputs::typed(TOGGLE));
        let track = matcher.enter_subject(root, &SelectorInputs::part(TRACK));
        let thumb = matcher.enter_subject(track, &SelectorInputs::part(THUMB));

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
                        part_tag: Some(TRACK),
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
                        part_tag: Some(CONTENT),
                        ..SelectorStep::default()
                    },
                ]),
                StyleBuilder::new().build(),
            )
            .build();

        let root = matcher.enter_subject(matcher.root_state(), &SelectorInputs::typed(TOGGLE));
        let track = matcher.enter_subject(root, &SelectorInputs::part(TRACK));
        let content = matcher.enter_subject(root, &SelectorInputs::part(CONTENT));

        assert_eq!(matcher.matching_rules(track).count(), 1);
        assert_eq!(matcher.matching_rules(content).count(), 1);

        let leaked = matcher.enter_subject(track, &SelectorInputs::part(CONTENT));
        assert_eq!(matcher.matching_rules(leaked).count(), 0);
    }

    #[test]
    fn transition_cache_keeps_parent_entries_sorted_and_reused() {
        let matcher = MatcherBuilder::new().build();
        let root = matcher.root_state();

        for part in [THUMB, CONTENT, TRACK] {
            let _ =
                matcher.enter_subject(root, &SelectorInputs::with_part(None, Some(part), &[], &[]));
        }

        let _ = matcher.enter_subject(root, &SelectorInputs::part(TRACK));
        assert_eq!(
            matcher.inner.transitions_by_parent.borrow()[state_index(root)].len(),
            3
        );

        for part in (20..30).rev().map(PartTag) {
            let _ =
                matcher.enter_subject(root, &SelectorInputs::with_part(None, Some(part), &[], &[]));
        }

        let transitions = matcher.inner.transitions_by_parent.borrow();
        let root_transitions = &transitions[state_index(root)];
        assert_eq!(root_transitions.len(), 13);
        assert!(
            root_transitions
                .windows(2)
                .all(|window| window[0].inputs < window[1].inputs)
        );
    }

    #[test]
    fn state_interning_reuses_equal_states_from_different_inputs() {
        let matcher = MatcherBuilder::new()
            .rule(
                Selector::single(SelectorStep::type_tag(TOGGLE)),
                StyleBuilder::new().build(),
            )
            .build();
        let root = matcher.root_state();

        let first_unmatched =
            matcher.enter_subject(root, &SelectorInputs::new(Some(TypeTag(98)), &[], &[]));
        let second_unmatched =
            matcher.enter_subject(root, &SelectorInputs::new(Some(TypeTag(99)), &[], &[]));

        assert_eq!(first_unmatched, second_unmatched);
        assert_eq!(matcher.inner.states.borrow().len(), 2);
        assert_eq!(matcher.inner.state_lookup.borrow().len(), 2);
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
                        part_tag: Some(TRACK),
                        ..SelectorStep::default()
                    },
                ]),
                StyleBuilder::new().build(),
            )
            .build();

        let checked = [CHECKED];
        let checked_root = matcher.enter_subject(
            matcher.root_state(),
            &SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
        );
        let unchecked_root =
            matcher.enter_subject(matcher.root_state(), &SelectorInputs::typed(TOGGLE));
        let checked_track = matcher.enter_subject(checked_root, &SelectorInputs::part(TRACK));
        let unchecked_track = matcher.enter_subject(unchecked_root, &SelectorInputs::part(TRACK));

        assert_eq!(matcher.matching_rules(checked_track).count(), 1);
        assert_eq!(matcher.matching_rules(unchecked_track).count(), 0);
    }

    #[test]
    fn descendant_combinator_skips_intermediate_subjects() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0_u32).build());

        let matcher = MatcherBuilder::new()
            .rule(
                Selector::from_segments(
                    SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
                    [(
                        SelectorCombinator::Descendant,
                        SelectorStep::part_tag(THUMB),
                    )],
                ),
                StyleBuilder::new().set(width, 30_u32).build(),
            )
            .build();

        let checked = [CHECKED];
        let root = matcher.enter_subject(
            matcher.root_state(),
            &SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
        );
        let track = matcher.enter_subject(root, &SelectorInputs::part(TRACK));
        let thumb = matcher.enter_subject(track, &SelectorInputs::part(THUMB));
        let content = matcher.enter_subject(root, &SelectorInputs::part(CONTENT));

        assert_eq!(matcher.get_value_ref(thumb, width), Some(&30));
        assert_eq!(matcher.matching_rules(track).count(), 0);
        assert_eq!(matcher.matching_rules(content).count(), 0);
    }

    #[test]
    fn descendant_combinator_retains_pending_cursor_after_match() {
        let matcher = MatcherBuilder::new()
            .rule(
                Selector::from_segments(
                    SelectorStep::type_tag(TOGGLE),
                    [(
                        SelectorCombinator::Descendant,
                        SelectorStep::part_tag(THUMB),
                    )],
                ),
                StyleBuilder::new().build(),
            )
            .build();

        let root = matcher.enter_subject(matcher.root_state(), &SelectorInputs::typed(TOGGLE));
        let first_thumb = matcher.enter_subject(root, &SelectorInputs::part(THUMB));
        let nested_thumb = matcher.enter_subject(first_thumb, &SelectorInputs::part(THUMB));

        assert_eq!(matcher.matching_rules(first_thumb).count(), 1);
        assert_eq!(matcher.matching_rules(nested_thumb).count(), 1);
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
                        part_tag: Some(TRACK),
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
        let root = cascade.enter_subject(cascade.root_state(), &SelectorInputs::typed(TOGGLE));
        let track = cascade.enter_subject(
            root,
            &SelectorInputs::with_part(None, Some(TRACK), &[], &checked),
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
            &SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
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

        let root = cascade.enter_subject(cascade.root_state(), &SelectorInputs::typed(TOGGLE));

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
                    SelectorStep::part_tag(TRACK),
                ],
                StyleBuilder::new().set(width, 20_u32).build(),
            )
            .build();

        let root = cascade.enter_subject(cascade.root_state(), &SelectorInputs::typed(TOGGLE));
        let track = cascade.enter_subject(root, &SelectorInputs::part(TRACK));

        assert_eq!(cascade.get_value_ref(root, width), Some(&10));
        assert_eq!(cascade.get_value_ref(track, width), Some(&20));
    }

    #[test]
    fn cascade_diagnostics_report_matched_rules_and_winning_source() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0_u32).build());
        let background =
            registry.register("Background", PropertyMetadataBuilder::new(0_u32).build());

        let cascade = StyleCascadeBuilder::new()
            .push_style(
                StyleOrigin::Base,
                StyleBuilder::new().set(background, 10_u32).build(),
            )
            .push_rules(
                StyleOrigin::Sheet,
                [
                    (
                        SelectorStep::type_tag(TOGGLE),
                        StyleBuilder::new().set(width, 20_u32).build(),
                    ),
                    (
                        SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
                        StyleBuilder::new().set(width, 30_u32).build(),
                    ),
                ],
            )
            .build();

        let checked = [CHECKED];
        let root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
        );

        assert_eq!(cascade.matching_rules(root).count(), 2);

        let width_source = cascade.winning_source(root, width).expect("width source");
        let width_rule = width_source.rule().expect("width should be rule-backed");
        assert_eq!(width_source.origin(), StyleOrigin::Sheet);
        assert_eq!(width_rule.style().get(width), Some(&30));

        let background_source = cascade
            .winning_source(root, background)
            .expect("background source");
        assert!(background_source.rule().is_none());
        assert_eq!(background_source.origin(), StyleOrigin::Base);
        assert_eq!(background_source.style().get(background), Some(&10));
    }

    #[test]
    fn restyle_subject_returns_state_properties_and_channels() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register(
            "Width",
            PropertyMetadataBuilder::new(0_u32)
                .affects_channels(LAYOUT.into_set())
                .build(),
        );

        let cascade = StyleCascadeBuilder::new()
            .push_rule(
                StyleOrigin::Sheet,
                SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
                StyleBuilder::new().set(width, 20_u32).build(),
            )
            .build();

        let unchecked = cascade.enter_subject(cascade.root_state(), &SelectorInputs::typed(TOGGLE));
        let checked = [CHECKED];
        let restyle = cascade.restyle_subject(
            &registry,
            unchecked,
            cascade.root_state(),
            &SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
        );

        assert_eq!(cascade.get_value_ref(restyle.state(), width), Some(&20));
        assert_eq!(restyle.changed_properties().property_ids(), &[width.id()]);
        assert!(restyle.changed_channels().contains(LAYOUT));
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
                part_tag: Some(TRACK),
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
        let unchecked_root =
            cascade.enter_subject(cascade.root_state(), &SelectorInputs::typed(TOGGLE));
        let checked_root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
        );
        let unchecked_track = cascade.enter_subject(unchecked_root, &SelectorInputs::part(TRACK));
        let checked_track = cascade.enter_subject(checked_root, &SelectorInputs::part(TRACK));

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
                part_tag: Some(TRACK),
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
        let unchecked_root =
            cascade.enter_subject(cascade.root_state(), &SelectorInputs::typed(TOGGLE));
        let checked_root = cascade.enter_subject(
            cascade.root_state(),
            &SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
        );
        let unchecked_track = cascade.enter_subject(unchecked_root, &SelectorInputs::part(TRACK));
        let checked_track = cascade.enter_subject(checked_root, &SelectorInputs::part(TRACK));

        let changes = cascade.changed_properties(unchecked_track, checked_track);
        assert!(changes.is_empty());
        assert!(changes.affected_channels(&registry).is_empty());
    }
}
