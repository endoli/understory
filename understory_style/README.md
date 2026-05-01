<div align="center">

# Understory Style

**Style and theme resolution for Understory dependency properties**

[![Latest published version.](https://img.shields.io/crates/v/understory_style.svg)](https://crates.io/crates/understory_style)
[![Documentation build status.](https://img.shields.io/docsrs/understory_style.svg)](https://docs.rs/understory_style)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_style
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Style: Style and theme resolution for dependency properties.

This crate extends `understory_property` with style and theme support,
providing the full WinUI-style precedence chain:

**Animation → Local → Style → Theme → Inherited → Default**

### Core Concepts

#### Styles

[`Style`] is a shared collection of property setters. Unlike per-element
storage, styles are immutable after creation and can be shared across
many elements—matching `WinUI`'s `OptimizedStyle` approach.

```rust
use understory_style::{Style, StyleBuilder};
use understory_property::{PropertyMetadataBuilder, PropertyRegistry};

let mut registry = PropertyRegistry::new();
let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
let height = registry.register("Height", PropertyMetadataBuilder::new(0.0_f64).build());

// Create a shared style
let button_style = StyleBuilder::new()
    .set(width, 100.0)
    .set(height, 40.0)
    .build();

// Multiple elements can reference the same style
assert_eq!(button_style.get(width), Some(&100.0));
```

#### Themes

[`Theme`] provides resource lookup by key. Themes map resource keys to
typed values, enabling theming (light/dark modes, brand colors, etc.).

```rust
use understory_style::{Theme, ThemeBuilder, ResourceKey};

// Define resource keys as constants
const ACCENT_COLOR: ResourceKey = ResourceKey::new(0);
const FONT_SIZE: ResourceKey = ResourceKey::new(1);

let light_theme = ThemeBuilder::new()
    .set(ACCENT_COLOR, 0x0078D4_u32)  // Blue
    .set(FONT_SIZE, 14.0_f64)
    .build();

let dark_theme = ThemeBuilder::new()
    .set(ACCENT_COLOR, 0x4CC2FF_u32)  // Light blue
    .set(FONT_SIZE, 14.0_f64)
    .build();

assert_eq!(light_theme.get::<u32>(ACCENT_COLOR), Some(&0x0078D4));
```

#### Resolution Context

[`ResolveCx`] bundles everything needed to resolve property values
through the full precedence chain. This avoids passing many parameters
to resolution functions.

```rust
use understory_style::{
    ClassId, SelectorStep, PseudoClassId, ResolveCx, SelectorInputs, StyleCascade,
    StyleCascadeBuilder, StyleBuilder, StyleOrigin, TargetTag, ThemeBuilder,
};
use understory_property::{
    DependencyObject, PropertyMetadataBuilder, PropertyRegistry, PropertyStore,
};

let mut registry = PropertyRegistry::new();
let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

let theme = ThemeBuilder::new().build();

const PRIMARY: ClassId = ClassId(1);
const HOVER: PseudoClassId = PseudoClassId(1);
const ICON: TargetTag = TargetTag(1);

// Base style for a "button"
let base = StyleBuilder::new().set(width, 100.0).build();
// Hover style when PRIMARY + HOVER
let hover = StyleBuilder::new().set(width, 120.0).build();

let style: StyleCascade = StyleCascadeBuilder::new()
    .push_style(StyleOrigin::Base, base)
    .push_rule(
        StyleOrigin::Sheet,
        SelectorStep::class(PRIMARY).with_pseudo(HOVER),
        hover,
    )
    .build();

struct Element {
    key: u32,
    parent: Option<u32>,
    store: PropertyStore<u32>,
}

impl DependencyObject<u32> for Element {
    fn property_store(&self) -> &PropertyStore<u32> { &self.store }
    fn property_store_mut(&mut self) -> &mut PropertyStore<u32> { &mut self.store }
    fn key(&self) -> u32 { self.key }
    fn parent_key(&self) -> Option<u32> { self.parent }
}

let element = Element {
    key: 1,
    parent: None,
    store: PropertyStore::new(1),
};

// Create resolution context
let cx = ResolveCx::new(&registry, &theme, |_key| None);

// Resolve with style (no hover)
let inputs = SelectorInputs::new(None, &[PRIMARY], &[]);
let state = style.enter_subject(style.root_state(), &inputs);
let value = cx.get_value(&element, width, Some((&style, state)));
assert_eq!(value, 100.0);

// Resolve with style (hovered)
let hovered = SelectorInputs::new(None, &[PRIMARY], &[HOVER]);
let hovered_state = style.enter_subject(style.root_state(), &hovered);
let value = cx.get_value(&element, width, Some((&style, hovered_state)));
assert_eq!(value, 120.0);

// Targets are owner-local style addresses supplied by the embedder.
let icon_inputs = SelectorInputs::with_target(None, Some(ICON), &[PRIMARY], &[]);
assert_eq!(icon_inputs.target_tag, Some(ICON));
```

#### Path Matching And Style Changes

[`StyleCascade`] is path-aware. Embedders walk their own style subject tree
and carry a compact [`MatchState`] from parent to child. A `MatchState` is
valid only with the cascade that produced it:

```rust
use invalidation::Channel;
use understory_property::{PropertyMetadataBuilder, PropertyRegistry};
use understory_style::{
    PseudoClassId, SelectorInputs, SelectorStep, StyleBuilder, StyleCascadeBuilder, StyleOrigin,
    TargetTag, TypeTag,
};

const PAINT: Channel = Channel::new(1);
const TOGGLE: TypeTag = TypeTag(1);
const TRACK: TargetTag = TargetTag(2);
const CHECKED: PseudoClassId = PseudoClassId(3);

let mut registry = PropertyRegistry::new();
let background = registry.register(
    "Background",
    PropertyMetadataBuilder::new(0_u32)
        .affects_channels(PAINT.into_set())
        .build(),
);

let cascade = StyleCascadeBuilder::new()
    .push_rule(
        StyleOrigin::Sheet,
        [
            SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
            SelectorStep::target_tag(TRACK),
        ],
        StyleBuilder::new().set(background, 0x00ff00_u32).build(),
    )
    .build();

let checked = [CHECKED];
let unchecked_root = cascade.enter_subject(
    cascade.root_state(),
    &SelectorInputs::new(Some(TOGGLE), &[], &[]),
);
let checked_root = cascade.enter_subject(
    cascade.root_state(),
    &SelectorInputs::new(Some(TOGGLE), &[], &checked),
);
let unchecked_track = cascade.enter_subject(
    unchecked_root,
    &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
);
let checked_track = cascade.enter_subject(
    checked_root,
    &SelectorInputs::with_target(None, Some(TRACK), &[], &[]),
);

let changed = cascade.changed_properties(unchecked_track, checked_track);
assert_eq!(changed.property_ids(), &[background.id()]);
assert!(changed.affected_channels(&registry).contains(PAINT));
```

Selectors are exact paths in this first grammar. Each [`SelectorStep`]
corresponds to one call to [`StyleCascade::enter_subject`], so selectors do
not skip intermediate structural subjects. [`StyleCascade::changed_properties`]
is conservative and reports properties whose winning style source changes;
it does not compare concrete typed values for equality.

### `no_std` Support

This crate is `no_std` and uses `alloc`. It does not depend on `std`.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE] or <http://www.apache.org/licenses/LICENSE-2.0>)

<!-- Needs to be defined here for rustdoc's benefit -->
[LICENSE]: LICENSE
