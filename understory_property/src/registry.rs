// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Global property registry.
//!
//! This module provides [`PropertyRegistry`] for registering and looking up
//! property metadata.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::{Any, TypeId};
use hashbrown::HashMap;
use understory_dirty::ChannelSet;

use crate::id::{Property, PropertyId};
use crate::metadata::PropertyMetadata;

/// A registration entry for a property.
///
/// This stores the property's name, type information, and metadata.
pub struct PropertyRegistration {
    name: &'static str,
    type_id: TypeId,
    metadata: Box<dyn ErasedMetadata>,
}

impl PropertyRegistration {
    /// Returns the property name.
    #[must_use]
    #[inline]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the [`TypeId`] of the property's value type.
    #[must_use]
    #[inline]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Returns the dirty channels affected by this property.
    #[must_use]
    #[inline]
    pub fn affects_channels(&self) -> ChannelSet {
        self.metadata.affects_channels()
    }

    /// Returns whether this property inherits from parents.
    #[must_use]
    #[inline]
    pub fn inherits(&self) -> bool {
        self.metadata.inherits()
    }
}

impl core::fmt::Debug for PropertyRegistration {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PropertyRegistration")
            .field("name", &self.name)
            .field("type_id", &self.type_id)
            .field("inherits", &self.inherits())
            .field("affects_channels", &self.affects_channels())
            .finish_non_exhaustive()
    }
}

/// A registry for dependency properties.
///
/// Properties are registered once at startup, and the registry provides
/// lookup by name or ID, as well as access to property metadata.
///
/// # Example
///
/// ```rust
/// use understory_property::{PropertyRegistry, PropertyMetadataBuilder};
/// use understory_dirty::Channel;
///
/// const LAYOUT: Channel = Channel::new(0);
///
/// let mut registry = PropertyRegistry::new();
///
/// let width = registry.register(
///     "Width",
///     PropertyMetadataBuilder::new(0.0_f64)
///         .affects_channels(LAYOUT.into_set())
///         .build()
/// );
///
/// assert_eq!(registry.name(width.id()), Some("Width"));
/// assert!(registry.affects_channels(width.id()).contains(LAYOUT));
/// ```
#[derive(Default)]
pub struct PropertyRegistry {
    properties: Vec<PropertyRegistration>,
    by_name: HashMap<&'static str, PropertyId>,
}

impl PropertyRegistry {
    /// Creates a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new property with the given name and metadata.
    ///
    /// Returns a type-safe [`Property<T>`] handle for accessing the property.
    ///
    /// # Panics
    ///
    /// Panics if a property with the same name is already registered,
    /// or if more than 65,536 properties are registered.
    pub fn register<T: Clone + 'static>(
        &mut self,
        name: &'static str,
        metadata: PropertyMetadata<T>,
    ) -> Property<T> {
        assert!(
            !self.by_name.contains_key(name),
            "Property '{name}' is already registered"
        );
        assert!(
            self.properties.len() < u16::MAX as usize,
            "Too many properties registered (max {})",
            u16::MAX
        );

        #[expect(clippy::cast_possible_truncation, reason = "checked above")]
        let id = PropertyId::new(self.properties.len() as u16);

        self.properties.push(PropertyRegistration {
            name,
            type_id: TypeId::of::<T>(),
            metadata: Box::new(metadata),
        });
        self.by_name.insert(name, id);

        Property::from_id(id)
    }

    /// Returns the number of registered properties.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.properties.len()
    }

    /// Returns `true` if no properties are registered.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }

    /// Looks up a property by name.
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<PropertyId> {
        self.by_name.get(name).copied()
    }

    /// Returns the name of a property.
    #[must_use]
    pub fn name(&self, id: PropertyId) -> Option<&'static str> {
        self.properties.get(id.index() as usize).map(|r| r.name)
    }

    /// Returns the registration for a property.
    #[must_use]
    pub fn get(&self, id: PropertyId) -> Option<&PropertyRegistration> {
        self.properties.get(id.index() as usize)
    }

    /// Returns the dirty channels affected by a property.
    #[must_use]
    pub fn affects_channels(&self, id: PropertyId) -> ChannelSet {
        self.properties
            .get(id.index() as usize)
            .map(|r| r.affects_channels())
            .unwrap_or_default()
    }

    /// Returns whether a property inherits from parents.
    #[must_use]
    pub fn inherits(&self, id: PropertyId) -> bool {
        self.properties
            .get(id.index() as usize)
            .is_some_and(|r| r.inherits())
    }

    /// Returns the metadata for a typed property.
    ///
    /// Returns `None` if the property is not registered or the type doesn't match.
    #[must_use]
    pub fn get_metadata<T: Clone + 'static>(
        &self,
        property: Property<T>,
    ) -> Option<&PropertyMetadata<T>> {
        self.properties
            .get(property.id().index() as usize)
            .and_then(|r| r.metadata.downcast_ref())
    }

    /// Returns an iterator over all registered properties.
    pub fn iter(&self) -> impl Iterator<Item = (PropertyId, &PropertyRegistration)> {
        self.properties.iter().enumerate().map(|(i, r)| {
            #[expect(clippy::cast_possible_truncation, reason = "index < len < u16::MAX")]
            (PropertyId::new(i as u16), r)
        })
    }
}

