// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Rule-based style selection.
//!
//! A [`StyleSheet`] is a collection of [`StyleRule`]s. Each rule combines a
//! [`Selector`] predicate and a [`Style`] payload (property setters).
//!
//! This module is intentionally limited to single-element matching (no
//! combinators). The embedder is responsible for providing a [`SelectorInputs`]
//! snapshot for each element.

use alloc::rc::Rc;
use alloc::vec::Vec;

use understory_property::Property;

use crate::selector::{Selector, SelectorInputs, Specificity};
use crate::style::Style;

/// The origin/strength of a style source within the Style layer.
///
/// Higher origins win over lower ones regardless of selector specificity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StyleOrigin {
    /// Low-precedence base styling (e.g. control defaults).
    Base = 0,
    /// Rule-based styling (stylesheets).
    Sheet = 1,
    /// High-precedence overrides (e.g. explicit style assignment).
    Override = 2,
}

/// A single rule in a [`StyleSheet`].
#[derive(Clone, Debug)]
pub struct StyleRule {
    selector: Selector,
    style: Style,
    order: u32,
}

impl StyleRule {
    /// Returns the selector.
    #[must_use]
    pub fn selector(&self) -> &Selector {
        &self.selector
    }

    /// Returns the rule's style payload.
    #[must_use]
    pub fn style(&self) -> &Style {
        &self.style
    }
}

#[derive(Debug, Default)]
struct StyleSheetData {
    rules: Vec<StyleRule>,
}

/// A collection of style rules.
///
/// `StyleSheet` is immutable after creation. Use [`StyleSheetBuilder`] to
/// construct instances.
#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    inner: Rc<StyleSheetData>,
}

impl StyleSheet {
    /// Returns the number of rules in this sheet.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.rules.len()
    }

    /// Returns `true` if this sheet has no rules.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.rules.is_empty()
    }

    /// Returns an iterator over rules.
    pub fn rules(&self) -> impl Iterator<Item = &StyleRule> + '_ {
        self.inner.rules.iter()
    }

    /// Returns the best matching value for a property in this sheet.
    #[must_use]
    pub fn get_value_ref<T: Clone + 'static>(
        &self,
        inputs: &SelectorInputs<'_>,
        property: Property<T>,
    ) -> Option<&T> {
        let mut best: Option<(Specificity, u32, &T)> = None;

        for rule in &self.inner.rules {
            if !rule.selector.matches(inputs) {
                continue;
            }
            let Some(value) = rule.style.get(property) else {
                continue;
            };

            let spec = rule.selector.specificity();
            let key = (spec, rule.order);

            match best {
                None => best = Some((key.0, key.1, value)),
                Some((best_spec, best_order, _)) => {
                    if key.0 > best_spec || (key.0 == best_spec && key.1 > best_order) {
                        best = Some((key.0, key.1, value));
                    }
                }
            }
        }

        best.map(|(_, _, v)| v)
    }
}

/// Builder for constructing [`StyleSheet`] instances.
#[derive(Debug, Default)]
pub struct StyleSheetBuilder {
    rules: Vec<StyleRule>,
    next_order: u32,
}

impl StyleSheetBuilder {
    /// Creates a new empty stylesheet builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a rule to the sheet.
    #[must_use]
    pub fn rule(mut self, selector: Selector, style: Style) -> Self {
        let order = self.next_order;
        self.next_order = self.next_order.saturating_add(1);
        self.rules.push(StyleRule {
            selector,
            style,
            order,
        });
        self
    }

    /// Builds the stylesheet.
    #[must_use]
    pub fn build(self) -> StyleSheet {
        StyleSheet {
            inner: Rc::new(StyleSheetData { rules: self.rules }),
        }
    }
}

/// A style source within a [`StyleCascade`].
#[derive(Clone, Debug)]
pub enum StyleSource {
    /// A direct style (unconditional setters).
    Style {
        /// Origin/strength of this source in the cascade.
        origin: StyleOrigin,
        /// The direct style setters.
        style: Style,
    },
    /// A stylesheet (conditional rules).
    Sheet {
        /// Origin/strength of this source in the cascade.
        origin: StyleOrigin,
        /// The stylesheet rules.
        sheet: StyleSheet,
    },
}

#[derive(Debug, Default)]
struct StyleCascadeData {
    sources: Vec<StyleSource>,
}

/// A composed, ordered set of style sources.
///
/// The cascade resolves the Style-layer value for a property given
/// [`SelectorInputs`]. It is independent of dependency-property storage and
/// higher-level invalidation; it simply answers "what style value applies?"
#[derive(Clone, Debug, Default)]
pub struct StyleCascade {
    inner: Rc<StyleCascadeData>,
}

