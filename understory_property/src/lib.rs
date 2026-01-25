// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Understory Property: Core dependency property storage.
//!
//! This crate provides the foundation for a dependency property system,
//! handling local and animation value storage. Style resolution, theme
//! support, and full precedence handling are provided by `understory_style`.
//!
//! ## Core Concepts
//!
//! ### Property Storage
//!
//! [`PropertyStore`] holds Local and Animation values per-object:
//!
//! - **Local** - explicitly set values
//! - **Animation** - temporary animation overrides (highest precedence)
//!
//! Inheritance is handled by [`DependencyObjectExt::get_inherited`].
//! Style and theme resolution are handled externally by `understory_style`.
//!
//! ### Key Operations
//!
//! - `set_local(property, value)` - set a local value
//! - `set_animation(property, value)` - set an animation value
//! - `get_effective_local(property, registry)` - Animation → Local → default
//!
//! ## Quick Start
//!
//! ```rust
//! use understory_property::{
//!     Property, PropertyMetadataBuilder, PropertyRegistry, PropertyStore,
//! };
//! use understory_dirty::Channel;
//!
//! const LAYOUT: Channel = Channel::new(0);
//!
//! // Create a registry and register properties
//! let mut registry = PropertyRegistry::new();
//! let width: Property<f64> = registry.register(
//!     "Width",
//!     PropertyMetadataBuilder::new(0.0_f64)
//!         .affects_channels(LAYOUT.into_set())
//!         .build()
//! );
//!
//! // Create a property store for an object
//! let mut store = PropertyStore::<u32>::new(1);
//!
//! // Set and get local values
//! store.set_local(width, 100.0);
//! assert_eq!(store.get_local(width), Some(&100.0));
//!
//! // Get effective value (Animation → Local → default)
//! let effective = store.get_effective_local(width, &registry);
//! assert_eq!(effective, 100.0);
//!
//! // Animation overrides local
//! store.set_animation(width, 200.0);
//! let effective = store.get_effective_local(width, &registry);
//! assert_eq!(effective, 200.0);
//! ```
//!
//! ## Memory Optimizations
//!
//! | Optimization | Description |
//! |--------------|-------------|
//! | **Sparse storage** | `PropertyStore` only allocates for non-default properties |
//! | **Shared defaults** | Default values stored in registry, not per-object |
//! | **Inline storage** | `SmallVec` for small property counts |
//! | **`PropertyId` as u16** | Compact property identification |
//!
//! ## Inheritance
//!
//! [`DependencyObjectExt::get_inherited`] provides inheritance resolution
//! by walking the parent chain. This is separate from style resolution.
//!
//! ## `no_std` Support
//!
//! This crate is `no_std` and uses `alloc`. It does not depend on `std`.

#![no_std]

extern crate alloc;

mod id;
mod metadata;
mod object;
mod registry;
mod store;
mod value;

pub use id::{Property, PropertyId};
pub use metadata::{
    CoerceValueCallback, PropertyChangedCallback, PropertyMetadata, PropertyMetadataBuilder,
};
pub use object::{
    DependencyObject, DependencyObjectExt, ParentLookup, walk_inherited, walk_inherited_ref,
};
pub use registry::{PropertyRegistration, PropertyRegistry};
pub use store::PropertyStore;
pub use value::ErasedValue;
