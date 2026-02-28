// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Understory Style: Style and theme resolution for dependency properties.
//!
//! This crate extends `understory_property` with style and theme support,
//! providing the full WinUI-style precedence chain:
//!
//! **Animation → Local → Style → Theme → Inherited → Default**
//!
//! ## Core Concepts
//!
//! ### Styles
//!
//! [`Style`] is a shared collection of property setters. Unlike per-element
//! storage, styles are immutable after creation and can be shared across
//! many elements—matching `WinUI`'s `OptimizedStyle` approach.
//!
//! ```rust
//! use understory_style::{Style, StyleBuilder};
//! use understory_property::{PropertyMetadataBuilder, PropertyRegistry};
//!
//! let mut registry = PropertyRegistry::new();
//! let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
//! let height = registry.register("Height", PropertyMetadataBuilder::new(0.0_f64).build());
//!
//! // Create a shared style
//! let button_style = StyleBuilder::new()
//!     .set(width, 100.0)
//!     .set(height, 40.0)
//!     .build();
//!
//! // Multiple elements can reference the same style
//! assert_eq!(button_style.get(width), Some(&100.0));
//! ```
//!
//! ### Themes
//!
//! [`Theme`] provides resource lookup by key. Themes map resource keys to
//! typed values, enabling theming (light/dark modes, brand colors, etc.).
//!
//! ```rust
//! use understory_style::{Theme, ThemeBuilder, ResourceKey};
//!
//! // Define resource keys as constants
//! const ACCENT_COLOR: ResourceKey = ResourceKey::new(0);
//! const FONT_SIZE: ResourceKey = ResourceKey::new(1);
//!
//! let light_theme = ThemeBuilder::new()
//!     .set(ACCENT_COLOR, 0x0078D4_u32)  // Blue
//!     .set(FONT_SIZE, 14.0_f64)
//!     .build();
//!
//! let dark_theme = ThemeBuilder::new()
//!     .set(ACCENT_COLOR, 0x4CC2FF_u32)  // Light blue
//!     .set(FONT_SIZE, 14.0_f64)
//!     .build();
//!
//! assert_eq!(light_theme.get::<u32>(ACCENT_COLOR), Some(&0x0078D4));
//! ```
//!
//! ### Resolution Context
//!
//! [`ResolveCx`] bundles everything needed to resolve property values
//! through the full precedence chain. This avoids passing many parameters
//! to resolution functions.
//!
//! ```rust
//! use understory_style::{
//!     ClassId, IdSet, PseudoClassId, ResolveCx, Selector, SelectorInputs, StyleCascade,
//!     StyleCascadeBuilder, StyleBuilder, StyleOrigin, StyleSheetBuilder, ThemeBuilder,
//! };
//! use understory_property::{
//!     DependencyObject, PropertyMetadataBuilder, PropertyRegistry, PropertyStore,
//! };
//!
//! let mut registry = PropertyRegistry::new();
//! let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
//!
//! let theme = ThemeBuilder::new().build();
//!
//! const PRIMARY: ClassId = ClassId(1);
//! const HOVER: PseudoClassId = PseudoClassId(1);
//!
//! // Base style for a "button"
//! let base = StyleBuilder::new().set(width, 100.0).build();
//! // Hover style when PRIMARY + HOVER
//! let hover = StyleBuilder::new().set(width, 120.0).build();
//!
//! let hover_selector = Selector {
//!     type_tag: None,
//!     required_classes: IdSet::from_ids([PRIMARY]),
//!     required_pseudos: IdSet::from_ids([HOVER]),
//! };
//!
//! let sheet = StyleSheetBuilder::new()
//!     .rule(hover_selector, hover)
//!     .build();
//!
//! let style: StyleCascade = StyleCascadeBuilder::new()
//!     .push_style(StyleOrigin::Base, base)
//!     .push_sheet(StyleOrigin::Sheet, sheet)
//!     .build();
//!
//! struct Element {
//!     key: u32,
//!     parent: Option<u32>,
//!     store: PropertyStore<u32>,
//!     style: Option<StyleCascade>,
//! }
//!
//! impl DependencyObject<u32> for Element {
//!     fn property_store(&self) -> &PropertyStore<u32> { &self.store }
//!     fn property_store_mut(&mut self) -> &mut PropertyStore<u32> { &mut self.store }
//!     fn key(&self) -> u32 { self.key }
//!     fn parent_key(&self) -> Option<u32> { self.parent }
//! }
//!
//! let element = Element {
//!     key: 1,
//!     parent: None,
//!     store: PropertyStore::new(1),
//!     style: Some(style.clone()),
//! };
//!
//! // Create resolution context
//! let cx = ResolveCx::new(&registry, &theme, |_key| None);
//!
//! // Resolve with style (no hover)
//! let inputs = SelectorInputs::new(None, &[PRIMARY], &[]);
//! let value = cx.get_value(&element, &inputs, width, element.style.as_ref());
//! assert_eq!(value, 100.0);
//!
//! // Resolve with style (hovered)
//! let hovered = SelectorInputs::new(None, &[PRIMARY], &[HOVER]);
//! let value = cx.get_value(&element, &hovered, width, element.style.as_ref());
//! assert_eq!(value, 120.0);
//! ```
//!
//! ## `no_std` Support
//!
//! This crate is `no_std` and uses `alloc`. It does not depend on `std`.

#![no_std]

extern crate alloc;

mod resolve;
mod selector;
mod style;
mod stylesheet;
mod theme;

pub use resolve::ResolveCx;
pub use selector::{ClassId, IdSet, PseudoClassId, Selector, SelectorInputs, Specificity, TypeTag};
pub use style::{Style, StyleBuilder};
pub use stylesheet::{
    StyleCascade, StyleCascadeBuilder, StyleOrigin, StyleRule, StyleSheet, StyleSheetBuilder,
    StyleSource,
};
pub use theme::{ResourceKey, Theme, ThemeBuilder};
