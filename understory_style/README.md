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

## Start Here

Use this crate when your application already owns a tree of UI, template, or
model nodes and needs style matching over that tree. `understory_style` does
not store the tree. The embedder walks its own style subjects and carries a
compact [`MatchState`] from parent to child.

The crate owns:

- selector matching over child and descendant paths;
- style and theme values for dependency properties;
- conservative style invalidation through `invalidation` channels;
- inspection hooks for matched rules and winning style sources.

The crate does not own widgets, templates, layout, rendering, event
dispatch, sibling relationships, parent queries, or structural selectors
such as `nth-*`, `odd`, or `even`.

### Glossary

- **Style subject**: one addressable item in the embedder's walk. It may be
  an element, generated template node, model row, or widget part.
- **`TypeTag`**: an application-defined subject kind, such as `Button`,
  `Toggle`, or `Row`.
- **`PartTag`**: an owner-local part label, such as `track`, `thumb`, or
  `icon`.
- **`SelectorInputs`**: the type, part, class, and pseudoclass snapshot for
  one subject.
- **`MatchState`**: matcher progress after entering a subject. It is valid
  only with the cascade that produced it.

### First Example: Owner State Styling A Part

This styles a `Toggle` owner's `track` part when the owner has `:checked`.
The checked state stays on the owner; it is not copied into the part inputs.

```rust
use invalidation::Channel;
use understory_property::{PropertyMetadataBuilder, PropertyRegistry};
use understory_style::{
    PartTag, PseudoClassId, Selector, SelectorInputs, SelectorStep, StyleBuilder,
    StyleCascadeBuilder, StyleOrigin, TypeTag,
};

const PAINT: Channel = Channel::new(1);
const TOGGLE: TypeTag = TypeTag(1);
const TRACK: PartTag = PartTag(1);
const CHECKED: PseudoClassId = PseudoClassId(1);

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
        Selector::child(
            SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
            SelectorStep::part_tag(TRACK),
        ),
        StyleBuilder::new().set(background, 0x00ff00_u32).build(),
    )
    .build();

let unchecked_owner = cascade.enter_subject(
    cascade.root_state(),
    &SelectorInputs::typed(TOGGLE),
);
let unchecked_track = cascade.enter_subject(
    unchecked_owner,
    &SelectorInputs::part(TRACK),
);

let checked = [CHECKED];
let checked_owner = cascade.enter_subject(
    cascade.root_state(),
    &SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
);
let restyle = cascade.restyle_subject(
    &registry,
    unchecked_track,
    checked_owner,
    &SelectorInputs::part(TRACK),
);

assert_eq!(cascade.get_value_ref(restyle.state(), background), Some(&0x00ff00));
assert!(restyle.changed_channels().contains(PAINT));
assert_eq!(cascade.matching_rules(restyle.state()).count(), 1);
assert!(cascade.winning_source(restyle.state(), background).unwrap().rule().is_some());
```

### Long-Lived Rules Of Thumb

Anchor part selectors under an owner [`TypeTag`]. [`PartTag`] values are
application-defined and may be reused by unrelated owners:

```rust
use understory_style::{PartTag, Selector, SelectorStep, TypeTag};

const BUTTON: TypeTag = TypeTag(1);
const ROW: TypeTag = TypeTag(2);
const LOCAL_PART: PartTag = PartTag(1);

let button_part = Selector::child(
    SelectorStep::type_tag(BUTTON),
    SelectorStep::part_tag(LOCAL_PART),
);
let row_part = Selector::child(
    SelectorStep::type_tag(ROW),
    SelectorStep::part_tag(LOCAL_PART),
);

assert_ne!(button_part.steps()[0], row_part.steps()[0]);
```

Use [`SelectorInputsOwned`] when classes or pseudoclasses come from unsorted
application data. It sorts and deduplicates before exposing borrowed
[`SelectorInputs`].

For integration debugging, use [`StyleCascade::matching_rules`],
[`StyleCascade::winning_source`], and [`Selector::diagnose_path`]. These are
deliberately small diagnostics for the current child / descendant grammar,
not a browser-CSS explanation engine.

## Reference Concepts

### Styles

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

### Themes

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

### Resolution Context

[`ResolveCx`] bundles everything needed to resolve property values
through the full precedence chain. This avoids passing many parameters
to resolution functions.

