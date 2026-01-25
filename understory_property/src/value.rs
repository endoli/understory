// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Type-erased property value storage.
//!
//! This module provides [`ErasedValue`] for storing property values of any type
//! in a heterogeneous collection.

use alloc::boxed::Box;
use core::any::{Any, TypeId};
use core::fmt;

/// A type-erased property value.
///
/// This wraps a value of any `'static + Clone` type, storing it on the heap
/// with its type information for later downcasting.
///
/// # Example
///
/// ```rust
/// use understory_property::ErasedValue;
///
/// let value = ErasedValue::new(42_i32);
/// assert!(value.is::<i32>());
/// assert_eq!(value.downcast_ref::<i32>(), Some(&42));
///
/// let cloned = value.clone_value();
/// assert_eq!(cloned.downcast_ref::<i32>(), Some(&42));
/// ```
pub struct ErasedValue {
    inner: Box<dyn ErasedValueTrait>,
    type_id: TypeId,
}

impl ErasedValue {
    /// Creates a new erased value from a concrete value.
    #[must_use]
    pub fn new<T: Clone + 'static>(value: T) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            inner: Box::new(value),
        }
    }

    /// Returns the [`TypeId`] of the contained value.
    #[must_use]
    #[inline]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Returns `true` if the contained value is of type `T`.
    #[must_use]
    #[inline]
    pub fn is<T: 'static>(&self) -> bool {
        self.type_id == TypeId::of::<T>()
    }

    /// Attempts to downcast to a reference of type `T`.
    ///
    /// Returns `None` if the contained value is not of type `T`.
    #[must_use]
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        if self.is::<T>() {
            // SAFETY: We verified the type matches
            self.inner.as_any().downcast_ref()
        } else {
            None
        }
    }

    /// Clones the contained value into a new [`ErasedValue`].
    #[must_use]
    pub fn clone_value(&self) -> Self {
        Self {
            inner: self.inner.clone_boxed(),
            type_id: self.type_id,
        }
    }
}

impl Clone for ErasedValue {
    fn clone(&self) -> Self {
        self.clone_value()
    }
}

impl fmt::Debug for ErasedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErasedValue")
            .field("type_id", &self.type_id)
            .finish_non_exhaustive()
    }
}

/// Trait object for type-erased values that can be cloned.
trait ErasedValueTrait: Any {
    fn as_any(&self) -> &dyn Any;
    fn clone_boxed(&self) -> Box<dyn ErasedValueTrait>;
}

impl<T: Clone + 'static> ErasedValueTrait for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_boxed(&self) -> Box<dyn ErasedValueTrait> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::String;

    #[test]
    fn erased_value_i32() {
        let value = ErasedValue::new(42_i32);
        assert!(value.is::<i32>());
        assert!(!value.is::<f64>());
        assert_eq!(value.downcast_ref::<i32>(), Some(&42));
        assert_eq!(value.downcast_ref::<f64>(), None);
    }

    #[test]
    fn erased_value_string() {
        let value = ErasedValue::new(String::from("hello"));
        assert!(value.is::<String>());
        assert_eq!(
            value.downcast_ref::<String>().map(|s| s.as_str()),
            Some("hello")
        );
    }

    #[test]
    fn erased_value_clone() {
        let value = ErasedValue::new(42_i32);
        let cloned = value.clone();
        assert_eq!(cloned.downcast_ref::<i32>(), Some(&42));

        // Original still works
        assert_eq!(value.downcast_ref::<i32>(), Some(&42));
    }

    #[test]
    fn erased_value_clone_string() {
        let value = ErasedValue::new(String::from("world"));
        let cloned = value.clone_value();
        assert_eq!(
            cloned.downcast_ref::<String>().map(|s| s.as_str()),
            Some("world")
        );
    }

    #[test]
    fn erased_value_type_id() {
        let value = ErasedValue::new(42_i32);
        assert_eq!(value.type_id(), TypeId::of::<i32>());
    }

    #[test]
    fn erased_value_debug() {
        let value = ErasedValue::new(42_i32);
        let debug = format!("{:?}", value);
        assert!(debug.contains("ErasedValue"));
        assert!(debug.contains("type_id"));
    }
}
