<div align="center">

# Understory Outline

**Hierarchical visible-row projection and expansion state primitives**

[![Latest published version.](https://img.shields.io/crates/v/understory_outline.svg)](https://crates.io/crates/understory_outline)
[![Documentation build status.](https://img.shields.io/docsrs/understory_outline.svg)](https://docs.rs/understory_outline)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_outline
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Outline: hierarchical visible-row projection primitives.

This crate provides a small, renderer-agnostic core for projecting your
hierarchical domain data into a stable visible row sequence with explicit
expansion state.

It is aimed at expandable tree views, grouped property grids, disclosure
panels, and other UIs that need to operate over an existing model rather
than a widget-owned tree structure:

- stable row keys,
- depth information,
- explicit expand/collapse state,
- a flat visible row sequence that higher layers can render or virtualize.

The core concepts are:

- [`OutlineModel`]: a trait describing hierarchical traversal by stable keys
  from your model.
- [`ExpansionState`]: a small container tracking which keys are currently expanded.
- [`Outline`]: a controller that owns a model, caches visible rows, and
  rebuilds that projection when model or expansion state changes.
- [`VisibleRow`]: metadata about one currently visible row.
- [`SliceOutline`]: a dense reference implementation over `usize` keys and
  slice-backed nodes when your data already exists in that form.

This crate deliberately does **not** know about:

- widgets, disclosure triangles, row rendering, or styling,
- selection or focus policy,
- virtualization or scroll math,
- drag/drop or keyboard behavior,
- async loading or mutation orchestration.

## Overview

Goal:
project hierarchical domain data into stable visible rows with explicit
expansion state.

Non-goals:
own rendering, styling, selection, focus, or virtualization.

## Glossary

- **Outline model**: your hierarchical source of truth keyed by stable IDs.
- **Expansion state**: the set of currently expanded keys.
- **Visible row**: one row in the current flattened, visible projection.
- **Depth**: the indentation level of a visible row.
- **Projection**: the flattened visible sequence derived from hierarchy plus expansion state.

## Fence

This crate owns hierarchical visible-row projection and expansion state over
your model; it explicitly does not own rendering, styling, selection, or
virtualization.

## Invariants

Callers and implementations should rely on the following:

- Keys come from the host model, remain meaningful outside the outline, and
  may be stored in external state.
- Root order and sibling order come from the [`OutlineModel`] implementation.
- Collapsing a parent hides descendants but does not discard their stored
  expansion state.
- [`Outline::visible_rows`] may rebuild a cached projection after model or
  expansion changes; that cost is explicit and documented.
- Virtualization belongs above the outline layer: hosts virtualize the
  visible rows produced by this crate.

## Why not bake this into a virtual list?

Expandable outlines are not just “lists with indentation”. They need:

- stable node IDs,
- expansion/collapse state,
- hierarchical traversal over a host model,
- a flattening step from hierarchy to visible rows.

[`understory_virtual_list`](https://docs.rs/understory_virtual_list) is a
better home for viewport math over dense visible rows. This crate is the
layer that creates those rows.

## Minimal example

The intended integration point is your own domain model implementing
[`OutlineModel`], not a separate parallel node array.

```rust
use understory_outline::{Outline, OutlineModel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKey {
    Group(usize),
    Property(usize),
}

struct Group<'a> {
    label: &'a str,
    first_property: Option<usize>,
    next_group: Option<usize>,
}

struct Property<'a> {
    label: &'a str,
    next_property: Option<usize>,
}

struct PropertyGridModel<'a> {
    groups: &'a [Group<'a>],
    properties: &'a [Property<'a>],
}

impl<'a> OutlineModel for PropertyGridModel<'a> {
    type Key = RowKey;
    type Item = &'a str;

    fn first_root_key(&self) -> Option<Self::Key> {
        (!self.groups.is_empty()).then_some(RowKey::Group(0))
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        match *key {
            RowKey::Group(index) => index < self.groups.len(),
            RowKey::Property(index) => index < self.properties.len(),
        }
    }

    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match *key {
            RowKey::Group(index) => self.groups[index].next_group.map(RowKey::Group),
            RowKey::Property(index) => self.properties[index].next_property.map(RowKey::Property),
        }
    }

    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match *key {
            RowKey::Group(index) => self.groups[index].first_property.map(RowKey::Property),
            RowKey::Property(_) => None,
        }
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        match *key {
            RowKey::Group(index) => self.groups.get(index).map(|group| group.label),
            RowKey::Property(index) => self.properties.get(index).map(|property| property.label),
        }
    }
}

let groups = [
    Group {
        label: "Appearance",
        first_property: Some(0),
        next_group: Some(1),
    },
    Group {
        label: "Layout",
        first_property: Some(3),
        next_group: None,
    },
];
let properties = [
    Property {
        label: "Fill",
        next_property: Some(1),
    },
    Property {
        label: "Stroke",
        next_property: Some(2),
    },
    Property {
        label: "Shadow",
        next_property: None,
    },
    Property {
        label: "Width",
        next_property: Some(4),
    },
    Property {
        label: "Height",
        next_property: None,
    },
];
let model = PropertyGridModel {
    groups: &groups,
    properties: &properties,
};
let mut outline = Outline::new(model);

assert!(outline.set_expanded(RowKey::Group(0), true));
let rows = outline.visible_rows();

assert_eq!(rows.len(), 5);
assert_eq!(rows[0].key, RowKey::Group(0));
assert_eq!(rows[1].depth, 1);
assert_eq!(rows[4].key, RowKey::Group(1));
```

## Grouped property-grid style example

Property grids often want group rows plus leaf rows with stable keys and
explicit expand/collapse state derived from their existing data model:

```rust
use understory_outline::{Outline, OutlineModel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKey {
    Group(usize),
    Property(usize),
}

struct Group<'a> {
    label: &'a str,
    first_property: Option<usize>,
}

struct Property<'a> {
    label: &'a str,
    next_property: Option<usize>,
}

struct PropertyGridModel<'a> {
    groups: &'a [Group<'a>],
    properties: &'a [Property<'a>],
}

impl<'a> OutlineModel for PropertyGridModel<'a> {
    type Key = RowKey;
    type Item = &'a str;

    fn first_root_key(&self) -> Option<Self::Key> {
        (!self.groups.is_empty()).then_some(RowKey::Group(0))
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        match *key {
            RowKey::Group(index) => index < self.groups.len(),
            RowKey::Property(index) => index < self.properties.len(),
        }
    }

    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match *key {
            RowKey::Group(index) => (index + 1 < self.groups.len()).then_some(RowKey::Group(index + 1)),
            RowKey::Property(index) => self.properties[index].next_property.map(RowKey::Property),
        }
    }

    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match *key {
            RowKey::Group(index) => self.groups[index].first_property.map(RowKey::Property),
            RowKey::Property(_) => None,
        }
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        match *key {
            RowKey::Group(index) => self.groups.get(index).map(|group| group.label),
            RowKey::Property(index) => self.properties.get(index).map(|property| property.label),
        }
    }
}

let groups = [Group {
    label: "Transform",
    first_property: Some(0),
}];
let properties = [
    Property {
        label: "Position",
        next_property: Some(1),
    },
    Property {
        label: "Rotation",
        next_property: Some(2),
    },
    Property {
        label: "Scale",
        next_property: None,
    },
];
let model = PropertyGridModel {
    groups: &groups,
    properties: &properties,
};
let mut outline = Outline::new(model);

assert_eq!(outline.visible_len(), 1);
assert!(outline.set_expanded(RowKey::Group(0), true));
assert_eq!(outline.visible_len(), 4);

let keys: Vec<_> = outline.visible_rows().iter().map(|row| row.key).collect();
let labels: Vec<_> = keys
    .iter()
    .map(|key| outline.item(key).expect("row key should resolve"))
    .collect();
assert_eq!(labels, vec!["Transform", "Position", "Rotation", "Scale"]);
```

## Dense reference model

[`SliceOutline`] exists for dense index-addressable data and as a compact
reference model for tests and adapters. It is not meant to imply that host
applications should mirror their domain into a separate outline-only node
structure if they already have a suitable model.

## Virtualization composition

A host that wants a virtualized tree view should:

1. use [`Outline`] to produce visible rows,
2. feed `visible_len()` into `understory_virtual_list`,
3. render only the visible row indices returned by the virtual list.

The runnable example `outline_virtual_list` in the top-level examples crate
shows that composition.

## Extension points

The most likely next additions are:

- a parent-link capability trait for ancestor/path helpers,
- richer row metadata if real hosts need sibling-position hints,
- a more specialized reference model if one representation repeats often.

## Gotchas

- `OutlineModel` implementations must not introduce cycles.
- If the model changes via interior mutability, call [`Outline::mark_dirty`]
  before querying visible rows again.
- This crate does not promise incremental diffing yet; it currently rebuilds
  the visible row projection when dirty.

This crate is `no_std` and uses `alloc`.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE] or <http://www.apache.org/licenses/LICENSE-2.0>), or
- MIT license ([LICENSE-MIT] or <http://opensource.org/licenses/MIT>),

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Contribution

Contributions are welcome by pull request. The [Rust code of conduct] applies.
Please feel free to add your name to the [AUTHORS] file in any substantive pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[LICENSE-APACHE]: ../LICENSE-APACHE
[LICENSE-MIT]: ../LICENSE-MIT
[Rust code of conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: ../AUTHORS
