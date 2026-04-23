// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared style definitions.
//!
//! This module provides [`Style`], a shared collection of property setters
//! that can be referenced by multiple elements.
//!
//! Styles can now carry either:
//! - literal property values, or
//! - theme resource references via [`StyleBuilder::set_resource`].
//!
//! That second form is useful when selectors should choose *which semantic
//! token applies* while the active theme still owns the concrete value.

use alloc::rc::Rc;
use alloc::vec::Vec;

use understory_property::{ErasedValue, Property, PropertyId};

use crate::ResourceKey;

#[derive(Clone, Debug)]
enum StyleEntryValue {
    Literal(ErasedValue),
    Resource(ResourceKey),
}

/// One style-layer entry for a property.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StyleValueRef<'a, T> {
    /// A concrete typed value stored directly in the style.
    Value(&'a T),
    /// A theme resource key to be resolved later.
    Resource(ResourceKey),
}

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
    entries: Vec<(PropertyId, StyleEntryValue)>,
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
        match self.value_ref(property)? {
            StyleValueRef::Value(value) => Some(value),
            StyleValueRef::Resource(_) => None,
        }
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

    /// Returns the theme resource key for a property, if this style references one.
    #[must_use]
    pub fn resource_key<T: Clone + 'static>(&self, property: Property<T>) -> Option<ResourceKey> {
        match self.value_ref(property)? {
            StyleValueRef::Value(_) => None,
            StyleValueRef::Resource(key) => Some(key),
        }
    }

    /// Returns an iterator over the property IDs set in this style.
    pub fn property_ids(&self) -> impl Iterator<Item = PropertyId> + '_ {
        self.inner.entries.iter().map(|(id, _)| *id)
    }

    /// Returns the raw style entry for a property, if present.
    ///
    /// This is primarily useful for higher-level cascade machinery that needs
    /// to preserve either a literal property value or a theme resource
    /// reference before final resolution.
    #[must_use]
    pub fn value_ref<T: Clone + 'static>(
        &self,
        property: Property<T>,
    ) -> Option<StyleValueRef<'_, T>> {
        let idx = self
            .inner
            .entries
            .binary_search_by_key(&property.id(), |(id, _)| *id)
            .ok()?;
        match &self.inner.entries[idx].1 {
            StyleEntryValue::Literal(value) => value.downcast_ref().map(StyleValueRef::Value),
            StyleEntryValue::Resource(key) => Some(StyleValueRef::Resource(*key)),
        }
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
    entries: Vec<(PropertyId, StyleEntryValue)>,
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
        let entry = StyleEntryValue::Literal(ErasedValue::new(value));

        match self.entries.binary_search_by_key(&id, |(pid, _)| *pid) {
            Ok(idx) => {
                self.entries[idx].1 = entry;
            }
            Err(idx) => {
                self.entries.insert(idx, (id, entry));
            }
        }
        self
    }

    /// Sets a property to resolve from a theme resource key.
    ///
    /// Use this when the style should select a semantic theme token rather than
    /// pinning a literal value directly into the style.
    #[must_use]
    pub fn set_resource<T: Clone + 'static>(
        mut self,
        property: Property<T>,
        resource_key: ResourceKey,
    ) -> Self {
        let id = property.id();
        let entry = StyleEntryValue::Resource(resource_key);

        match self.entries.binary_search_by_key(&id, |(pid, _)| *pid) {
            Ok(idx) => {
                self.entries[idx].1 = entry;
            }
            Err(idx) => {
                self.entries.insert(idx, (id, entry));
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
    fn style_resource_property() {
        let (_, width, _) = setup_registry();
        let resource = ResourceKey::new(42);

        let style = StyleBuilder::new().set_resource(width, resource).build();

        assert!(style.contains(width));
        assert_eq!(style.get(width), None);
        assert_eq!(style.resource_key(width), Some(resource));
        assert_eq!(
            style.value_ref(width),
            Some(StyleValueRef::Resource(resource))
        );
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
        let StyleEntryValue::Literal(value) = &style.inner.entries[0].1 else {
            panic!("expected literal style entry");
        };
        let wrong: Option<&i32> = value.downcast_ref();
        assert!(wrong.is_none());
    }
}
