// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dependency object traits.
//!
//! This module provides the [`DependencyObject`] trait for objects that can
//! have dependency properties, and [`DependencyObjectExt`] for convenient
//! property access methods including inheritance resolution.

use understory_dirty::ChannelSet;

use crate::id::Property;
use crate::registry::PropertyRegistry;
use crate::store::PropertyStore;

/// A lookup mechanism for walking parent chains for inheritance.
///
/// Given an object key, returns its [`PropertyStore`] and its parent key.
///
/// This is used by inheritance walking helpers such as [`walk_inherited`] and
/// [`walk_inherited_ref`], and by higher-level resolution layers.
pub trait ParentLookup<'a, K: Copy + Eq + 'a> {
    /// Looks up the store and parent key for `key`.
    fn lookup(&self, key: K) -> Option<(&'a PropertyStore<K>, Option<K>)>;
}

impl<'a, K, F> ParentLookup<'a, K> for F
where
    K: Copy + Eq + 'a,
    F: Fn(K) -> Option<(&'a PropertyStore<K>, Option<K>)>,
{
    #[inline]
    fn lookup(&self, key: K) -> Option<(&'a PropertyStore<K>, Option<K>)> {
        self(key)
    }
}

/// Walks the parent chain looking for an inherited value.
///
/// This is the canonical implementation of inheritance walking, checking
/// Animation → Local at each ancestor. Used by [`DependencyObjectExt::get_inherited`]
/// and can be reused by higher-level crates (e.g., `understory_style`).
///
/// Returns the first value found, or `None` if no ancestor has the property set.
///
/// # Arguments
///
/// * `start_key` - The key to start walking from (typically `parent_key()`)
/// * `property` - The property to look for
/// * `store_lookup` - Function returning (store, `parent_key`) for a given key
pub fn walk_inherited<'a, K, T, F>(
    mut current_key: Option<K>,
    property: Property<T>,
    store_lookup: &F,
) -> Option<T>
where
    K: Copy + Eq + 'a,
    T: Clone + 'static,
    F: ParentLookup<'a, K> + ?Sized,
{
    while let Some(key) = current_key {
        if let Some((parent_store, next_parent)) = store_lookup.lookup(key) {
            // Check parent's values (Animation > Local)
            if let Some(value) = parent_store.get_animation(property) {
                return Some(value.clone());
            }
            if let Some(value) = parent_store.get_local(property) {
                return Some(value.clone());
            }
            current_key = next_parent;
        } else {
            break;
        }
    }
    None
}

/// Walks the parent chain looking for an inherited value, returning a reference.
///
/// This is the borrowed variant of [`walk_inherited`]. It checks Animation → Local at each
/// ancestor and returns the first value found.
///
/// Returns `None` if no ancestor has the property set.
///
/// # Arguments
///
/// * `start_key` - The key to start walking from (typically `parent_key()`)
/// * `property` - The property to look for
/// * `store_lookup` - Function returning (store, `parent_key`) for a given key
pub fn walk_inherited_ref<'a, K, T, F>(
    mut current_key: Option<K>,
    property: Property<T>,
    store_lookup: &F,
) -> Option<&'a T>
where
    K: Copy + Eq + 'a,
    T: Clone + 'static,
    F: ParentLookup<'a, K> + ?Sized,
{
    while let Some(key) = current_key {
        if let Some((parent_store, next_parent)) = store_lookup.lookup(key) {
            // Check parent's values (Animation > Local)
            if let Some(value) = parent_store.get_animation(property) {
                return Some(value);
            }
            if let Some(value) = parent_store.get_local(property) {
                return Some(value);
            }
            current_key = next_parent;
        } else {
            break;
        }
    }
    None
}

/// A trait for objects that can have dependency properties.
///
/// This trait provides access to the object's property store and key,
/// enabling the extension methods in [`DependencyObjectExt`].
///
/// # Example
///
/// ```rust
/// use understory_property::{DependencyObject, PropertyStore};
///
/// struct MyElement {
///     key: u32,
///     parent: Option<u32>,
///     store: PropertyStore<u32>,
/// }
///
/// impl DependencyObject<u32> for MyElement {
///     fn property_store(&self) -> &PropertyStore<u32> {
///         &self.store
///     }
///
///     fn property_store_mut(&mut self) -> &mut PropertyStore<u32> {
///         &mut self.store
///     }
///
///     fn key(&self) -> u32 {
///         self.key
///     }
///
///     fn parent_key(&self) -> Option<u32> {
///         self.parent
///     }
/// }
/// ```
pub trait DependencyObject<K: Copy + Eq> {
    /// Returns a reference to the object's property store.
    fn property_store(&self) -> &PropertyStore<K>;

