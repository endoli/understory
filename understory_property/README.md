<div align="center">

# Understory Property

**WPF/WinUI-style dependency property system with layered value resolution**

[![Latest published version.](https://img.shields.io/crates/v/understory_property.svg)](https://crates.io/crates/understory_property)
[![Documentation build status.](https://img.shields.io/docsrs/understory_property.svg)](https://docs.rs/understory_property)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_property
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Property: Core dependency property storage.

This crate provides the foundation for a dependency property system,
handling local and animation value storage. Style resolution, theme
support, and full precedence handling are provided by `understory_style`.

## Core Concepts

### Property Storage

[`PropertyStore`] holds Local and Animation values per-object:

- **Local** - explicitly set values
- **Animation** - temporary animation overrides (highest precedence)

Inheritance is handled by [`DependencyObjectExt::get_inherited`].
Style and theme resolution are handled externally by `understory_style`.

### Key Operations

- `set_local(property, value)` - set a local value
- `set_animation(property, value)` - set an animation value
- `get_effective_local(property, registry)` - Animation → Local → default

## Quick Start

```rust
use understory_property::{
    Property, PropertyMetadataBuilder, PropertyRegistry, PropertyStore,
};
use understory_dirty::Channel;

const LAYOUT: Channel = Channel::new(0);

// Create a registry and register properties
let mut registry = PropertyRegistry::new();
let width: Property<f64> = registry.register(
    "Width",
    PropertyMetadataBuilder::new(0.0_f64)
        .affects_channels(LAYOUT.into_set())
        .build()
);

// Create a property store for an object
let mut store = PropertyStore::<u32>::new(1);

// Set and get local values
store.set_local(width, 100.0);
assert_eq!(store.get_local(width), Some(&100.0));

// Get effective value (Animation → Local → default)
let effective = store.get_effective_local(width, &registry);
assert_eq!(effective, 100.0);

// Animation overrides local
store.set_animation(width, 200.0);
let effective = store.get_effective_local(width, &registry);
assert_eq!(effective, 200.0);
```

## Memory Optimizations

| Optimization | Description |
|--------------|-------------|
| **Sparse storage** | `PropertyStore` only allocates for non-default properties |
| **Shared defaults** | Default values stored in registry, not per-object |
| **Inline storage** | `SmallVec` for small property counts |
| **`PropertyId` as u16** | Compact property identification |

## Inheritance

[`DependencyObjectExt::get_inherited`] provides inheritance resolution
by walking the parent chain. This is separate from style resolution.

## `no_std` Support

This crate is `no_std` and uses `alloc`. It does not depend on `std`.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE] or <http://www.apache.org/licenses/LICENSE-2.0>)

<!-- Needs to be defined here for rustdoc's benefit -->
[LICENSE]: LICENSE
