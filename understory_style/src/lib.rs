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
//! use understory_style::{ResolveCx, Style, Theme, ThemeBuilder, StyleBuilder};
//! use understory_property::{
//!     DependencyObject, PropertyMetadataBuilder, PropertyRegistry, PropertyStore,
//! };
//!
//! let mut registry = PropertyRegistry::new();
//! let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
//!
//! let style = StyleBuilder::new().set(width, 100.0).build();
//! let theme = ThemeBuilder::new().build();
//!
//! struct Element {
//!     key: u32,
//!     parent: Option<u32>,
//!     store: PropertyStore<u32>,
//!     style: Option<Style>,
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
//! // Resolve with style
//! let value = cx.get_value(&element, width, element.style.as_ref());
//! assert_eq!(value, 100.0);
//! ```
//!
//! ## `no_std` Support
//!
//! This crate is `no_std` and uses `alloc`. It does not depend on `std`.

#![no_std]

extern crate alloc;

mod resolve;
mod style;
mod theme;

pub use resolve::ResolveCx;
pub use style::{Style, StyleBuilder};
pub use theme::{ResourceKey, Theme, ThemeBuilder};
