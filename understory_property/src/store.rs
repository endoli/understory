// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Per-object sparse property storage.
//!
//! This module provides [`PropertyStore`] for storing property values on objects,
//! using sparse storage to minimize memory for objects with few properties set.
//!
//! # Implementation
//!
//! Following the `WinUI` approach, we use a sorted vector with binary search rather
//! than a hash map. This provides:
//!
//! - Better cache locality (contiguous memory)
//! - Lower memory overhead (no hash buckets)
//! - O(log n) lookup, which is fast for typical property counts (5-20)
//! - Inline storage for small property sets via `SmallVec`
//!
//! # Scope
//!
//! `PropertyStore` handles **local storage only** - just Local and Animation values.
//! Style resolution and inheritance belong to higher-level layers (see `understory_style`).

use alloc::vec::Vec;
use smallvec::SmallVec;

use crate::id::{Property, PropertyId};
use crate::registry::PropertyRegistry;
use crate::value::ErasedValue;

/// Default inline capacity for property entries.
///
/// Most UI objects have fewer than 8 non-default properties set,
/// so this avoids heap allocation in the common case.
const INLINE_CAPACITY: usize = 8;

/// Per-object sparse storage for property values.
///
/// Stores Local and Animation values only. Style/theme resolution and inheritance
/// are handled by higher-level APIs (see `understory_style`).
///
/// # Storage Strategy
///
/// Uses a sorted `SmallVec` with binary search, following the `WinUI` `vector_map`
/// approach. This provides O(log n) lookup with excellent cache locality.
/// The first 8 properties are stored inline without heap allocation.
///
/// # Precedence
///
/// Animation values take precedence over Local values:
/// - `get_effective_local` returns: Animation → Local → registry default
///
/// # Example
///
/// ```rust
/// use understory_property::{PropertyStore, PropertyMetadataBuilder, PropertyRegistry};
///
/// let mut registry = PropertyRegistry::new();
/// let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
///
/// let mut store = PropertyStore::<u32>::new(1);
///
/// // No value set - uses default
/// assert!(store.get_local(width).is_none());
///
/// // Set local value
/// store.set_local(width, 100.0);
/// assert_eq!(store.get_local(width), Some(&100.0));
///
/// // Animation overrides local
/// store.set_animation(width, 200.0);
/// let effective = store.get_effective_local(width, &registry);
/// assert_eq!(effective, 200.0);
/// ```
#[derive(Debug)]
pub struct PropertyStore<K> {
    /// Local values, sorted by [`PropertyId`] for binary search lookup.
    local_entries: SmallVec<[(PropertyId, ErasedValue); INLINE_CAPACITY]>,
    /// Animation values, sorted by [`PropertyId`] for binary search lookup.
    ///
    /// Stored out-of-line so that objects with no animation values pay minimal
    /// per-object overhead.
    animation_entries: Vec<(PropertyId, ErasedValue)>,
    owner: K,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Layer {
    Local,
    Animation,
}

impl<K: Copy + Eq> PropertyStore<K> {
    /// Creates a new property store for the given owner key.
    #[must_use]
    pub fn new(owner: K) -> Self {
        Self {
            local_entries: SmallVec::new(),
            animation_entries: Vec::new(),
            owner,
        }
    }

    /// Returns the owner key of this store.
    #[must_use]
    #[inline]
    pub fn owner(&self) -> K {
        self.owner
    }

    /// Returns `true` if no properties have explicit values set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.local_entries.is_empty() && self.animation_entries.is_empty()
    }