    /// Returns a mutable reference to the object's property store.
    fn property_store_mut(&mut self) -> &mut PropertyStore<K>;

    /// Returns the key that identifies this object.
    fn key(&self) -> K;

    /// Returns the parent's key, if this object has a parent.
    ///
    /// This is used for property inheritance resolution.
    fn parent_key(&self) -> Option<K>;
}

/// Extension methods for [`DependencyObject`].
///
/// These methods provide convenient access to property values.
pub trait DependencyObjectExt<K: Copy + Eq>: DependencyObject<K> {
    /// Gets the local value only.
    ///
    /// Returns `None` if no local value is set.
    fn get_local_value<'a, T: Clone + 'static>(&'a self, property: Property<T>) -> Option<&'a T>
    where
        K: 'a,
    {
        self.property_store().get_local(property)
    }

    /// Gets the animation value only.
    ///
    /// Returns `None` if no animation value is set.
    fn get_animation_value<'a, T: Clone + 'static>(&'a self, property: Property<T>) -> Option<&'a T>
    where
        K: 'a,
    {
        self.property_store().get_animation(property)
    }

    /// Gets the effective local value (Animation → Local → default).
    ///
    /// Does **not** handle style or inheritance—those belong to higher-level APIs.
    fn get_effective_local<T: Clone + 'static>(
        &self,
        property: Property<T>,
        registry: &PropertyRegistry,
    ) -> T {
        self.property_store()
            .get_effective_local(property, registry)
    }

    /// Gets the effective local value (Animation → Local → default), borrowed.
    ///
    /// Does **not** handle style or inheritance—those belong to higher-level APIs.
    ///
    /// # Panics
    ///
    /// Panics if the property is not registered in the registry.
    fn get_effective_local_ref<'a, T: Clone + 'static>(
        &'a self,
        property: Property<T>,
        registry: &'a PropertyRegistry,
    ) -> &'a T
    where
        K: 'a,
    {
        self.property_store()
            .get_effective_local_ref(property, registry)
    }

    /// Gets the effective value with inheritance resolution.
    ///
    /// Resolution order:
    /// 1. This object's Animation value
    /// 2. This object's Local value
    /// 3. If property inherits: walk parent chain (Animation → Local at each level)
    /// 4. Registry default
    ///
    /// # Arguments
    ///
    /// * `property` - The property to get
    /// * `registry` - The property registry containing metadata
    /// * `store_lookup` - Returns (`PropertyStore`, `parent_key`) for a given key
    ///
    /// # Example
    ///
    /// ```rust
    /// use understory_property::{
    ///     DependencyObject, DependencyObjectExt, PropertyStore,
    ///     PropertyMetadataBuilder, PropertyRegistry,
    /// };
    /// use std::collections::HashMap;
    ///
    /// let mut registry = PropertyRegistry::new();
    /// let font_size = registry.register(
    ///     "FontSize",
    ///     PropertyMetadataBuilder::new(12.0_f64)
    ///         .inherits(true)
    ///         .build()
    /// );
    ///
    /// struct Element { key: u32, parent: Option<u32>, store: PropertyStore<u32> }
    /// impl DependencyObject<u32> for Element {
    ///     fn property_store(&self) -> &PropertyStore<u32> { &self.store }
    ///     fn property_store_mut(&mut self) -> &mut PropertyStore<u32> { &mut self.store }
    ///     fn key(&self) -> u32 { self.key }
    ///     fn parent_key(&self) -> Option<u32> { self.parent }
    /// }
    ///
    /// let mut parent = Element { key: 1, parent: None, store: PropertyStore::new(1) };
    /// let child = Element { key: 2, parent: Some(1), store: PropertyStore::new(2) };
    ///
    /// parent.store.set_local(font_size, 16.0);
    ///
    /// let elements: HashMap<u32, &Element> = [(1, &parent), (2, &child)].into_iter().collect();
    ///
    /// let value = child.get_inherited(font_size, &registry, &|key| {
    ///     elements.get(&key).map(|e| (e.property_store(), e.parent_key()))
    /// });
    /// assert_eq!(value, 16.0);
    /// ```
    fn get_inherited<'a, T, F>(
        &'a self,
        property: Property<T>,
        registry: &PropertyRegistry,
        store_lookup: &F,
    ) -> T
    where
        K: 'a,
        T: Clone + 'static,
        F: ParentLookup<'a, K> + ?Sized,
    {
        // Check our values first (Animation > Local)
        if let Some(value) = self.property_store().get_animation(property) {
            return value.clone();
        }
        if let Some(value) = self.property_store().get_local(property) {
            return value.clone();
        }

        // Check inheritance chain if property inherits
        if let Some(metadata) = registry.get_metadata::<T>(property) {
            if metadata.inherits()
                && let Some(value) = walk_inherited(self.parent_key(), property, store_lookup)
            {
                return value;
            }
            return metadata.default_value().clone();
        }

        panic!("Property {:?} not found in registry", property.id());
    }

