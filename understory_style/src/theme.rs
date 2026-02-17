// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Theme resource lookup.
//!
//! This module provides [`Theme`], a collection of themed resources that can
//! be looked up by [`ResourceKey`].

use alloc::rc::Rc;
use alloc::vec::Vec;
use core::fmt;

use understory_property::ErasedValue;

/// A key for looking up resources in a [`Theme`].
///
/// Resource keys are simple u16 identifiers, typically defined as constants
/// at the application level.
///
/// # Example
///
/// ```rust
/// use understory_style::ResourceKey;
///
/// // Define resource keys as constants
/// const ACCENT_COLOR: ResourceKey = ResourceKey::new(0);
/// const FONT_SIZE: ResourceKey = ResourceKey::new(1);
/// const CORNER_RADIUS: ResourceKey = ResourceKey::new(2);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceKey(u16);

impl ResourceKey {
    /// Creates a new resource key with the given index.
    #[must_use]
    #[inline]
    pub const fn new(index: u16) -> Self {
        Self(index)
    }

    /// Returns the underlying index of this resource key.
    #[must_use]
    #[inline]
    pub const fn index(self) -> u16 {
        self.0
    }
}

impl fmt::Debug for ResourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ResourceKey").field(&self.0).finish()
    }
}

impl fmt::Display for ResourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ResourceKey({})", self.0)
    }
}

/// A collection of themed resources.
///
/// Themes provide resource lookup by [`ResourceKey`], enabling theming
/// (light/dark modes, brand colors, etc.). Unlike styles which set property
/// values directly, themes provide values that properties can reference.
///
/// Themes are immutable after creation. Use [`ThemeBuilder`] to construct them.
///
/// # Memory Layout
///
/// Internally, `Theme` wraps an `Rc<ThemeData>`, making cloning cheap.
/// Resources are stored in a sorted vector for O(log n) lookup.
///
/// # Example
///
/// ```rust
/// use understory_style::{Theme, ThemeBuilder, ResourceKey};
///
/// const ACCENT_COLOR: ResourceKey = ResourceKey::new(0);
///
/// let light_theme = ThemeBuilder::new()
///     .set(ACCENT_COLOR, 0x0078D4_u32)
///     .build();
///
/// let dark_theme = ThemeBuilder::new()
///     .set(ACCENT_COLOR, 0x4CC2FF_u32)
///     .build();
///
/// assert_eq!(light_theme.get::<u32>(ACCENT_COLOR), Some(&0x0078D4));
/// assert_eq!(dark_theme.get::<u32>(ACCENT_COLOR), Some(&0x4CC2FF));
/// ```
#[derive(Clone, Debug)]
pub struct Theme {
    inner: Rc<ThemeData>,
}

/// Internal storage for theme resources.
#[derive(Debug, Default)]
struct ThemeData {
    /// Sorted by `ResourceKey` for binary search lookup.
    resources: Vec<(ResourceKey, ErasedValue)>,
}

impl Theme {
    /// Returns `true` if this theme has no resources.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.resources.is_empty()
    }

    /// Returns the number of resources in this theme.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.resources.len()
    }

    /// Gets the value for a resource key, if present.
    #[must_use]
    #[inline]
    pub fn get<T: Clone + 'static>(&self, key: ResourceKey) -> Option<&T> {
        self.inner
            .resources
            .binary_search_by_key(&key, |(k, _)| *k)
            .ok()
            .and_then(|idx| self.inner.resources[idx].1.downcast_ref())
    }

    /// Returns `true` if this theme has a value for the resource key.
    #[must_use]
    #[inline]
    pub fn contains(&self, key: ResourceKey) -> bool {
        self.inner
            .resources
            .binary_search_by_key(&key, |(k, _)| *k)
            .is_ok()
    }

    /// Returns an iterator over the resource keys in this theme.
    pub fn keys(&self) -> impl Iterator<Item = ResourceKey> + '_ {
        self.inner.resources.iter().map(|(k, _)| *k)
    }
}

impl Default for Theme {
    fn default() -> Self {
        ThemeBuilder::new().build()
    }
}

/// Builder for constructing [`Theme`] instances.
///
/// # Example
///
/// ```rust
/// use understory_style::{ThemeBuilder, ResourceKey};
///
/// const PRIMARY: ResourceKey = ResourceKey::new(0);
/// const SECONDARY: ResourceKey = ResourceKey::new(1);
///
/// let theme = ThemeBuilder::new()
///     .set(PRIMARY, "#0078D4".to_string())
///     .set(SECONDARY, "#106EBE".to_string())
///     .build();
///
/// assert_eq!(theme.get::<String>(PRIMARY).map(|s| s.as_str()), Some("#0078D4"));
/// ```
#[derive(Debug, Default)]
pub struct ThemeBuilder {
    resources: Vec<(ResourceKey, ErasedValue)>,
}

