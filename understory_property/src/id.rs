// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property identification types.
//!
//! This module provides [`PropertyId`] for runtime property identification and
//! [`Property<T>`] for type-safe compile-time property keys.

use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;

/// A runtime property identifier.
///
/// This is a lightweight handle (u16) that uniquely identifies a property
/// within a [`PropertyRegistry`](crate::PropertyRegistry). The u16 size allows
/// up to 65,536 properties while keeping storage compact.
///
/// # Example
///
/// ```rust
/// use understory_property::PropertyId;
///
/// let id = PropertyId::new(42);
/// assert_eq!(id.index(), 42);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PropertyId(u16);

impl PropertyId {
    /// Creates a new property ID from the given index.
    ///
    /// This is typically called by [`PropertyRegistry::register`](crate::PropertyRegistry::register)
    /// rather than directly.
    #[must_use]
    #[inline]
    pub const fn new(index: u16) -> Self {
        Self(index)
    }

    /// Returns the underlying index of this property ID.
    #[must_use]
    #[inline]
    pub const fn index(self) -> u16 {
        self.0
    }
}

impl fmt::Debug for PropertyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PropertyId").field(&self.0).finish()
    }
}

impl fmt::Display for PropertyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PropertyId({})", self.0)
    }
}

/// A type-safe property key with phantom type for compile-time checking.
///
/// This wraps a [`PropertyId`] with a phantom type parameter `T` that represents
/// the property's value type. This enables compile-time type safety when getting
/// and setting property values.
///
/// # Type Safety
///
/// The phantom type ensures that you can only get/set values of the correct type:
///
/// ```rust
/// use understory_property::{Property, PropertyMetadataBuilder, PropertyRegistry};
///
/// let mut registry = PropertyRegistry::new();
///
/// // Register a f64 property
/// let width: Property<f64> = registry.register(
///     "Width",
///     PropertyMetadataBuilder::new(0.0_f64).build()
/// );
///
/// // The type is inferred/checked at compile time
/// // store.set_local(width, "not a number"); // Would not compile!
/// ```
///
/// # Memory Layout
///
/// `Property<T>` is the same size as `PropertyId` (2 bytes) since `PhantomData`
/// has zero size.
pub struct Property<T> {
    id: PropertyId,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Property<T> {
    /// Creates a new typed property from a property ID.
    ///
    /// This is typically called by [`PropertyRegistry::register`](crate::PropertyRegistry::register)
    /// rather than directly.
    ///
    /// # Safety Note
    ///
    /// The caller must ensure that the `PropertyId` was registered with the same
    /// type `T`. Using mismatched types will cause panics at runtime.
    #[must_use]
    #[inline]
    pub const fn from_id(id: PropertyId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// Returns the underlying property ID.
    #[must_use]
    #[inline]
    pub const fn id(self) -> PropertyId {
        self.id
    }
}

// Manual trait implementations to avoid requiring T: Clone, etc.

impl<T> Copy for Property<T> {}

impl<T> Clone for Property<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> PartialEq for Property<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Property<T> {}

impl<T> Hash for Property<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> fmt::Debug for Property<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Property")
            .field("id", &self.id)
            .field("type", &core::any::type_name::<T>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::String;

    #[test]
    fn property_id_basics() {
        let id = PropertyId::new(42);
        assert_eq!(id.index(), 42);

        let id2 = PropertyId::new(42);
        assert_eq!(id, id2);

        let id3 = PropertyId::new(43);
        assert_ne!(id, id3);
    }

    #[test]
    fn property_id_debug() {
        let id = PropertyId::new(42);
        assert_eq!(format!("{:?}", id), "PropertyId(42)");
    }

    #[test]
    fn property_id_display() {
        let id = PropertyId::new(42);
        assert_eq!(format!("{}", id), "PropertyId(42)");
    }

    #[test]
    fn property_type_safety() {
        let id = PropertyId::new(1);
        let prop_f64: Property<f64> = Property::from_id(id);
        let prop_i32: Property<i32> = Property::from_id(id);

        // Same ID, different phantom types
        assert_eq!(prop_f64.id(), prop_i32.id());
    }

    #[test]
    fn property_copy_clone() {
        let prop: Property<f64> = Property::from_id(PropertyId::new(1));
        let prop2 = prop;
        let prop3 = prop;

        assert_eq!(prop, prop2);
        assert_eq!(prop, prop3);
    }

    #[test]
    fn property_size() {
        use core::mem::size_of;
        assert_eq!(size_of::<PropertyId>(), 2);
        assert_eq!(size_of::<Property<f64>>(), 2);
        assert_eq!(size_of::<Property<String>>(), 2);
    }
}