    /// Returns the number of properties with explicit values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.property_ids().count()
    }

    /// Returns the property IDs that have values set.
    pub fn property_ids(&self) -> impl Iterator<Item = PropertyId> + '_ {
        PropertyIds {
            local: self.local_entries.as_slice(),
            animation: self.animation_entries.as_slice(),
            local_i: 0,
            animation_i: 0,
        }
    }

    /// Binary search for a local entry by property ID.
    #[inline]
    fn find_local_entry(&self, id: PropertyId) -> Result<usize, usize> {
        self.local_entries
            .binary_search_by_key(&id, |(pid, _)| *pid)
    }

    /// Binary search for an animation entry by property ID.
    #[inline]
    fn find_animation_entry(&self, id: PropertyId) -> Result<usize, usize> {
        self.animation_entries
            .binary_search_by_key(&id, |(pid, _)| *pid)
    }

    /// Gets an erased value for a given layer.
    #[inline]
    fn get_layer_value(&self, id: PropertyId, layer: Layer) -> Option<&ErasedValue> {
        match layer {
            Layer::Local => self
                .find_local_entry(id)
                .ok()
                .map(|idx| &self.local_entries[idx].1),
            Layer::Animation => self
                .find_animation_entry(id)
                .ok()
                .map(|idx| &self.animation_entries[idx].1),
        }
    }

    #[inline]
    fn set_layer_value(&mut self, id: PropertyId, layer: Layer, value: ErasedValue) {
        match layer {
            Layer::Local => match self.find_local_entry(id) {
                Ok(idx) => self.local_entries[idx].1 = value,
                Err(idx) => self.local_entries.insert(idx, (id, value)),
            },
            Layer::Animation => {
                match self.find_animation_entry(id) {
                    Ok(idx) => self.animation_entries[idx].1 = value,
                    Err(idx) => self.animation_entries.insert(idx, (id, value)),
                };
            }
        }
    }

    #[inline]
    fn clear_layer_value(&mut self, id: PropertyId, layer: Layer) -> bool {
        match layer {
            Layer::Local => {
                if let Ok(idx) = self.find_local_entry(id) {
                    self.local_entries.remove(idx);
                    true
                } else {
                    false
                }
            }
            Layer::Animation => {
                if let Ok(idx) = self.find_animation_entry(id) {
                    self.animation_entries.remove(idx);
                    true
                } else {
                    false
                }
            }
        }
    }

    // =========================================================================
    // Local value methods
    // =========================================================================

    /// Gets the local value, if set.
    #[must_use]
    #[inline]
    pub fn get_local<T: Clone + 'static>(&self, property: Property<T>) -> Option<&T> {
        self.get_layer_value(property.id(), Layer::Local)
            .and_then(ErasedValue::downcast_ref)
    }

    /// Sets the local value.
    ///
    /// Returns a reference to the stored value.
    pub fn set_local<T: Clone + 'static>(&mut self, property: Property<T>, value: T) -> &T {
        let id = property.id();
        self.set_layer_value(id, Layer::Local, ErasedValue::new(value));
        self.get_local(property).unwrap()
    }

    /// Clears the local value.
    ///
    /// Returns `true` if a value was removed.
    pub fn clear_local<T: Clone + 'static>(&mut self, property: Property<T>) -> bool {
        self.clear_layer_value(property.id(), Layer::Local)
    }

    /// Returns `true` if the property has a local value.
    #[must_use]
    #[inline]
    pub fn has_local<T: Clone + 'static>(&self, property: Property<T>) -> bool {
        self.find_local_entry(property.id()).is_ok()
    }

    // =========================================================================
    // Animation value methods
    // =========================================================================

    /// Gets the animation value, if set.
    #[must_use]
    #[inline]
    pub fn get_animation<T: Clone + 'static>(&self, property: Property<T>) -> Option<&T> {
        self.get_layer_value(property.id(), Layer::Animation)
            .and_then(ErasedValue::downcast_ref)
    }

    /// Sets the animation value.
    ///
    /// Returns a reference to the stored value.
    pub fn set_animation<T: Clone + 'static>(&mut self, property: Property<T>, value: T) -> &T {
        let id = property.id();
        self.set_layer_value(id, Layer::Animation, ErasedValue::new(value));
        self.get_animation(property).unwrap()
    }

    /// Clears the animation value.
    ///
    /// Returns `true` if a value was removed.
    pub fn clear_animation<T: Clone + 'static>(&mut self, property: Property<T>) -> bool {
        self.clear_layer_value(property.id(), Layer::Animation)
    }

    /// Returns `true` if the property has an animation value.
    #[must_use]
    #[inline]
    pub fn has_animation<T: Clone + 'static>(&self, property: Property<T>) -> bool {
        self.find_animation_entry(property.id()).is_ok()
    }

    // =========================================================================
    // Effective value resolution
    // =========================================================================

    /// Gets the effective local value (Animation → Local → registry default).
    ///
    /// This resolves from this store's values and falls back to the registry
    /// default. It does **not** handle style or inheritance—those belong to
    /// higher-level APIs.
    ///
    /// # Panics
    ///
    /// Panics if the property is not registered in the registry.
    #[must_use]
    pub fn get_effective_local<T: Clone + 'static>(
        &self,
        property: Property<T>,
        registry: &PropertyRegistry,
    ) -> T {
        let id = property.id();
        // Check our values (Animation > Local)
        if let Some(v) = self.get_layer_value(id, Layer::Animation)
            && let Some(v) = v.downcast_ref::<T>()
        {
            return v.clone();
        }
        if let Some(v) = self.get_layer_value(id, Layer::Local)
            && let Some(v) = v.downcast_ref::<T>()
        {
            return v.clone();
        }

        // Fall back to registry default
        if let Some(metadata) = registry.get_metadata::<T>(property) {
            return metadata.default_value().clone();
        }

        panic!("Property {:?} not found in registry", property.id());
    }

    /// Gets the effective local value (Animation → Local → registry default), borrowed.
    ///
    /// This avoids cloning and returns a reference into either this store (Animation/Local)
    /// or the registry's default value.
    ///
    /// # Panics
    ///
    /// Panics if the property is not registered in the registry.
    #[must_use]
    #[inline]
    pub fn get_effective_local_ref<'a, T: Clone + 'static>(
        &'a self,
        property: Property<T>,
        registry: &'a PropertyRegistry,
    ) -> &'a T {
        let id = property.id();
        // Check our values (Animation > Local)
        if let Some(v) = self.get_layer_value(id, Layer::Animation)
            && let Some(v) = v.downcast_ref::<T>()
        {
            return v;
        }
        if let Some(v) = self.get_layer_value(id, Layer::Local)
            && let Some(v) = v.downcast_ref::<T>()
        {
            return v;
        }

        // Fall back to registry default
        if let Some(metadata) = registry.get_metadata::<T>(property) {
            return metadata.default_value();
        }

        panic!("Property {:?} not found in registry", property.id());
    }

    /// Returns `true` if the property has any value (local or animation).
    #[must_use]
    #[inline]
    pub fn has_value<T: Clone + 'static>(&self, property: Property<T>) -> bool {
        self.find_local_entry(property.id()).is_ok()
            || self.find_animation_entry(property.id()).is_ok()
    }

    /// Clears all values for a property.
    ///
    /// Returns `true` if any values were removed.
    pub fn clear_all<T: Clone + 'static>(&mut self, property: Property<T>) -> bool {
        let id = property.id();
        let mut removed = self.clear_layer_value(id, Layer::Local);
        removed |= self.clear_layer_value(id, Layer::Animation);
        removed
    }

    /// Clears all animation values across all properties.
    ///
    /// Returns the number of animation values removed.
    pub fn clear_all_animations(&mut self) -> usize {
        let len = self.animation_entries.len();
        self.animation_entries.clear();
        len
    }
}