impl ThemeBuilder {
    /// Creates a new empty theme builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a resource value in the theme.
    ///
    /// If the resource was already set, the value is replaced.
    #[must_use]
    pub fn set<T: Clone + 'static>(mut self, key: ResourceKey, value: T) -> Self {
        let erased = ErasedValue::new(value);

        match self.resources.binary_search_by_key(&key, |(k, _)| *k) {
            Ok(idx) => {
                self.resources[idx].1 = erased;
            }
            Err(idx) => {
                self.resources.insert(idx, (key, erased));
            }
        }
        self
    }

    /// Builds the theme.
    #[must_use]
    pub fn build(self) -> Theme {
        Theme {
            inner: Rc::new(ThemeData {
                resources: self.resources,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    const PRIMARY: ResourceKey = ResourceKey::new(0);
    const SECONDARY: ResourceKey = ResourceKey::new(1);
    const FONT_SIZE: ResourceKey = ResourceKey::new(2);

    #[test]
    fn resource_key_basics() {
        let key = ResourceKey::new(42);
        assert_eq!(key.index(), 42);

        let key2 = ResourceKey::new(42);
        assert_eq!(key, key2);

        let key3 = ResourceKey::new(43);
        assert_ne!(key, key3);
    }

    #[test]
    fn resource_key_debug() {
        use alloc::format;
        let key = ResourceKey::new(42);
        assert_eq!(format!("{:?}", key), "ResourceKey(42)");
    }

    #[test]
    fn resource_key_display() {
        use alloc::format;
        let key = ResourceKey::new(42);
        assert_eq!(format!("{}", key), "ResourceKey(42)");
    }

    #[test]
    fn theme_empty() {
        let theme = ThemeBuilder::new().build();
        assert!(theme.is_empty());
        assert_eq!(theme.len(), 0);
    }

    #[test]
    fn theme_single_resource() {
        let theme = ThemeBuilder::new().set(PRIMARY, 0x0078D4_u32).build();

        assert!(!theme.is_empty());
        assert_eq!(theme.len(), 1);
        assert_eq!(theme.get::<u32>(PRIMARY), Some(&0x0078D4));
    }

    #[test]
    fn theme_multiple_resources() {
        let theme = ThemeBuilder::new()
            .set(PRIMARY, 0x0078D4_u32)
            .set(FONT_SIZE, 14.0_f64)
            .build();

        assert_eq!(theme.len(), 2);
        assert_eq!(theme.get::<u32>(PRIMARY), Some(&0x0078D4));
        assert_eq!(theme.get::<f64>(FONT_SIZE), Some(&14.0));
    }

    #[test]
    fn theme_string_resources() {
        let theme = ThemeBuilder::new()
            .set(PRIMARY, "#0078D4".to_string())
            .set(SECONDARY, "#106EBE".to_string())
            .build();

        assert_eq!(
            theme.get::<String>(PRIMARY).map(|s| s.as_str()),
            Some("#0078D4")
        );
        assert_eq!(
            theme.get::<String>(SECONDARY).map(|s| s.as_str()),
            Some("#106EBE")
        );
    }

    #[test]
    fn theme_replace_value() {
        let theme = ThemeBuilder::new()
            .set(PRIMARY, 0x0078D4_u32)
            .set(PRIMARY, 0x4CC2FF_u32)
            .build();

        assert_eq!(theme.len(), 1);
        assert_eq!(theme.get::<u32>(PRIMARY), Some(&0x4CC2FF));
    }

    #[test]
    fn theme_contains() {
        let theme = ThemeBuilder::new().set(PRIMARY, 0x0078D4_u32).build();

        assert!(theme.contains(PRIMARY));
        assert!(!theme.contains(SECONDARY));
    }

    #[test]
    fn theme_clone_is_cheap() {
        let theme = ThemeBuilder::new().set(PRIMARY, 0x0078D4_u32).build();
        let theme2 = theme.clone();

        // Both reference the same data
        assert_eq!(theme.get::<u32>(PRIMARY), Some(&0x0078D4));
        assert_eq!(theme2.get::<u32>(PRIMARY), Some(&0x0078D4));

        // Rc makes this cheap
        assert!(Rc::ptr_eq(&theme.inner, &theme2.inner));
    }

    #[test]
    fn theme_keys() {
        let theme = ThemeBuilder::new()
            .set(SECONDARY, 0_u32)
            .set(PRIMARY, 0_u32)
            .set(FONT_SIZE, 0.0_f64)
            .build();

        let keys: Vec<_> = theme.keys().collect();
        assert_eq!(keys.len(), 3);
        // Should be sorted by ResourceKey
        assert!(keys[0].index() < keys[1].index());
        assert!(keys[1].index() < keys[2].index());
    }

    #[test]
    fn theme_default() {
        let theme = Theme::default();
        assert!(theme.is_empty());
    }

    #[test]
    fn theme_get_wrong_type_returns_none() {
        let theme = ThemeBuilder::new().set(PRIMARY, 0x0078D4_u32).build();

        // PRIMARY is u32, trying to get as f64 fails
        assert!(theme.get::<f64>(PRIMARY).is_none());
    }
}
