<div align="center">

# Understory Selection

**Selection management primitives for lists, canvases, and infinite-surface UIs**

[![Latest published version.](https://img.shields.io/crates/v/understory_selection.svg)](https://crates.io/crates/understory_selection)
[![Documentation build status.](https://img.shields.io/docsrs/understory_selection.svg)](https://docs.rs/understory_selection)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_selection
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Selection: selection management primitives.

This crate focuses on the _bookkeeping_ of a selection: the set of selected
keys plus common higher-level concepts such as a primary item and an anchor
used for range extension. It does **not** know anything about how your items
are laid out or ordered; callers decide how to map user input (click, toggle,
lasso) into concrete sets of keys.

The core type is [`Selection`], a small, generic container that tracks:
- The set of selected keys.
- An optional **primary** key (typically the most recently interacted-with item).
- An optional **anchor** key (used as a reference point for shift-click/range selection).
- A monotonically increasing **revision** counter that bumps when the
  selection changes.

The container is intentionally opinionated and compact:
- Keys live in a small `Vec<K>` with uniqueness enforced by equality.
- No hashing or ordering constraints are imposed on `K`, making it easy to integrate
  with existing ID types such as generational handles from a scene tree.
- The API exposes simple operations that mirror common UI gestures like
  “replace with a single item”, “toggle one item”, and “replace/extend with a batch”.

## Minimal example

```rust
use understory_selection::Selection;

// Using u32 as a stand-in for an application-specific ID.
let mut selection = Selection::<u32>::new();

// Simple click: replace selection with a single item.
selection.select_only(10);
assert_eq!(selection.primary(), Some(&10));

// Ctrl-click: toggle a single item.
selection.toggle(10);
assert!(selection.is_empty());

// Lasso or range gesture: compute the affected IDs elsewhere and
// then replace the current selection with that batch.
selection.replace_with([1, 2, 3]);
assert_eq!(selection.len(), 3);
```

## Concepts

[`Selection`] models three related pieces of state:

- **Selection contents**: a set of keys, stored as a small `Vec<K>` with no duplicates.
- **Primary**: an optional distinguished key, typically the most recently interacted-with
  item. Many UIs use this as the “focus” of keyboard actions or the reference for
  commands like “delete selection”.
- **Anchor**: an optional reference key used as a starting point for range extension
  (for example, shift-click in a list). The crate does not know how items are ordered;
  callers are expected to compute ranges based on their own data structures and then
  call methods like [`Selection::replace_with`] or [`Selection::extend_with`].

The container is agnostic to the domain: it works equally well for list selections,
canvas/infinite-surface editors, or any other place where you want to track a set of
selected items plus a primary/anchor.

## List-style click helpers

Higher layers typically map pointer + modifier input into selection changes. For a
simple list with `click` / `ctrl+click` / `shift+click` semantics, you might write
a helper like this:

```rust
use understory_selection::Selection;

#[derive(Default, Copy, Clone)]
struct Modifiers {
    ctrl: bool,
    shift: bool,
}

fn handle_click(
    selection: &mut Selection<u32>,
    clicked: u32,
    mods: Modifiers,
    items_in_order: &[u32],
) {
    if !mods.ctrl && !mods.shift {
        // Plain click: replace selection with a single item.
        selection.select_only(clicked);
        return;
    }

    if mods.ctrl && !mods.shift {
        // Ctrl-click: toggle membership, keep anchor stable.
        selection.toggle(clicked);
        return;
    }

    if mods.shift {
        // Shift-click: treat anchor as the pivot, build a range between
        // anchor and the clicked item according to the list ordering, and
        // replace the current selection with that range.
        let anchor = selection
            .anchor()
            .copied()
            .unwrap_or(clicked);

        let index_of = |value: u32| {
            items_in_order
                .iter()
                .position(|&id| id == value)
                .expect("anchor and clicked must be in items_in_order")
        };

        let a = index_of(anchor);
        let b = index_of(clicked);
        let (start, end) = if a <= b { (a, b) } else { (b, a) };

        let range = items_in_order[start..=end].iter().copied();
        selection.replace_with(range);
    }
}

let items = [10_u32, 20, 30, 40];
let mut sel = Selection::new();

// Click on 20.
handle_click(&mut sel, 20, Modifiers::default(), &items);
assert_eq!(sel.items(), &[20]);

// Shift-click on 40: select the range 20..=40.
handle_click(
    &mut sel,
    40,
    Modifiers { ctrl: false, shift: true },
    &items,
);
assert_eq!(sel.items(), &[20, 30, 40]);
```

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

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be licensed as above, without any additional terms or conditions.

[Rust Code of Conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: ../AUTHORS
[LICENSE-APACHE]: LICENSE-APACHE
[LICENSE-MIT]: LICENSE-MIT
