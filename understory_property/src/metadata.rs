// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property metadata definitions.
//!
//! This module provides [`PropertyMetadata`] for storing property configuration
//! and [`PropertyMetadataBuilder`] for ergonomic construction.

use alloc::boxed::Box;
use understory_dirty::ChannelSet;

/// Callback invoked when a property value changes.
///
/// The callback receives the old value (if any) and the new value.
pub type PropertyChangedCallback<T> = Box<dyn Fn(Option<&T>, &T) + Send + Sync>;

/// Callback for coercing a property value before it's stored.
///
/// This can be used to clamp values, validate ranges, etc.
/// The callback receives the proposed value and returns the coerced value.
pub type CoerceValueCallback<T> = Box<dyn Fn(T) -> T + Send + Sync>;

/// Metadata for a dependency property.
///
/// This contains the configuration for a property including its default value,
/// whether it inherits, which dirty channels it affects, and optional callbacks.
///
/// # Example
///
/// ```rust
/// use understory_property::PropertyMetadataBuilder;
/// use understory_dirty::Channel;
///
/// const LAYOUT: Channel = Channel::new(0);
///
/// let metadata = PropertyMetadataBuilder::new(100.0_f64)
///     .inherits(true)
///     .affects_channels(LAYOUT.into_set())
///     .build();
///
/// assert_eq!(metadata.default_value(), &100.0);
/// assert!(metadata.inherits());
/// ```
pub struct PropertyMetadata<T: Clone + 'static> {
    default_value: T,
    inherits: bool,
    affects_channels: ChannelSet,
    changed_callback: Option<PropertyChangedCallback<T>>,
    coerce_callback: Option<CoerceValueCallback<T>>,
}

impl<T: Clone + 'static> PropertyMetadata<T> {
    /// Creates new property metadata with the given default value.
    ///
    /// All other fields use their defaults:
    /// - `inherits`: `false`
    /// - `affects_channels`: empty
    /// - `changed_callback`: `None`
    /// - `coerce_callback`: `None`
    #[must_use]
    pub fn new(default_value: T) -> Self {
        Self {
            default_value,
            inherits: false,
            affects_channels: ChannelSet::empty(),
            changed_callback: None,
            coerce_callback: None,
        }
    }

    /// Returns a reference to the default value.
    #[must_use]
    #[inline]
    pub fn default_value(&self) -> &T {
        &self.default_value
    }

    /// Returns whether this property inherits from parent objects.
    #[must_use]
    #[inline]
    pub fn inherits(&self) -> bool {
        self.inherits
    }

    /// Returns the dirty channels affected by changes to this property.
    #[must_use]
    #[inline]
    pub fn affects_channels(&self) -> ChannelSet {
        self.affects_channels
    }

    /// Invokes the changed callback if one is set.
    #[inline]
    pub fn on_changed(&self, old_value: Option<&T>, new_value: &T) {
        if let Some(callback) = &self.changed_callback {
            callback(old_value, new_value);
        }
    }

    /// Coerces a value using the coerce callback if one is set.
    #[inline]
    pub fn coerce(&self, value: T) -> T {
        if let Some(callback) = &self.coerce_callback {
            callback(value)
        } else {
            value
        }
    }

    /// Returns whether a changed callback is set.
    #[must_use]
    #[inline]
    pub fn has_changed_callback(&self) -> bool {
        self.changed_callback.is_some()
    }

    /// Returns whether a coerce callback is set.
    #[must_use]
    #[inline]
    pub fn has_coerce_callback(&self) -> bool {
        self.coerce_callback.is_some()
    }
}

// Manual Debug impl since callbacks aren't Debug
impl<T: Clone + core::fmt::Debug + 'static> core::fmt::Debug for PropertyMetadata<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PropertyMetadata")
            .field("default_value", &self.default_value)
            .field("inherits", &self.inherits)
            .field("affects_channels", &self.affects_channels)
            .field("has_changed_callback", &self.changed_callback.is_some())
            .field("has_coerce_callback", &self.coerce_callback.is_some())
            .finish()
    }
}

/// Builder for [`PropertyMetadata`].
///
/// # Example
///
/// ```rust
/// use understory_property::PropertyMetadataBuilder;
/// use understory_dirty::Channel;
///
/// const LAYOUT: Channel = Channel::new(0);
/// const PAINT: Channel = Channel::new(1);
///
/// let metadata = PropertyMetadataBuilder::new(0.0_f64)
///     .inherits(true)
///     .affects_channels(LAYOUT.into_set() | PAINT.into_set())
///     .coerce(|v| v.max(0.0).min(100.0))
///     .build();
/// ```
pub struct PropertyMetadataBuilder<T: Clone + 'static> {
    default_value: T,
    inherits: bool,
    affects_channels: ChannelSet,
    changed_callback: Option<PropertyChangedCallback<T>>,
    coerce_callback: Option<CoerceValueCallback<T>>,
}

