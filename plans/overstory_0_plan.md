# overstory_0 Plan

## Fence

Overstory owns retained widget/runtime policy above Understory's headless kernels; it explicitly does not own the foundational box-tree, style/property, responder, or future display-list/presentation seams that belong in Understory.

## Overview

Goal:
- Start a real UI-kit layer above Understory that uses the existing kernels together instead of bypassing them.
- Pressure-test what belongs in Overstory versus what still wants new Understory seam crates such as `understory_display`.
- Produce a small but honest showcase that exercises style resolution, retained state, box-tree projection, event routing, and viewport glue.

Non-goals:
- Do not build a full widget vocabulary or a complete application shell.
- Do not invent a substitute for `understory_display` or the planned presentation-tree layer.
- Do not add text layout, accessibility bridges, or platform event loops to the core Overstory crate.
- Do not turn `understory_box_tree` into a layout engine or `understory_property` into a widget runtime.

## Chosen first slice

Create a small `overstory` crate with:
- a retained element tree with stable ids,
- built-in element kinds that are enough to show composition (`Root`, `Panel`, `Column`, `Button`, `Spacer`),
- `understory_property` stores on each element,
- `understory_style`-driven resolution over type/class/pseudoclass inputs,
- a simple layout pass that computes concrete rectangles,
- a derived `understory_box_tree` projection for hit testing and focusability,
- a `ui-events`-facing runtime that updates hover/press state and emits high-level interactions.

Keep the display-facing side intentionally thin:
- resolved visual data should be exposed as a retained snapshot of rectangles/colors/borders/text labels,
- this snapshot is for examples and pressure-testing only,
- it is not the long-term substitute for `understory_display`.

## Why this shape

- It uses existing crates directly, which makes boundary mistakes obvious quickly.
- It proves whether Overstory really needs its own runtime layer instead of just more examples.
- It keeps the missing display/presentation seam visible rather than burying it inside the toolkit.
- It gives a realistic path to a future display tree: style + layout + interaction already resolved, but still separate from paint recording.

## Planned steps

1. Scaffold `overstory` as a workspace crate with crate docs that explain the fence and the temporary visual snapshot boundary.
2. Add the retained element tree, layout primitives, style/property registry, and derived scene snapshot.
3. Add runtime state and `ui-events` integration for pointer hover/press/click over the projected box tree.
4. Add a showcase example in `understory_examples` that demonstrates:
   - style/theme resolution,
   - panel + button composition,
   - hover/press state changes from `ui-events`,
   - hit testing through the box tree,
   - optional viewport/panning glue where it adds pressure.
5. Tighten docs/tests and split the work into a small series of coherent commits.

## Risks

- The first slice may expose that we need `understory_display` sooner than expected.
- A too-rich temporary visual snapshot could accidentally become the wrong long-term abstraction.
- If layout grows beyond straightforward fixed/stack primitives, this branch should stop and extract a calmer contract before expanding further.
