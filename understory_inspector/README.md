# Understory Inspector

<!-- cargo-rdme start -->

Understory Inspector: host-side controller for hierarchical inspection UIs.

This crate sits above [`understory_outline`] and below any actual widget or
rendering system. It owns the small but repeated host loop around:
- visible-row projection via [`understory_outline::Outline`],
- fixed-row virtualization via [`understory_virtual_list::VirtualList`],
- current focus over visible rows,
- and range-style selection via [`understory_selection::Selection`].

It does **not** own:
- rendering,
- icons, badges, or columns,
- property editing widgets,
- or domain-specific adapters such as box-tree or style inspectors.

## First read

An inspector is a controller for “expandable rows with focus and selection.”
You provide:
- a domain model implementing [`InspectorModel`],
- a fixed-row [`InspectorConfig`],
- and host rendering that turns visible keys into actual UI rows.

The controller keeps these concerns coherent:
- expanding/collapsing updates the visible row projection,
- the virtual list length stays in sync with the current visible row count,
- focus moves by visible-row order,
- hidden focused rows fall back to a parent row,
- and hidden selected rows are pruned after collapse or model change.

## Minimal example

```rust
use understory_inspector::{Inspector, InspectorConfig, InspectorModel};
use understory_outline::OutlineModel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Group,
    Alpha,
    Beta,
}

struct DemoModel;

impl OutlineModel for DemoModel {
    type Key = Key;
    type Item = &'static str;

    fn first_root_key(&self) -> Option<Self::Key> {
        Some(Key::Group)
    }

    fn contains_key(&self, _key: &Self::Key) -> bool {
        true
    }

    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match key {
            Key::Group => None,
            Key::Alpha => Some(Key::Beta),
            Key::Beta => None,
        }
    }

    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match key {
            Key::Group => Some(Key::Alpha),
            Key::Alpha | Key::Beta => None,
        }
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        Some(match key {
            Key::Group => "Group",
            Key::Alpha => "Alpha",
            Key::Beta => "Beta",
        })
    }
}

impl InspectorModel for DemoModel {
    fn parent_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match key {
            Key::Alpha | Key::Beta => Some(Key::Group),
            Key::Group => None,
        }
    }
}

let mut inspector = Inspector::new(
    DemoModel,
    InspectorConfig::fixed_rows(18.0, 36.0),
);

assert!(inspector.focus_first());
assert!(inspector.select_only_focused());
assert!(inspector.expand_focused());
assert!(inspector.focus_next());
assert_eq!(inspector.focus(), Some(&Key::Alpha));
assert_eq!(inspector.selection().items(), &[Key::Group]);
```

## Second read

This v0 crate is intentionally narrow:
- fixed row extent only,
- linear visible-row focus only,
- no presentation model,
- no domain adapters yet.

That is deliberate. The goal is to dogfood the composition pattern from the
outline inspector example without prematurely inventing a widget layer.

## Glossary

- **Inspector model**: your domain model plus parent lookup.
- **Visible row**: one flattened row in the current expansion state.
- **Focus**: the single row targeted by keyboard-like movement.
- **Anchor**: the selection pivot used for visible-order range extension.
- **Realized range**: the currently virtualized row indices that should be rendered.

## Usage pattern

A typical host loop looks like:
1. mutate your domain model,
2. call [`Inspector::sync`],
3. render [`Inspector::visible_rows`] over [`Inspector::realized_range`],
4. map user input into controller methods such as [`Inspector::focus_next`],
   [`Inspector::toggle_focused`], or [`Inspector::extend_selection_next`].

This crate is `no_std` and uses `alloc`.

<!-- cargo-rdme end -->
