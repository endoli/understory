# Overstory

Retained UI/runtime layer built on top of Understory kernels.

Overstory owns toolkit-facing retained state and runtime policy. It uses
Understory crates for the headless kernels:

- `understory_property` for dependency-style property storage,
- `understory_style` for selector-based style and theme resolution,
- `understory_box_tree` for spatial indexing and hit testing,
- `understory_responder` for deterministic routing helpers,
- `ui-events` for transport-agnostic input event types,
- `peniko` for the shared color vocabulary exposed by the public API,
- `understory_display` for the retained display-tree seam above paint backends.

This crate intentionally does **not** own a renderer-facing display list or
presentation system. Instead it resolves retained UI/runtime state into:

- a `SceneSnapshot` for debug/projection and hit testing, and
- a retained `understory_display::DisplayTree` that embedders can lay out and
  lower into paint backends.

## First slice

The initial crate is deliberately small:

- append-only retained element tree with stable `ElementId`s,
- a built-in element vocabulary (`Root`, `Panel`, `Row`, `Column`, `Button`, `Spacer`),
- built-in layout/visual dependency properties,
- a full rebuild path that resolves style, lays out elements, projects them
  into an `understory_box_tree::Tree`, and can build a retained
  `understory_display::DisplayTree`,
- a `ui-events` pointer runtime that updates hover/press state and emits
  high-level interactions.

## Non-goals

This crate does not yet own:

- text shaping or glyph recording,
- accessibility bridges,
- platform event loops,
- a renderer-facing display list,
- a general widget authoring API.

## Example

See `examples/overstory_showcase.rs` in the workspace examples crate.
For a windowed demo, see `examples/overstory_visual_demo.rs`, which asks
`SceneSnapshot` for a retained `understory_display::DisplayTree`, lays it out,
lowers it into a retained `understory_display::DisplayList`, and then lowers
that into `imaging`.
