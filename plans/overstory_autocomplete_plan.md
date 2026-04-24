# Overstory Autocomplete Plan

## Goal

Add a real type-ahead autocomplete path for `TextInput` that composes:
- anchored dropdown surfaces
- reusable list-based option presentation
- timer-driven debounce/open-state behavior
- keyboard and pointer handoff between input and popup

## Non-goals

- full command palette or global launcher semantics
- generalized menu system
- perfect virtualization in the first slice
- a finalized popup/layout/compositor story for every overlay type

## First slices

1. Add a reusable dropdown/list controller seam in Overstory terms.
   - anchored under one owner element
   - controller-owned open/closed state
   - reuse `overstory_list` for visible option rows

2. Add autocomplete state to `TextInput` integration without making
   `TextInput` itself own domain suggestion models.
   - query text
   - highlighted/selected option
   - debounce timer
   - accept/cancel behavior

3. Add one concrete example that proves:
   - typing updates suggestions
   - dropdown opens/closes correctly
   - keyboard navigation works
   - pointer selection works
   - scrollable option list works

4. Only then decide whether the option surface wants virtualization.
   The list controller is already separate; if row counts justify it, add
   a second-pass windowing model instead of baking that assumption into the
   first dropdown slice.

## Risks

- Popup ownership can easily collapse back into demo glue if the anchor/open
  state lives outside reusable controllers.
- `TextInput` can become overcoupled to suggestion models if it owns too much
  autocomplete policy directly.
- Keyboard routing must remain explicit: input editing keys, dropdown list
  navigation keys, accept/cancel keys, and focus transitions need one clear
  policy.
- If virtualization is forced too early, the popup/list contract will become
  harder to reason about before the simpler dropdown path is stable.