impl StyleCascade {
    /// Returns the number of sources in this cascade.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.sources.len()
    }

    /// Returns `true` if this cascade has no sources.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.sources.is_empty()
    }

    /// Returns an iterator over sources.
    pub fn sources(&self) -> impl Iterator<Item = &StyleSource> + '_ {
        self.inner.sources.iter()
    }

    /// Returns the best Style-layer value for a property.
    ///
    /// Ordering is deterministic:
    /// 1. Higher [`StyleOrigin`] wins.
    /// 2. Higher selector specificity wins (styles have specificity 0).
    /// 3. Later sources win (source order).
    /// 4. Later rules win within a sheet (rule order).
    #[must_use]
    pub fn get_value_ref<T: Clone + 'static>(
        &self,
        inputs: &SelectorInputs<'_>,
        property: Property<T>,
    ) -> Option<&T> {
        // Lexicographic tuple ordering gives the correct cascade priority:
        // (origin, specificity, source_index, rule_order)
        // — higher origin wins first, then higher specificity, then later
        // source, then later rule within a sheet.
        type Key = (StyleOrigin, Specificity, usize, u32);
        let mut best: Option<(Key, &T)> = None;

        for (source_index, source) in self.inner.sources.iter().enumerate() {
            match source {
                StyleSource::Style { origin, style } => {
                    let Some(value) = style.get(property) else {
                        continue;
                    };
                    let key: Key = (*origin, Specificity::default(), source_index, 0);
                    let replace = match best {
                        None => true,
                        Some((cur, _)) => key > cur,
                    };
                    if replace {
                        best = Some((key, value));
                    }
                }
                StyleSource::Sheet { origin, sheet } => {
                    // Find best rule in this sheet for the property.
                    let mut sheet_best: Option<(Specificity, u32, &T)> = None;
                    for rule in sheet.rules() {
                        if !rule.selector.matches(inputs) {
                            continue;
                        }
                        let Some(value) = rule.style.get(property) else {
                            continue;
                        };
                        let spec = rule.selector.specificity();
                        match sheet_best {
                            None => sheet_best = Some((spec, rule.order, value)),
                            Some((best_spec, best_order, _)) => {
                                if spec > best_spec
                                    || (spec == best_spec && rule.order > best_order)
                                {
                                    sheet_best = Some((spec, rule.order, value));
                                }
                            }
                        }
                    }

                    let Some((spec, rule_order, value)) = sheet_best else {
                        continue;
                    };
                    let key: Key = (*origin, spec, source_index, rule_order);
                    let replace = match best {
                        None => true,
                        Some((cur, _)) => key > cur,
                    };
                    if replace {
                        best = Some((key, value));
                    }
                }
            }
        }

        best.map(|(_, v)| v)
    }
}

/// Builder for constructing [`StyleCascade`] instances.
#[derive(Debug, Default)]
pub struct StyleCascadeBuilder {
    sources: Vec<StyleSource>,
}

impl StyleCascadeBuilder {
    /// Creates a new empty cascade builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a direct style source.
    #[must_use]
    pub fn push_style(mut self, origin: StyleOrigin, style: Style) -> Self {
        self.sources.push(StyleSource::Style { origin, style });
        self
    }

    /// Adds a stylesheet source.
    #[must_use]
    pub fn push_sheet(mut self, origin: StyleOrigin, sheet: StyleSheet) -> Self {
        self.sources.push(StyleSource::Sheet { origin, sheet });
        self
    }

    /// Builds the cascade.
    #[must_use]
    pub fn build(self) -> StyleCascade {
        StyleCascade {
            inner: Rc::new(StyleCascadeData {
                sources: self.sources,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::{ClassId, IdSet, PseudoClassId, Selector, SelectorInputs, TypeTag};
    use crate::style::StyleBuilder;
    use understory_property::{PropertyMetadataBuilder, PropertyRegistry};

    #[test]
    fn sheet_picks_highest_specificity_then_order() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let s1 = StyleBuilder::new().set(width, 10.0).build();
        let s2 = StyleBuilder::new().set(width, 20.0).build();
        let s3 = StyleBuilder::new().set(width, 30.0).build();

        let base = Selector::default();
        let classy = Selector {
            type_tag: None,
            required_classes: IdSet::from_ids([ClassId(1)]),
            required_pseudos: IdSet::default(),
        };

        let sheet = StyleSheetBuilder::new()
            .rule(base.clone(), s1)
            .rule(base, s2.clone())
            .rule(classy, s3)
            .build();

        let classes = [ClassId(1)];
        let inputs = SelectorInputs::new(None, &classes, &[]);
        assert_eq!(sheet.get_value_ref(&inputs, width), Some(&30.0));

        let inputs2 = SelectorInputs::new(None, &[], &[]);
        // Same specificity (0), later rule wins.
        assert_eq!(sheet.get_value_ref(&inputs2, width), Some(&20.0));
    }

    #[test]
    fn cascade_origin_beats_specificity() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let base_style = StyleBuilder::new().set(width, 10.0).build();
        let override_style = StyleBuilder::new().set(width, 99.0).build();
        let rule_style = StyleBuilder::new().set(width, 50.0).build();

        let hover = Selector {
            type_tag: Some(TypeTag(1)),
            required_classes: IdSet::default(),
            required_pseudos: IdSet::from_ids([PseudoClassId(1)]),
        };

        let sheet = StyleSheetBuilder::new().rule(hover, rule_style).build();

        let cascade = StyleCascadeBuilder::new()
            .push_style(StyleOrigin::Base, base_style)
            .push_sheet(StyleOrigin::Sheet, sheet)
            .push_style(StyleOrigin::Override, override_style)
            .build();

        let pseudos = [PseudoClassId(1)];
        let inputs = SelectorInputs::new(Some(TypeTag(1)), &[], &pseudos);
        assert_eq!(cascade.get_value_ref(&inputs, width), Some(&99.0));
    }
}