    /// Gets the effective value with inheritance resolution (borrowed).
    ///
    /// Resolution order:
    /// 1. This object's Animation value
    /// 2. This object's Local value
    /// 3. If property inherits: walk parent chain (Animation → Local at each level)
    /// 4. Registry default
    ///
    /// # Panics
    ///
    /// Panics if the property is not registered in the registry.
    fn get_inherited_ref<'a, T, F>(
        &'a self,
        property: Property<T>,
        registry: &'a PropertyRegistry,
        store_lookup: &F,
    ) -> &'a T
    where
        K: 'a,
        T: Clone + 'static,
        F: ParentLookup<'a, K> + ?Sized,
    {
        // Check our values first (Animation > Local)
        if let Some(value) = self.property_store().get_animation(property) {
            return value;
        }
        if let Some(value) = self.property_store().get_local(property) {
            return value;
        }

        // Check inheritance chain if property inherits
        if let Some(metadata) = registry.get_metadata::<T>(property) {
            if metadata.inherits()
                && let Some(value) = walk_inherited_ref(self.parent_key(), property, store_lookup)
            {
                return value;
            }
            return metadata.default_value();
        }

        panic!("Property {:?} not found in registry", property.id());
    }

    /// Sets the local value.
    fn set_local<T: Clone + 'static>(&mut self, property: Property<T>, value: T) {
        self.property_store_mut().set_local(property, value);
    }

    /// Sets the animation value.
    fn set_animation<T: Clone + 'static>(&mut self, property: Property<T>, value: T) {
        self.property_store_mut().set_animation(property, value);
    }

    /// Sets the local value with coercion and callbacks.
    ///
    /// This is the "blessed" API for setting property values. It:
    /// 1. Coerces the value using the property's coerce callback (if any)
    /// 2. Stores the value at the local layer
    /// 3. Calls the property's changed callback (if any)
    /// 4. Returns the affected channels for dirty marking
    ///
    /// The caller is responsible for marking dirty channels:
    /// ```ignore
    /// let channels = element.set_local_notifying(width, 100.0, &registry);
    /// for channel in channels {
    ///     tracker.mark(element.key(), channel);
    /// }
    /// ```
    ///
    /// # Example
    ///
    /// ```rust
    /// use understory_property::{
    ///     DependencyObject, DependencyObjectExt, PropertyStore,
    ///     PropertyMetadataBuilder, PropertyRegistry,
    /// };
    /// use understory_dirty::Channel;
    ///
    /// const LAYOUT: Channel = Channel::new(0);
    ///
    /// let mut registry = PropertyRegistry::new();
    /// let width = registry.register(
    ///     "Width",
    ///     PropertyMetadataBuilder::new(0.0_f64)
    ///         .affects_channels(LAYOUT.into_set())
    ///         .coerce(|v| v.max(0.0))
    ///         .build()
    /// );
    ///
    /// struct Element { key: u32, parent: Option<u32>, store: PropertyStore<u32> }
    /// impl DependencyObject<u32> for Element {
    ///     fn property_store(&self) -> &PropertyStore<u32> { &self.store }
    ///     fn property_store_mut(&mut self) -> &mut PropertyStore<u32> { &mut self.store }
    ///     fn key(&self) -> u32 { self.key }
    ///     fn parent_key(&self) -> Option<u32> { self.parent }
    /// }
    ///
    /// let mut element = Element { key: 1, parent: None, store: PropertyStore::new(1) };
    ///
    /// // Set value - coerces, stores, returns affected channels
    /// let channels = element.set_local_notifying(width, -10.0, &registry);
    ///
    /// // Value was coerced to 0.0
    /// assert_eq!(element.property_store().get_local(width), Some(&0.0));
    ///
    /// // Caller marks dirty
    /// assert!(channels.contains(LAYOUT));
    /// ```
    fn set_local_notifying<T: Clone + 'static>(
        &mut self,
        property: Property<T>,
        value: T,
        registry: &PropertyRegistry,
    ) -> ChannelSet {
        let metadata = registry.get_metadata(property);

        // 1. Coerce the value
        let value = match metadata {
            Some(m) => m.coerce(value),
            None => value,
        };

        // 2. Only clone old value if we have a callback that needs it
        let old_value = metadata
            .filter(|m| m.has_changed_callback())
            .and_then(|_| self.property_store().get_local(property).cloned());

        // 3. Store the value
        let stored_value = self.property_store_mut().set_local(property, value);

        // 4. Call changed callback
        if let Some(m) = metadata {
            m.on_changed(old_value.as_ref(), stored_value);
        }

        // 5. Return affected channels
        metadata.map(|m| m.affects_channels()).unwrap_or_default()
    }

    /// Clears the local value.
    ///
    /// Returns `true` if a value was removed.
    fn clear_local<T: Clone + 'static>(&mut self, property: Property<T>) -> bool {
        self.property_store_mut().clear_local(property)
    }

    /// Clears the animation value.
    ///
    /// Returns `true` if a value was removed.
    fn clear_animation<T: Clone + 'static>(&mut self, property: Property<T>) -> bool {
        self.property_store_mut().clear_animation(property)
    }

    /// Returns `true` if the property has any value (local or animation).
    fn has_value<T: Clone + 'static>(&self, property: Property<T>) -> bool {
        self.property_store().has_value(property)
    }

    /// Returns `true` if the property has a local value.
    fn has_local<T: Clone + 'static>(&self, property: Property<T>) -> bool {
        self.property_store().has_local(property)
    }

    /// Returns `true` if the property has an animation value.
    fn has_animation<T: Clone + 'static>(&self, property: Property<T>) -> bool {
        self.property_store().has_animation(property)
    }
}