impl core::fmt::Debug for PropertyRegistry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PropertyRegistry")
            .field("count", &self.properties.len())
            .field("properties", &self.by_name.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Type-erased metadata trait for heterogeneous storage.
trait ErasedMetadata: Any {
    fn as_any(&self) -> &dyn Any;
    fn affects_channels(&self) -> ChannelSet;
    fn inherits(&self) -> bool;
}

impl<T: Clone + 'static> ErasedMetadata for PropertyMetadata<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn affects_channels(&self) -> ChannelSet {
        Self::affects_channels(self)
    }

    fn inherits(&self) -> bool {
        Self::inherits(self)
    }
}

impl dyn ErasedMetadata {
    fn downcast_ref<T: Clone + 'static>(&self) -> Option<&PropertyMetadata<T>> {
        self.as_any().downcast_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::PropertyMetadataBuilder;
    use alloc::{format, vec, vec::Vec};
    use understory_dirty::Channel;

    const LAYOUT: Channel = Channel::new(0);
    const PAINT: Channel = Channel::new(1);

    #[test]
    fn registry_new() {
        let registry = PropertyRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn registry_register() {
        let mut registry = PropertyRegistry::new();

        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
        assert_eq!(width.id().index(), 0);
    }

    #[test]
    fn registry_by_name() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        assert_eq!(registry.by_name("Width"), Some(width.id()));
        assert_eq!(registry.by_name("Height"), None);
    }

    #[test]
    fn registry_name() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        assert_eq!(registry.name(width.id()), Some("Width"));
        assert_eq!(registry.name(PropertyId::new(999)), None);
    }

    #[test]
    fn registry_affects_channels() {
        let mut registry = PropertyRegistry::new();

        let width = registry.register(
            "Width",
            PropertyMetadataBuilder::new(0.0_f64)
                .affects_channels(LAYOUT.into_set() | PAINT.into_set())
                .build(),
        );

        assert!(registry.affects_channels(width.id()).contains(LAYOUT));
        assert!(registry.affects_channels(width.id()).contains(PAINT));
    }

    #[test]
    fn registry_inherits() {
        let mut registry = PropertyRegistry::new();

        let font_size = registry.register(
            "FontSize",
            PropertyMetadataBuilder::new(12.0_f64)
                .inherits(true)
                .build(),
        );

        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        assert!(registry.inherits(font_size.id()));
        assert!(!registry.inherits(width.id()));
    }

    #[test]
    fn registry_get_metadata() {
        let mut registry = PropertyRegistry::new();

        let width = registry.register(
            "Width",
            PropertyMetadataBuilder::new(100.0_f64)
                .inherits(true)
                .build(),
        );

        let metadata = registry.get_metadata(width).unwrap();
        assert_eq!(metadata.default_value(), &100.0);
        assert!(metadata.inherits());
    }

    #[test]
    fn registry_iter() {
        let mut registry = PropertyRegistry::new();
        registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
        registry.register("Height", PropertyMetadataBuilder::new(0.0_f64).build());

        let names: Vec<_> = registry.iter().map(|(_, r)| r.name()).collect();
        assert_eq!(names, vec!["Width", "Height"]);
    }

    #[test]
    #[should_panic(expected = "already registered")]
    fn registry_duplicate_name() {
        let mut registry = PropertyRegistry::new();
        registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
        registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
    }

    #[test]
    fn registry_debug() {
        let mut registry = PropertyRegistry::new();
        registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let debug = format!("{:?}", registry);
        assert!(debug.contains("PropertyRegistry"));
        assert!(debug.contains("Width"));
    }
}