```rust
use understory_style::{
    ClassId, SelectorStep, PseudoClassId, ResolveCx, SelectorInputs, StyleCascade,
    StyleCascadeBuilder, StyleBuilder, StyleOrigin, PartTag, ThemeBuilder,
};
use understory_property::{
    DependencyObject, PropertyMetadataBuilder, PropertyRegistry, PropertyStore,
};

let mut registry = PropertyRegistry::new();
let width = registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());

let theme = ThemeBuilder::new().build();

const PRIMARY: ClassId = ClassId(1);
const HOVER: PseudoClassId = PseudoClassId(1);
const ICON: PartTag = PartTag(1);

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

// Parts are owner-local style addresses supplied by the embedder.
let icon_inputs = SelectorInputs::with_part(None, Some(ICON), &[PRIMARY], &[]);
assert_eq!(icon_inputs.part_tag, Some(ICON));
```

[`PartTag`] values are application-defined. In UI code, prefer anchoring
part selectors under an owner [`TypeTag`] (for example, `Button > icon`) so
unrelated widgets can reuse local part IDs without colliding.

### Path Matching And Style Changes

[`StyleCascade`] is path-aware. Embedders walk their own style subject tree
and carry a compact [`MatchState`] from parent to child. A `MatchState` is
valid only with the cascade that produced it.

```rust
use invalidation::Channel;
use understory_property::{PropertyMetadataBuilder, PropertyRegistry};
use understory_style::{
    PseudoClassId, Selector, SelectorInputs, SelectorStep, StyleBuilder,
    StyleCascadeBuilder, StyleOrigin, PartTag, TypeTag,
};

const PAINT: Channel = Channel::new(1);
const TOGGLE: TypeTag = TypeTag(1);
const TRACK: PartTag = PartTag(2);
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
            SelectorStep::part_tag(TRACK),
        ],
        StyleBuilder::new().set(background, 0x00ff00_u32).build(),
    )
    .build();

let checked = [CHECKED];
let unchecked_root = cascade.enter_subject(
    cascade.root_state(),
    &SelectorInputs::typed(TOGGLE),
);
let checked_root = cascade.enter_subject(
    cascade.root_state(),
    &SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
);
let unchecked_track = cascade.enter_subject(
    unchecked_root,
    &SelectorInputs::part(TRACK),
);
let checked_track = cascade.enter_subject(
    checked_root,
    &SelectorInputs::part(TRACK),
);

let changed = cascade.changed_properties(unchecked_track, checked_track);
assert_eq!(changed.property_ids(), &[background.id()]);
assert!(changed.affected_channels(&registry).contains(PAINT));

let descendant = Selector::descendant(
    SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
    SelectorStep::part_tag(TRACK),
);
assert!(descendant.matches_path(&[
    SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
    SelectorInputs::with_part(None, Some(PartTag(99)), &[], &[]),
    SelectorInputs::part(TRACK),
]));
```

Plain selector arrays are exact child paths. For fallback relationships where
a step may appear deeper in the subject tree, use [`SelectorCombinator::Descendant`].
The current grammar is intentionally limited to child and descendant
relationships. It does not include sibling selectors, `nth-*` selectors,
parent queries, or structural `odd`/`even` selectors. Embedders that need
structural state today should compute that state themselves and expose it as
classes or pseudoclasses:

```rust
use understory_style::{ClassId, PartTag, Selector, SelectorStep, TypeTag};

const ROW: TypeTag = TypeTag(1);
const TEXT: PartTag = PartTag(2);
const ODD: ClassId = ClassId(3);

let odd_row_text = Selector::from([
    SelectorStep::type_tag(ROW).with_class(ODD),
    SelectorStep::part_tag(TEXT),
]);
assert_eq!(odd_row_text.len(), 2);
```

[`StyleCascade::changed_properties`] is conservative and reports properties
whose winning style source changes; it does not compare concrete typed
values for equality.

For inspection and update loops, [`StyleCascade`] also exposes
[`StyleCascade::matching_rules`], [`StyleCascade::winning_source`], and
[`StyleCascade::restyle_subject`]. For selector authoring diagnostics, use
[`Selector::diagnose_path`] to get the first path mismatch.

## `no_std` Support

This crate is `no_std` and uses `alloc`. It does not depend on `std`.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE] or <http://www.apache.org/licenses/LICENSE-2.0>)

<!-- Needs to be defined here for rustdoc's benefit -->
[LICENSE]: LICENSE