// Blanket implementation for all DependencyObject types
impl<K: Copy + Eq, T: DependencyObject<K>> DependencyObjectExt<K> for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PropertyRegistry;
    use crate::metadata::PropertyMetadataBuilder;

    struct TestElement {
        key: u32,
        parent: Option<u32>,
        store: PropertyStore<u32>,
    }

    impl TestElement {
        fn new(key: u32, parent: Option<u32>) -> Self {
            Self {
                key,
                parent,
                store: PropertyStore::new(key),
            }
        }
    }

    impl DependencyObject<u32> for TestElement {
        fn property_store(&self) -> &PropertyStore<u32> {
            &self.store
        }

        fn property_store_mut(&mut self) -> &mut PropertyStore<u32> {
            &mut self.store
        }

        fn key(&self) -> u32 {
            self.key
        }

        fn parent_key(&self) -> Option<u32> {
            self.parent
        }
    }

    fn setup_registry() -> (PropertyRegistry, Property<f64>) {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
        (registry, width)
    }

    #[test]
    fn ext_get_set_local() {
        let (_, width) = setup_registry();
        let mut element = TestElement::new(1, None);

        assert!(element.get_local_value(width).is_none());

        element.set_local(width, 100.0);
        assert_eq!(element.get_local_value(width), Some(&100.0));
    }

    #[test]
    fn ext_get_set_animation() {
        let (_, width) = setup_registry();
        let mut element = TestElement::new(1, None);

        assert!(element.get_animation_value(width).is_none());

        element.set_animation(width, 200.0);
        assert_eq!(element.get_animation_value(width), Some(&200.0));
    }

    #[test]
    fn ext_animation_precedence() {
        let (registry, width) = setup_registry();
        let mut element = TestElement::new(1, None);

        element.set_local(width, 100.0);
        element.set_animation(width, 200.0);

        // Animation wins
        assert_eq!(element.get_effective_local(width, &registry), 200.0);

        // Clear animation
        element.clear_animation(width);
        assert_eq!(element.get_effective_local(width, &registry), 100.0);
    }

    #[test]
    fn ext_effective_local_ref_borrows_from_store() {
        let (registry, width) = setup_registry();
        let mut element = TestElement::new(1, None);

        element.set_local(width, 100.0);
        element.set_animation(width, 200.0);

        let value_ref = element.get_effective_local_ref(width, &registry);
        assert!(core::ptr::eq(
            value_ref,
            element.property_store().get_animation(width).unwrap()
        ));
    }

    #[test]
    fn ext_clear_local() {
        let (_, width) = setup_registry();
        let mut element = TestElement::new(1, None);

        element.set_local(width, 100.0);
        assert!(element.has_local(width));

        assert!(element.clear_local(width));
        assert!(!element.has_local(width));
    }

    #[test]
    fn dependency_object_key() {
        let element = TestElement::new(42, Some(1));
        assert_eq!(element.key(), 42);
        assert_eq!(element.parent_key(), Some(1));
    }

    #[test]
    fn ext_inheritance_from_parent() {
        let mut registry = PropertyRegistry::new();
        let font_size = registry.register(
            "FontSize",
            PropertyMetadataBuilder::new(12.0_f64)
                .inherits(true)
                .build(),
        );

        let mut parent = TestElement::new(1, None);
        let child = TestElement::new(2, Some(1));

        parent.set_local(font_size, 16.0);

        let elements: alloc::collections::BTreeMap<u32, &TestElement> =
            [(1, &parent), (2, &child)].into_iter().collect();

        let value = child.get_inherited(font_size, &registry, &|key| {
            elements
                .get(&key)
                .map(|e| (e.property_store(), e.parent_key()))
        });
        assert_eq!(value, 16.0);
    }

    #[test]
    fn ext_inheritance_from_parent_ref() {
        let mut registry = PropertyRegistry::new();
        let font_size = registry.register(
            "FontSize",
            PropertyMetadataBuilder::new(12.0_f64)
                .inherits(true)
                .build(),
        );

        let mut parent = TestElement::new(1, None);
        let child = TestElement::new(2, Some(1));

        parent.set_local(font_size, 16.0);

        let elements: alloc::collections::BTreeMap<u32, &TestElement> =
            [(1, &parent), (2, &child)].into_iter().collect();

        let value_ref = child.get_inherited_ref(font_size, &registry, &|key| {
            elements
                .get(&key)
                .map(|e| (e.property_store(), e.parent_key()))
        });

        assert!(core::ptr::eq(
            value_ref,
            parent.property_store().get_local(font_size).unwrap()
        ));
        assert_eq!(*value_ref, 16.0);
    }

    #[test]
    fn ext_local_overrides_inherited() {
        let mut registry = PropertyRegistry::new();
        let font_size = registry.register(
            "FontSize",
            PropertyMetadataBuilder::new(12.0_f64)
                .inherits(true)
                .build(),
        );

        let mut parent = TestElement::new(1, None);
        let mut child = TestElement::new(2, Some(1));

        parent.set_local(font_size, 16.0);
        child.set_local(font_size, 20.0);

        let elements: alloc::collections::BTreeMap<u32, &TestElement> =
            [(1, &parent), (2, &child)].into_iter().collect();

        let value = child.get_inherited(font_size, &registry, &|key| {
            elements
                .get(&key)
                .map(|e| (e.property_store(), e.parent_key()))
        });
        assert_eq!(value, 20.0);
    }

    #[test]
    fn ext_non_inherited_uses_default() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register(
            "Width",
            PropertyMetadataBuilder::new(100.0_f64)
                .inherits(false)
                .build(),
        );

        let mut parent = TestElement::new(1, None);
        let child = TestElement::new(2, Some(1));

        parent.set_local(width, 200.0);

        let elements: alloc::collections::BTreeMap<u32, &TestElement> =
            [(1, &parent), (2, &child)].into_iter().collect();

        let value = child.get_inherited(width, &registry, &|key| {
            elements
                .get(&key)
                .map(|e| (e.property_store(), e.parent_key()))
        });
        assert_eq!(value, 100.0); // Default, not parent's 200.0
    }

    #[test]
    fn ext_set_local_notifying() {
        use understory_dirty::Channel;

        const LAYOUT: Channel = Channel::new(0);

        let mut registry = PropertyRegistry::new();
        let width = registry.register(
            "Width",
            PropertyMetadataBuilder::new(0.0_f64)
                .affects_channels(LAYOUT.into_set())
                .coerce(|v| v.max(0.0))
                .build(),
        );

        let mut element = TestElement::new(1, None);

        let channels = element.set_local_notifying(width, -10.0, &registry);

        // Value was coerced
        assert_eq!(element.get_local_value(width), Some(&0.0));

        // Returns affected channels
        assert!(channels.contains(LAYOUT));
    }
}