impl<K: Copy + Eq> Clone for PropertyStore<K> {
    fn clone(&self) -> Self {
        Self {
            local_entries: self.local_entries.clone(),
            animation_entries: self.animation_entries.clone(),
            owner: self.owner,
        }
    }
}

struct PropertyIds<'a> {
    local: &'a [(PropertyId, ErasedValue)],
    animation: &'a [(PropertyId, ErasedValue)],
    local_i: usize,
    animation_i: usize,
}

impl Iterator for PropertyIds<'_> {
    type Item = PropertyId;

    fn next(&mut self) -> Option<Self::Item> {
        let local = self.local.get(self.local_i).map(|(id, _)| *id);
        let animation = self.animation.get(self.animation_i).map(|(id, _)| *id);

        match (local, animation) {
            (None, None) => None,
            (Some(id), None) => {
                self.local_i += 1;
                Some(id)
            }
            (None, Some(id)) => {
                self.animation_i += 1;
                Some(id)
            }
            (Some(local_id), Some(animation_id)) => {
                if local_id < animation_id {
                    self.local_i += 1;
                    Some(local_id)
                } else if animation_id < local_id {
                    self.animation_i += 1;
                    Some(animation_id)
                } else {
                    self.local_i += 1;
                    self.animation_i += 1;
                    Some(local_id)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::PropertyMetadataBuilder;
    use alloc::boxed::Box;
    use alloc::vec::Vec;

    fn setup_registry() -> (PropertyRegistry, Property<f64>, Property<i32>) {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
        let count = registry.register("Count", PropertyMetadataBuilder::new(0_i32).build());
        (registry, width, count)
    }

    #[test]
    fn store_new() {
        let store = PropertyStore::<u32>::new(1);
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert_eq!(store.owner(), 1);
    }

    #[test]
    fn store_set_get_local() {
        let (_, width, _) = setup_registry();
        let mut store = PropertyStore::<u32>::new(1);

        assert!(store.get_local(width).is_none());

        store.set_local(width, 100.0);
        assert_eq!(store.get_local(width), Some(&100.0));
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn store_set_get_animation() {
        let (_, width, _) = setup_registry();
        let mut store = PropertyStore::<u32>::new(1);

        assert!(store.get_animation(width).is_none());

        store.set_animation(width, 200.0);
        assert_eq!(store.get_animation(width), Some(&200.0));
    }

    #[test]
    fn store_animation_precedence() {
        let (registry, width, _) = setup_registry();
        let mut store = PropertyStore::<u32>::new(1);

        // Set local value
        store.set_local(width, 100.0);
        assert_eq!(store.get_effective_local(width, &registry), 100.0);

        // Animation overrides local
        store.set_animation(width, 200.0);
        assert_eq!(store.get_effective_local(width, &registry), 200.0);

        // Clear animation, local becomes effective
        store.clear_animation(width);
        assert_eq!(store.get_effective_local(width, &registry), 100.0);
    }

    #[test]
    fn store_effective_local_ref_precedence_and_sources() {
        let (registry, width, _) = setup_registry();
        let mut store = PropertyStore::<u32>::new(1);

        // Default comes from registry.
        let default_ref = store.get_effective_local_ref(width, &registry);
        let metadata_default = registry.get_metadata(width).unwrap().default_value();
        assert!(core::ptr::eq(default_ref, metadata_default));

        // Local comes from store.
        store.set_local(width, 100.0);
        let local_ref = store.get_effective_local_ref(width, &registry);
        assert!(core::ptr::eq(local_ref, store.get_local(width).unwrap()));

        // Animation overrides local.
        store.set_animation(width, 200.0);
        let anim_ref = store.get_effective_local_ref(width, &registry);
        assert!(core::ptr::eq(anim_ref, store.get_animation(width).unwrap()));
    }

    #[test]
    fn store_clear_local() {
        let (_, width, _) = setup_registry();
        let mut store = PropertyStore::<u32>::new(1);

        store.set_local(width, 100.0);
        assert!(store.has_local(width));

        assert!(store.clear_local(width));
        assert!(!store.has_local(width));
        assert!(store.is_empty());

        // Clearing non-existent returns false
        assert!(!store.clear_local(width));
    }

    #[test]
    fn store_clear_animation() {
        let (_, width, _) = setup_registry();
        let mut store = PropertyStore::<u32>::new(1);

        store.set_animation(width, 200.0);
        assert!(store.has_animation(width));

        assert!(store.clear_animation(width));
        assert!(!store.has_animation(width));
        assert!(store.is_empty());
    }

    #[test]
    fn store_clear_all_animations() {
        let (_, width, count) = setup_registry();
        let mut store = PropertyStore::<u32>::new(1);

        store.set_local(width, 100.0);
        store.set_animation(width, 200.0);
        store.set_animation(count, 5);

        let removed = store.clear_all_animations();
        assert_eq!(removed, 2);

        assert!(!store.has_animation(width));
        assert!(!store.has_animation(count));
        assert!(store.has_local(width)); // Local preserved
        assert!(!store.has_value(count)); // Entry removed (was animation only)
    }

    #[test]
    fn store_default_value() {
        let (registry, width, _) = setup_registry();
        let store = PropertyStore::<u32>::new(1);

        // No value set, should return default from registry
        assert_eq!(store.get_effective_local(width, &registry), 0.0);
    }

    #[test]
    fn store_clone() {
        let (_, width, _) = setup_registry();
        let mut store = PropertyStore::<u32>::new(1);
        store.set_local(width, 100.0);

        let cloned = store.clone();
        assert_eq!(cloned.get_local(width), Some(&100.0));
        assert_eq!(cloned.owner(), 1);
    }

    #[test]
    fn store_property_ids() {
        let (_, width, count) = setup_registry();
        let mut store = PropertyStore::<u32>::new(1);

        store.set_local(width, 100.0);
        store.set_local(count, 5);

        let ids: Vec<_> = store.property_ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&width.id()));
        assert!(ids.contains(&count.id()));
    }

    #[test]
    fn store_sorted_order() {
        let mut registry = PropertyRegistry::new();
        // Register in reverse order to test sorting
        let c: Property<i32> = registry.register("C", PropertyMetadataBuilder::new(0).build());
        let a: Property<i32> = registry.register("A", PropertyMetadataBuilder::new(0).build());
        let b: Property<i32> = registry.register("B", PropertyMetadataBuilder::new(0).build());

        let mut store = PropertyStore::<u32>::new(1);

        // Set in arbitrary order
        store.set_local(b, 2);
        store.set_local(c, 3);
        store.set_local(a, 1);

        // Should maintain sorted order by PropertyId
        let ids: Vec<_> = store.property_ids().collect();
        assert_eq!(ids.len(), 3);

        // IDs should be in ascending order
        for i in 1..ids.len() {
            assert!(ids[i - 1].index() < ids[i].index());
        }
    }

    #[test]
    fn store_binary_search_correctness() {
        let mut registry = PropertyRegistry::new();
        let props: Vec<Property<i32>> = (0..20)
            .map(|i| {
                registry.register(
                    Box::leak(alloc::format!("Prop{i}").into_boxed_str()),
                    PropertyMetadataBuilder::new(0).build(),
                )
            })
            .collect();

        let mut store = PropertyStore::<u32>::new(1);

        // Set every other property
        for (i, prop) in props.iter().enumerate() {
            if i % 2 == 0 {
                let value = i32::try_from(i).unwrap();
                store.set_local(*prop, value);
            }
        }

        // Verify lookups work correctly
        for (i, prop) in props.iter().enumerate() {
            if i % 2 == 0 {
                let value = i32::try_from(i).unwrap();
                assert_eq!(store.get_local(*prop), Some(&value));
            } else {
                assert!(store.get_local(*prop).is_none());
            }
        }
    }

    #[test]
    fn store_local_and_animation_together() {
        let (_, width, _) = setup_registry();
        let mut store = PropertyStore::<u32>::new(1);

        // Set both
        store.set_local(width, 100.0);
        store.set_animation(width, 200.0);

        // Both accessible individually
        assert_eq!(store.get_local(width), Some(&100.0));
        assert_eq!(store.get_animation(width), Some(&200.0));

        // Clear local, animation still there
        store.clear_local(width);
        assert!(store.get_local(width).is_none());
        assert_eq!(store.get_animation(width), Some(&200.0));
        assert!(store.has_value(width)); // Entry still exists

        // Clear animation, entry removed
        store.clear_animation(width);
        assert!(!store.has_value(width));
        assert!(store.is_empty());
    }
}
