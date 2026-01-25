// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared style definitions.
//!
//! This module provides [`Style`], a shared collection of property setters
//! that can be referenced by multiple elements.

use alloc::rc::Rc;
use alloc::vec::Vec;

use understory_property::{ErasedValue, Property, PropertyId};

/// A shared, immutable collection of property setters.
///
/// Styles store property values once and can be shared across many elements.
/// This follows `WinUI`'s `OptimizedStyle` pattern for memory efficiency—rather
/// than storing style values per-element, elements hold a reference to a
/// shared style.
///
/// Styles are immutable after creation. Use [`StyleBuilder`] to construct them.
///
/// # Memory Layout
///
/// Internally, `Style` wraps an `Rc<StyleData>`, making cloning cheap (just
/// incrementing a reference count). The actual property values are stored once
/// in a sorted vector, similar to `PropertyStore`.
///
/// # Example
///
/// ```rust
/// use understory_style::{Style, StyleBuilder};
/// use understory_property::{PropertyMetadataBuilder, PropertyRegistry};
///
/// let mut registry = PropertyRegistry::new();
/// let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
///
/// let style = StyleBuilder::new()
///     .set(width, 100.0)
///     .build();
///
/// // Style can be cloned cheaply (`Rc`)
/// let style2 = style.clone();
///
/// assert_eq!(style.get(width), Some(&100.0));
/// assert_eq!(style2.get(width), Some(&100.0));
/// ```
#[derive(Clone, Debug)]
pub struct Style {
    inner: Rc<StyleData>,
}

/// Internal storage for style property values.
#[derive(Debug, Default)]
struct StyleData {
    /// Sorted by `PropertyId` for binary search lookup.
    entries: Vec<(PropertyId, ErasedValue)>,
}

impl Style {
    /// Returns `true` if this style has no property setters.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.entries.is_empty()
    }

    /// Returns the number of property setters in this style.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.entries.len()
    }

    /// Gets the value for a property, if set in this style.
    #[must_use]
    #[inline]
    pub fn get<T: Clone + 'static>(&self, property: Property<T>) -> Option<&T> {
        self.inner
            .entries
            .binary_search_by_key(&property.id(), |(id, _)| *id)
            .ok()
            .and_then(|idx| self.inner.entries[idx].1.downcast_ref())
    }

    /// Returns `true` if this style has a value for the property.
    #[must_use]
    #[inline]
    pub fn contains<T: Clone + 'static>(&self, property: Property<T>) -> bool {
        self.inner
            .entries
            .binary_search_by_key(&property.id(), |(id, _)| *id)
            .is_ok()
    }

    /// Returns an iterator over the property IDs set in this style.
    pub fn property_ids(&self) -> impl Iterator<Item = PropertyId> + '_ {
        self.inner.entries.iter().map(|(id, _)| *id)
    }
}

/// Builder for constructing [`Style`] instances.
///
/// # Example
///
/// ```rust
/// use understory_style::StyleBuilder;
/// use understory_property::{PropertyMetadataBuilder, PropertyRegistry};
///
/// let mut registry = PropertyRegistry::new();
/// let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
/// let height = registry.register("Height", PropertyMetadataBuilder::new(0.0_f64).build());
///
/// let style = StyleBuilder::new()
///     .set(width, 100.0)
///     .set(height, 50.0)
///     .build();
///
/// assert_eq!(style.get(width), Some(&100.0));
/// assert_eq!(style.get(height), Some(&50.0));
/// ```
#[derive(Debug, Default)]
pub struct StyleBuilder {
    entries: Vec<(PropertyId, ErasedValue)>,
}

impl StyleBuilder {
    /// Creates a new empty style builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a property value in the style.
    ///
    /// If the property was already set, the value is replaced.
    #[must_use]
    pub fn set<T: Clone + 'static>(mut self, property: Property<T>, value: T) -> Self {
        let id = property.id();
        let erased = ErasedValue::new(value);

        match self.entries.binary_search_by_key(&id, |(pid, _)| *pid) {
            Ok(idx) => {
                self.entries[idx].1 = erased;
            }
            Err(idx) => {
                self.entries.insert(idx, (id, erased));
            }
        }
        self
    }

    /// Builds the style.
    #[must_use]
    pub fn build(self) -> Style {
        Style {
            inner: Rc::new(StyleData {
                entries: self.entries,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use understory_property::{PropertyMetadataBuilder, PropertyRegistry};

    fn setup_registry() -> (PropertyRegistry, Property<f64>, Property<i32>) {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
        let count = registry.register("Count", PropertyMetadataBuilder::new(0_i32).build());
        (registry, width, count)
    }

    #[test]
    fn style_empty() {
        let style = StyleBuilder::new().build();
        assert!(style.is_empty());
        assert_eq!(style.len(), 0);
    }

    #[test]
    fn style_single_property() {
        let (_, width, _) = setup_registry();

        let style = StyleBuilder::new().set(width, 100.0).build();

        assert!(!style.is_empty());
        assert_eq!(style.len(), 1);
        assert_eq!(style.get(width), Some(&100.0));
    }

    #[test]
    fn style_multiple_properties() {
        let (_, width, count) = setup_registry();

        let style = StyleBuilder::new().set(width, 100.0).set(count, 42).build();

        assert_eq!(style.len(), 2);
        assert_eq!(style.get(width), Some(&100.0));
        assert_eq!(style.get(count), Some(&42));
    }

    #[test]
    fn style_replace_value() {
        let (_, width, _) = setup_registry();

        let style = StyleBuilder::new()
            .set(width, 100.0)
            .set(width, 200.0)
            .build();

        assert_eq!(style.len(), 1);
        assert_eq!(style.get(width), Some(&200.0));
    }

    #[test]
    fn style_contains() {
        let (_, width, count) = setup_registry();

        let style = StyleBuilder::new().set(width, 100.0).build();

        assert!(style.contains(width));
        assert!(!style.contains(count));
    }

    #[test]
    fn style_clone_is_cheap() {
        let (_, width, _) = setup_registry();

        let style = StyleBuilder::new().set(width, 100.0).build();
        let style2 = style.clone();

        // Both reference the same data
        assert_eq!(style.get(width), Some(&100.0));
        assert_eq!(style2.get(width), Some(&100.0));

        // Rc makes this cheap
        assert!(Rc::ptr_eq(&style.inner, &style2.inner));
    }

    #[test]
    fn style_property_ids() {
        let (_, width, count) = setup_registry();

        let style = StyleBuilder::new().set(count, 42).set(width, 100.0).build();

        let ids: Vec<_> = style.property_ids().collect();
        assert_eq!(ids.len(), 2);
        // Should be sorted by PropertyId
        assert!(ids[0].index() < ids[1].index());
    }

    #[test]
    fn style_get_wrong_type_returns_none() {
        let (_, width, _) = setup_registry();

        let style = StyleBuilder::new().set(width, 100.0).build();

        // width is f64, trying to get as i32 fails
        let wrong: Option<&i32> = style.inner.entries[0].1.downcast_ref();
        assert!(wrong.is_none());
    }
}
