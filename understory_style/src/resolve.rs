// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Resolution context for full precedence chain.
//!
//! This module provides [`ResolveCx`], which bundles everything needed to
//! resolve property values through the full precedence chain.

use understory_property::{
    DependencyObject, ParentLookup, Property, PropertyRegistry, walk_inherited_ref,
};

use crate::selector::SelectorInputs;
use crate::stylesheet::StyleCascade;
use crate::theme::Theme;

/// Resolution context bundling registry, theme, and parent lookup.
///
/// This avoids passing many parameters to resolution functions and provides
/// the full WinUI-style precedence chain:
///
/// **Animation → Local → Style → Theme → Inherited → Default**
///
/// # Type Parameters
///
/// * `K` - The key type for objects (e.g., `u32`, `NodeId`)
/// * `F` - The parent lookup function type
///
/// # Example
///
/// ```rust
/// use understory_style::{
///     ResolveCx, SelectorInputs, StyleCascadeBuilder, StyleBuilder, StyleOrigin, ThemeBuilder,
/// };
/// use understory_property::{
///     DependencyObject, PropertyMetadataBuilder, PropertyRegistry, PropertyStore,
/// };
///
/// let mut registry = PropertyRegistry::new();
/// let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
///
/// let theme = ThemeBuilder::new().build();
/// let style = StyleBuilder::new().set(width, 80.0).build();
/// let cascade = StyleCascadeBuilder::new()
///     .push_style(StyleOrigin::Override, style)
///     .build();
///
/// // Create context with no parent lookup (flat tree)
/// let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
///
/// struct Element {
///     key: u32,
///     parent: Option<u32>,
///     store: PropertyStore<u32>,
/// }
///
/// impl DependencyObject<u32> for Element {
///     fn property_store(&self) -> &PropertyStore<u32> { &self.store }
///     fn property_store_mut(&mut self) -> &mut PropertyStore<u32> { &mut self.store }
///     fn key(&self) -> u32 { self.key }
///     fn parent_key(&self) -> Option<u32> { self.parent }
/// }
///
/// let mut element = Element {
///     key: 1,
///     parent: None,
///     store: PropertyStore::new(1),
/// };
///
/// element.store.set_local(width, 100.0);
///
/// let inputs = SelectorInputs::EMPTY;
///
/// // Local still wins over style
/// let value = cx.get_value(&element, &inputs, width, Some(&cascade));
/// assert_eq!(value, 100.0);
/// ```
pub struct ResolveCx<'a, K, F>
where
    K: Copy + Eq + 'a,
    F: ParentLookup<'a, K>,
{
    /// The property registry containing metadata and defaults.
    registry: &'a PropertyRegistry,
    /// The current theme for resource lookups.
    theme: &'a Theme,
    /// Function to look up parent stores for inheritance.
    store_lookup: F,
    /// Phantom to hold K in the type.
    _marker: core::marker::PhantomData<K>,
}

impl<'a, K, F> core::fmt::Debug for ResolveCx<'a, K, F>
where
    K: Copy + Eq + 'a,
    F: ParentLookup<'a, K>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ResolveCx")
            .field("registry", &self.registry)
            .field("theme", &self.theme)
            .field("store_lookup", &core::any::type_name::<F>())
            .finish()
    }
}

impl<'a, K, F> ResolveCx<'a, K, F>
where
    K: Copy + Eq + 'a,
    F: ParentLookup<'a, K>,
{
    /// Creates a new resolution context.
    ///
    /// # Arguments
    ///
    /// * `registry` - The property registry
    /// * `theme` - The current theme
    /// * `store_lookup` - Lookup returning (store, `parent_key`) for a given key
    pub fn new(registry: &'a PropertyRegistry, theme: &'a Theme, store_lookup: F) -> Self {
        Self {
            registry,
            theme,
            store_lookup,
            _marker: core::marker::PhantomData,
        }
    }

    /// Returns a reference to the property registry.
    #[must_use]
    #[inline]
    pub fn registry(&self) -> &PropertyRegistry {
        self.registry
    }

    /// Returns a reference to the current theme.
    #[must_use]
    #[inline]
    pub fn theme(&self) -> &Theme {
        self.theme
    }
}