// Manual Debug impl since callbacks aren't Debug
impl<T: Clone + core::fmt::Debug + 'static> core::fmt::Debug for PropertyMetadataBuilder<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PropertyMetadataBuilder")
            .field("default_value", &self.default_value)
            .field("inherits", &self.inherits)
            .field("affects_channels", &self.affects_channels)
            .field("has_changed_callback", &self.changed_callback.is_some())
            .field("has_coerce_callback", &self.coerce_callback.is_some())
            .finish()
    }
}

impl<T: Clone + 'static> PropertyMetadataBuilder<T> {
    /// Creates a new builder with the given default value.
    #[must_use]
    pub fn new(default_value: T) -> Self {
        Self {
            default_value,
            inherits: false,
            affects_channels: ChannelSet::empty(),
            changed_callback: None,
            coerce_callback: None,
        }
    }

    /// Sets whether this property inherits from parent objects.
    ///
    /// When `true`, `get_effective` will walk up the parent chain to find
    /// a value if none is set locally.
    #[must_use]
    pub fn inherits(mut self, inherits: bool) -> Self {
        self.inherits = inherits;
        self
    }

    /// Sets the dirty channels affected by changes to this property.
    ///
    /// When the property changes, these channels will be marked dirty.
    #[must_use]
    pub fn affects_channels(mut self, channels: ChannelSet) -> Self {
        self.affects_channels = channels;
        self
    }

    /// Sets a callback to be invoked when the property value changes.
    #[must_use]
    pub fn on_changed<F>(mut self, callback: F) -> Self
    where
        F: Fn(Option<&T>, &T) + Send + Sync + 'static,
    {
        self.changed_callback = Some(Box::new(callback));
        self
    }

    /// Sets a callback to coerce values before they are stored.
    ///
    /// This is useful for clamping values, validation, etc.
    #[must_use]
    pub fn coerce<F>(mut self, callback: F) -> Self
    where
        F: Fn(T) -> T + Send + Sync + 'static,
    {
        self.coerce_callback = Some(Box::new(callback));
        self
    }

    /// Builds the [`PropertyMetadata`].
    #[must_use]
    pub fn build(self) -> PropertyMetadata<T> {
        PropertyMetadata {
            default_value: self.default_value,
            inherits: self.inherits,
            affects_channels: self.affects_channels,
            changed_callback: self.changed_callback,
            coerce_callback: self.coerce_callback,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use understory_dirty::Channel;

    const LAYOUT: Channel = Channel::new(0);
    const PAINT: Channel = Channel::new(1);

    #[test]
    fn metadata_defaults() {
        let metadata = PropertyMetadata::new(42_i32);
        assert_eq!(metadata.default_value(), &42);
        assert!(!metadata.inherits());
        assert!(metadata.affects_channels().is_empty());
        assert!(!metadata.has_changed_callback());
        assert!(!metadata.has_coerce_callback());
    }

    #[test]
    fn metadata_builder() {
        let metadata = PropertyMetadataBuilder::new(100.0_f64)
            .inherits(true)
            .affects_channels(LAYOUT.into_set() | PAINT.into_set())
            .build();

        assert_eq!(metadata.default_value(), &100.0);
        assert!(metadata.inherits());
        assert!(metadata.affects_channels().contains(LAYOUT));
        assert!(metadata.affects_channels().contains(PAINT));
    }

    #[test]
    fn metadata_coerce() {
        let metadata = PropertyMetadataBuilder::new(0.0_f64)
            .coerce(|v| v.clamp(0.0, 100.0))
            .build();

        assert_eq!(metadata.coerce(-10.0), 0.0);
        assert_eq!(metadata.coerce(50.0), 50.0);
        assert_eq!(metadata.coerce(150.0), 100.0);
    }

    #[test]
    fn metadata_changed_callback() {
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let metadata = PropertyMetadataBuilder::new(0_i32)
            .on_changed(move |_, _| {
                called_clone.store(true, Ordering::SeqCst);
            })
            .build();

        assert!(metadata.has_changed_callback());
        assert!(!called.load(Ordering::SeqCst));

        metadata.on_changed(None, &42);
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn metadata_debug() {
        let metadata = PropertyMetadataBuilder::new(42_i32).inherits(true).build();

        let debug = format!("{:?}", metadata);
        assert!(debug.contains("PropertyMetadata"));
        assert!(debug.contains("42"));
        assert!(debug.contains("true"));
    }
}