impl<'a, K, F> ResolveCx<'a, K, F>
where
    K: Copy + Eq + 'a,
    F: ParentLookup<'a, K>,
{
    /// Resolves a property value through the full precedence chain, borrowed.
    ///
    /// This is the borrowed variant of [`ResolveCx::get_value`]. It returns a reference to the
    /// effective value, avoiding cloning for large types.
    ///
    /// # Panics
    ///
    /// Panics if the property is not registered in the registry.
    pub fn get_value_ref<'cx, T, O>(
        &'cx self,
        object: &'cx O,
        inputs: &SelectorInputs<'_>,
        property: Property<T>,
        style: Option<&'cx StyleCascade>,
    ) -> &'cx T
    where
        T: Clone + 'static,
        O: DependencyObject<K>,
    {
        // 1. Animation value
        if let Some(value) = object.property_store().get_animation(property) {
            return value;
        }

        // 2. Local value
        if let Some(value) = object.property_store().get_local(property) {
            return value;
        }

        // 3. Style-layer value
        if let Some(style) = style
            && let Some(value) = style.get_value_ref(inputs, property)
        {
            return value;
        }

        // 4. Inherited value (if property inherits)
        if let Some(metadata) = self.registry.get_metadata::<T>(property) {
            if metadata.inherits()
                && let Some(value) =
                    walk_inherited_ref(object.parent_key(), property, &self.store_lookup)
            {
                return value;
            }
            // 5. Default value
            return metadata.default_value();
        }

        panic!("Property {:?} not found in registry", property.id());
    }

    /// Resolves a property value through the full precedence chain.
    ///
    /// Precedence (highest to lowest):
    /// 1. Animation value on the object
    /// 2. Local value on the object
    /// 3. Style value (if style provided)
    /// 4. Inherited value (if property inherits, walks parent chain)
    /// 5. Default value from registry
    ///
    /// # Arguments
    ///
    /// * `object` - The object to get the value for
    /// * `property` - The property to resolve
    /// * `style` - Optional style to check for property values
    ///
    /// # Panics
    ///
    /// Panics if the property is not registered in the registry.
    pub fn get_value<T, O>(
        &self,
        object: &O,
        inputs: &SelectorInputs<'_>,
        property: Property<T>,
        style: Option<&StyleCascade>,
    ) -> T
    where
        T: Clone + 'static,
        O: DependencyObject<K>,
    {
        self.get_value_ref(object, inputs, property, style).clone()
    }

    /// Resolves a property value with a resource key fallback, borrowed.
    ///
    /// This is the borrowed variant of [`ResolveCx::get_value_with_theme`].
    ///
    /// # Panics
    ///
    /// Panics if the property is not registered in the registry.
    pub fn get_value_with_theme_ref<'cx, T, O>(
        &'cx self,
        object: &'cx O,
        inputs: &SelectorInputs<'_>,
        property: Property<T>,
        style: Option<&'cx StyleCascade>,
        resource_key: Option<crate::theme::ResourceKey>,
    ) -> &'cx T
    where
        T: Clone + 'static,
        O: DependencyObject<K>,
    {
        // 1. Animation value
        if let Some(value) = object.property_store().get_animation(property) {
            return value;
        }

        // 2. Local value
        if let Some(value) = object.property_store().get_local(property) {
            return value;
        }

        // 3. Style-layer value
        if let Some(style) = style
            && let Some(value) = style.get_value_ref(inputs, property)
        {
            return value;
        }

        // 4. Theme resource
        if let Some(key) = resource_key
            && let Some(value) = self.theme.get::<T>(key)
        {
            return value;
        }

        // 5. Inherited value (if property inherits)
        if let Some(metadata) = self.registry.get_metadata::<T>(property) {
            if metadata.inherits()
                && let Some(value) =
                    walk_inherited_ref(object.parent_key(), property, &self.store_lookup)
            {
                return value;
            }
            // 6. Default value
            return metadata.default_value();
        }

        panic!("Property {:?} not found in registry", property.id());
    }

    /// Resolves a property value with a resource key fallback.
    ///
    /// This is useful when a property can be set directly or can reference
    /// a theme resource. The precedence is:
    ///
    /// 1. Animation value on the object
    /// 2. Local value on the object
    /// 3. Style value (if style provided)
    /// 4. Theme resource (if `resource_key` provided and present in theme)
    /// 5. Inherited value (if property inherits)
    /// 6. Default value from registry
    ///
    /// # Arguments
    ///
    /// * `object` - The object to get the value for
    /// * `property` - The property to resolve
    /// * `style` - Optional style to check
    /// * `resource_key` - Optional theme resource key to check
    ///
    /// # Panics
    ///
    /// Panics if the property is not registered in the registry.
    pub fn get_value_with_theme<T, O>(
        &self,
        object: &O,
        inputs: &SelectorInputs<'_>,
        property: Property<T>,
        style: Option<&StyleCascade>,
        resource_key: Option<crate::theme::ResourceKey>,
    ) -> T
    where
        T: Clone + 'static,
        O: DependencyObject<K>,
    {
        self.get_value_with_theme_ref(object, inputs, property, style, resource_key)
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StyleBuilder, ThemeBuilder};
    use crate::{StyleCascadeBuilder, StyleOrigin};
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use understory_property::PropertyMetadataBuilder;
    use understory_property::PropertyStore;

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

    #[test]
    fn resolve_local_value() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let theme = ThemeBuilder::new().build();

        let mut element = TestElement::new(1, None);
        element.store.set_local(width, 100.0);

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value = cx.get_value(&element, &SelectorInputs::EMPTY, width, None);
        assert_eq!(value, 100.0);
    }

    #[test]
    fn resolve_local_value_ref_borrows_from_store() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let theme = ThemeBuilder::new().build();

        let mut element = TestElement::new(1, None);
        element.store.set_local(width, 100.0);

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value_ref = cx.get_value_ref(&element, &SelectorInputs::EMPTY, width, None);
        assert!(core::ptr::eq(
            value_ref,
            element.property_store().get_local(width).unwrap()
        ));
        assert_eq!(*value_ref, 100.0);
    }

    #[test]
    fn resolve_animation_over_local() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let theme = ThemeBuilder::new().build();

        let mut element = TestElement::new(1, None);
        element.store.set_local(width, 100.0);
        element.store.set_animation(width, 200.0);

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value = cx.get_value(&element, &SelectorInputs::EMPTY, width, None);
        assert_eq!(value, 200.0);
    }

    #[test]
    fn resolve_local_over_style() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let theme = ThemeBuilder::new().build();
        let style = StyleBuilder::new().set(width, 50.0).build();
        let style = StyleCascadeBuilder::new()
            .push_style(StyleOrigin::Override, style)
            .build();

        let mut element = TestElement::new(1, None);
        element.store.set_local(width, 100.0);

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value = cx.get_value(&element, &SelectorInputs::EMPTY, width, Some(&style));
        assert_eq!(value, 100.0);
    }

    #[test]
    fn resolve_style_value() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let theme = ThemeBuilder::new().build();
        let style = StyleBuilder::new().set(width, 50.0).build();
        let style = StyleCascadeBuilder::new()
            .push_style(StyleOrigin::Override, style)
            .build();

        let element = TestElement::new(1, None);

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value = cx.get_value(&element, &SelectorInputs::EMPTY, width, Some(&style));
        assert_eq!(value, 50.0);
    }

    #[test]
    fn resolve_default_value() {
        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(42.0_f64).build());

        let theme = ThemeBuilder::new().build();
        let element = TestElement::new(1, None);

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value = cx.get_value(&element, &SelectorInputs::EMPTY, width, None);
        assert_eq!(value, 42.0);
    }

    #[test]
    fn resolve_inherited_value() {
        let mut registry = PropertyRegistry::new();
        let font_size = registry.register(
            "FontSize",
            PropertyMetadataBuilder::new(12.0_f64)
                .inherits(true)
                .build(),
        );

        let theme = ThemeBuilder::new().build();

        let mut parent = TestElement::new(1, None);
        parent.store.set_local(font_size, 16.0);

        let child = TestElement::new(2, Some(1));

        let elements: BTreeMap<u32, &TestElement> =
            [(1, &parent), (2, &child)].into_iter().collect();

        let cx = ResolveCx::new(&registry, &theme, |key| {
            elements
                .get(&key)
                .map(|e| (e.property_store(), e.parent_key()))
        });

        let value = cx.get_value(&child, &SelectorInputs::EMPTY, font_size, None);
        assert_eq!(value, 16.0);
    }

    #[test]
    fn resolve_local_over_inherited() {
        let mut registry = PropertyRegistry::new();
        let font_size = registry.register(
            "FontSize",
            PropertyMetadataBuilder::new(12.0_f64)
                .inherits(true)
                .build(),
        );

        let theme = ThemeBuilder::new().build();

        let mut parent = TestElement::new(1, None);
        parent.store.set_local(font_size, 16.0);

        let mut child = TestElement::new(2, Some(1));
        child.store.set_local(font_size, 20.0);

        let elements: BTreeMap<u32, &TestElement> =
            [(1, &parent), (2, &child)].into_iter().collect();

        let cx = ResolveCx::new(&registry, &theme, |key| {
            elements
                .get(&key)
                .map(|e| (e.property_store(), e.parent_key()))
        });

        let value = cx.get_value(&child, &SelectorInputs::EMPTY, font_size, None);
        assert_eq!(value, 20.0);
    }

    #[test]
    fn resolve_style_over_inherited() {
        let mut registry = PropertyRegistry::new();
        let font_size = registry.register(
            "FontSize",
            PropertyMetadataBuilder::new(12.0_f64)
                .inherits(true)
                .build(),
        );

        let theme = ThemeBuilder::new().build();
        let style = StyleBuilder::new().set(font_size, 18.0).build();
        let style = StyleCascadeBuilder::new()
            .push_style(StyleOrigin::Override, style)
            .build();

        let mut parent = TestElement::new(1, None);
        parent.store.set_local(font_size, 16.0);

        let child = TestElement::new(2, Some(1));

        let elements: BTreeMap<u32, &TestElement> =
            [(1, &parent), (2, &child)].into_iter().collect();

        let cx = ResolveCx::new(&registry, &theme, |key| {
            elements
                .get(&key)
                .map(|e| (e.property_store(), e.parent_key()))
        });

        let value = cx.get_value(&child, &SelectorInputs::EMPTY, font_size, Some(&style));
        assert_eq!(value, 18.0);
    }

    #[test]
    fn resolve_with_theme_resource() {
        use crate::ResourceKey;

        const ACCENT_WIDTH: ResourceKey = ResourceKey::new(0);

        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let theme = ThemeBuilder::new().set(ACCENT_WIDTH, 75.0_f64).build();
        let element = TestElement::new(1, None);

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value = cx.get_value_with_theme(
            &element,
            &SelectorInputs::EMPTY,
            width,
            None,
            Some(ACCENT_WIDTH),
        );
        assert_eq!(value, 75.0);
    }

    #[test]
    fn resolve_with_theme_resource_ref_borrows_from_theme() {
        use crate::ResourceKey;

        const ACCENT_WIDTH: ResourceKey = ResourceKey::new(0);

        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let theme = ThemeBuilder::new().set(ACCENT_WIDTH, 75.0_f64).build();
        let element = TestElement::new(1, None);

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value_ref = cx.get_value_with_theme_ref(
            &element,
            &SelectorInputs::EMPTY,
            width,
            None,
            Some(ACCENT_WIDTH),
        );
        assert!(core::ptr::eq(
            value_ref,
            theme.get::<f64>(ACCENT_WIDTH).unwrap()
        ));
        assert_eq!(*value_ref, 75.0);
    }

    #[test]
    fn resolve_local_over_theme() {
        use crate::ResourceKey;

        const ACCENT_WIDTH: ResourceKey = ResourceKey::new(0);

        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let theme = ThemeBuilder::new().set(ACCENT_WIDTH, 75.0_f64).build();
        let mut element = TestElement::new(1, None);
        element.store.set_local(width, 100.0);

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value = cx.get_value_with_theme(
            &element,
            &SelectorInputs::EMPTY,
            width,
            None,
            Some(ACCENT_WIDTH),
        );
        assert_eq!(value, 100.0);
    }

    #[test]
    fn resolve_style_over_theme() {
        use crate::ResourceKey;

        const ACCENT_WIDTH: ResourceKey = ResourceKey::new(0);

        let mut registry = PropertyRegistry::new();
        let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

        let theme = ThemeBuilder::new().set(ACCENT_WIDTH, 75.0_f64).build();
        let style = StyleBuilder::new().set(width, 50.0).build();
        let style = StyleCascadeBuilder::new()
            .push_style(StyleOrigin::Override, style)
            .build();
        let element = TestElement::new(1, None);

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value = cx.get_value_with_theme(
            &element,
            &SelectorInputs::EMPTY,
            width,
            Some(&style),
            Some(ACCENT_WIDTH),
        );
        assert_eq!(value, 50.0);
    }

    #[test]
    fn resolve_theme_over_inherited() {
        use crate::ResourceKey;

        const ACCENT_SIZE: ResourceKey = ResourceKey::new(0);

        let mut registry = PropertyRegistry::new();
        let font_size = registry.register(
            "FontSize",
            PropertyMetadataBuilder::new(12.0_f64)
                .inherits(true)
                .build(),
        );

        let theme = ThemeBuilder::new().set(ACCENT_SIZE, 18.0_f64).build();

        let mut parent = TestElement::new(1, None);
        parent.store.set_local(font_size, 16.0);

        let child = TestElement::new(2, Some(1));

        let elements: BTreeMap<u32, &TestElement> =
            [(1, &parent), (2, &child)].into_iter().collect();

        let cx = ResolveCx::new(&registry, &theme, |key| {
            elements
                .get(&key)
                .map(|e| (e.property_store(), e.parent_key()))
        });

        let value = cx.get_value_with_theme(
            &child,
            &SelectorInputs::EMPTY,
            font_size,
            None,
            Some(ACCENT_SIZE),
        );
        assert_eq!(value, 18.0);
    }

    #[test]
    fn cx_accessors() {
        let registry = PropertyRegistry::new();
        let theme = ThemeBuilder::new().build();

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);

        // Can access registry and theme
        assert_eq!(cx.registry().len(), 0);
        assert!(cx.theme().is_empty());
    }

    #[test]
    fn resolve_string_local_ref() {
        let mut registry = PropertyRegistry::new();
        let text = registry.register("Text", PropertyMetadataBuilder::new(String::new()).build());

        let theme = ThemeBuilder::new().build();

        let mut element = TestElement::new(1, None);
        element.store.set_local(text, String::from("hello world"));

        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        let value_ref = cx.get_value_ref(&element, &SelectorInputs::EMPTY, text, None);
        assert!(core::ptr::eq(
            value_ref,
            element.property_store().get_local(text).unwrap()
        ));
        assert_eq!(value_ref.as_str(), "hello world");
    }

    /// Asserts `ResolveCx::get_value` matches `DependencyObjectExt::get_inherited`
    /// when style=None and theme is empty for an inheriting property.
    /// This prevents precedence drift between the two APIs.
    #[test]
    fn resolve_matches_get_inherited() {
        use understory_property::DependencyObjectExt;

        let mut registry = PropertyRegistry::new();
        let font_size = registry.register(
            "FontSize",
            PropertyMetadataBuilder::new(12.0_f64)
                .inherits(true)
                .build(),
        );

        let theme = ThemeBuilder::new().build();

        // Build a 3-level hierarchy: grandparent -> parent -> child
        let mut grandparent = TestElement::new(1, None);
        grandparent.store.set_local(font_size, 24.0);

        let mut parent = TestElement::new(2, Some(1));
        parent.store.set_animation(font_size, 18.0); // Animation at parent level

        let child = TestElement::new(3, Some(2));

        let elements: BTreeMap<u32, &TestElement> = [(1, &grandparent), (2, &parent), (3, &child)]
            .into_iter()
            .collect();

        let store_lookup = |key| {
            elements
                .get(&key)
                .map(|e| (e.property_store(), e.parent_key()))
        };

        // ResolveCx::get_value with no style
        let cx = ResolveCx::new(&registry, &theme, store_lookup);
        let cx_value = cx.get_value(&child, &SelectorInputs::EMPTY, font_size, None);

        // DependencyObjectExt::get_inherited
        let ext_value = child.get_inherited(font_size, &registry, &|key| {
            elements
                .get(&key)
                .map(|e| (e.property_store(), e.parent_key()))
        });

        // Both should return the same value (parent's animation: 18.0)
        assert_eq!(cx_value, ext_value);
        assert_eq!(cx_value, 18.0);
    }
}
