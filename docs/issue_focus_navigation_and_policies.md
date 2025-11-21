# Understory Focus Navigation & Policies Plan

This document records the current design for focus navigation in Understory and how we intend to evolve focus policies over time. It ties together:

- `understory_box_tree` for geometry and focusable flags.
- `understory_focus` for focus spaces and navigation policies.
- `understory_responder` for focus paths and enter/leave events.

The goal is predictable, testable focus behavior without pushing high‑level policy into the geometry layer.

## Current State

### Geometry and focusability

- `understory_box_tree`:
  - Nodes carry `NodeFlags`, including `FOCUSABLE`.
  - `QueryFilter` supports `.visible().pickable().focusable()`.
  - `Tree` exposes:
    - `world_bounds(NodeId)` for world‑space AABBs.
    - Structural helpers: `parent_of`, `children_of`, `next_depth_first`, `prev_depth_first`.
  - The box tree stays focused on geometry and spatial indexing; it does not know about focus order or keyboard policies.

### Focus state and routing

- `understory_responder`:
  - `FocusState<K>` tracks the current root→target focus path and emits `FocusEvent::{Enter,Leave}` transitions when the path changes.
  - `Router::dispatch_for` routes a keyboard/IME event along the focus path (capture → target → bubble).
  - The responder is agnostic about *how* the next focus target is chosen; it expects a node id and a path.

### Focus navigation crate

- `understory_focus`:
  - Models focus navigation as:
    - `Navigation` intents (Next, Prev, Up, Down, Left, Right, EnterScope, ExitScope).
    - `FocusProps` per node (enabled, order, group, autofocus, policy_hint).
    - `FocusEntry<K>` and `FocusSpace<'a, K>`: a snapshot of focusable candidates with geometry.
    - `FocusPolicy<K>`: trait for policies that pick the next node given an origin, direction, and `FocusSpace`.
    - `DefaultPolicy` as the main, shipping policy implementation.
  - `adapters::box_tree` (behind `box_tree_adapter` feature) converts a `Tree` subtree into a `FocusSpace<NodeId>` by:
    - Traversing a scope subtree.
    - Filtering to visible + focusable + enabled nodes.
    - Using `world_bounds` for `FocusEntry::rect`.

## Policy Abstraction

### FocusPolicy and FocusSpace

- `FocusPolicy<K>`:
  - `fn next(&self, origin: K, direction: Navigation, space: &FocusSpace<'_, K>) -> Option<K>;`
  - Generic over `K` so callers can use `understory_box_tree::NodeId` or any other id type.
- `FocusSpace<'a, K>`:
  - Holds a slice of `FocusEntry<K>`.
  - All entries share the same coordinate space (e.g., box‑tree world space or scope‑local space).
- `FocusEntry<K>`:
  - `id: K`
  - `rect: kurbo::Rect` in the focus space’s coordinate system.
  - `order: Option<i32>` (explicit order override).
  - `group: Option<FocusSymbol>` (for clustering).
  - `enabled: bool`
  - `scope_depth: u8` (relative depth inside the current scope).

### FocusProps and FocusSymbol

- `FocusProps` is the host‑side description of a node’s focus intent:
  - `enabled`: whether the node participates in focus traversal.
  - `order`: optional explicit ordering key for reading‑order policies.
  - `group`: optional `FocusSymbol` for partitioning.
  - `autofocus`: whether the node is a candidate for initial focus in a scope.
  - `policy_hint`: optional `FocusSymbol` to steer policy choice per scope.
- `FocusSymbol(u64)` is intentionally minimal:
  - Hosts decide how to create and manage symbols (interned strings, enums, static constants).
  - Typical uses:
    - `group`: keep navigation within a logical cluster (grid, toolbar, inspector section).
    - `policy_hint`: mark containers for a particular traversal style (reading‑order vs. grid‑like).

## Default Policy Behavior

`DefaultPolicy` is intended to be the main “just do the obvious thing” implementation. It composes a couple of behaviors:

- Linear traversal (`Navigation::Next` / `Prev`):
  - Builds a list of enabled entries from the `FocusSpace`.
  - Sorts by:
    1. `order` when present (lower first).
    2. Otherwise, reading order: `(y0, x0)` in the focus space’s coordinate system.
  - `Next` moves forward; `Prev` moves backward.
  - `WrapMode` controls whether traversal wraps within the scope or stops at edges.

- Directional traversal (`Up`, `Down`, `Left`, `Right`):
  - Finds the origin entry (must be enabled).
  - Uses centers of `rect` to compute deltas.
  - Restricts to the forward hemiplane for the requested direction.
  - Scores candidates using a simple “distance + heading penalty” heuristic:
    - `score = |primary| + 4 * |secondary|`
    - Prefers closer, more aligned candidates along the chosen axis.
  - If no candidate survives the hemiplane filter, falls back to the linear behavior:
    - Arrows map to linear `Prev`/`Next` with wrap honoring `WrapMode`.

- Scope enter/exit (`EnterScope`, `ExitScope`):
  - `DefaultPolicy` does not change focus on these; they are treated as higher‑level intents that a scope controller should interpret.

## Should We Have More Than One Policy Impl?

Short answer: yes, but we want to keep the set small and intentional.

The trait abstraction allows many policies, but we want:

- A single, ergonomic default (`DefaultPolicy`) that covers common cases.
- A small number of additional concrete policies only when we have clear, distinct needs that are hard to express as configuration on `DefaultPolicy`.
- Policy *selection* handled by a higher‑level “focus manager” rather than exploding the type surface.

### DefaultPolicy (shipping today)

- Recommended default for most scopes.
- Composite behavior:
  - Reading‑order + `order` for `Next`/`Prev`.
  - Directional scoring + hemiplane filtering for arrows, with linear fallback.
  - Wrap behavior via `WrapMode`.
- Does **not** currently interpret `group` or `policy_hint` directly; these are left for higher‑level orchestration (see plan below).

### Candidate future policies

We foresee two likely additions if/when real use‑cases demand them:

1. **ReadingOrderPolicy**
   - Purpose: minimal, predictable focus order where geometry is secondary.
   - Behavior:
     - `Next`/`Prev` purely by `(order, y, x)`; ignores directional scoring entirely.
     - Arrows either alias to `Next`/`Prev` or are left to higher layers.
   - Use cases: forms, dialogs, and other “reading order is king” screens; tests that want a very simple model.

2. **GridDirectionalPolicy**
   - Purpose: tuned for grid/masonry layouts where row/column adjacency matters more than raw distance.
   - Behavior sketch:
     - Favors candidates sharing row/column with the origin when moving along corresponding axes.
     - Uses `group` (e.g., a `GRID` symbol) to avoid jumping out of the grid until the user explicitly exits the scope.
     - May ignore `order` entirely in favor of geometric adjacency.
   - Use cases: dense grids, masonry layouts, or TV‑style focus where arrows need to feel “snappy” and localized.

We will only add these once we have concrete usage (e.g., a masonry grid demo or a complex form) that shows `DefaultPolicy` is not sufficient or becomes too complex to tune via configuration.

## Policy Selection and Focus Manager

We do **not** plan to bake policy selection logic into `understory_focus` itself. Instead:

- A host‑side “focus manager” or scope controller is responsible for:
  - Building `FocusSpace` instances (e.g., via the box‑tree adapter).
  - Choosing a `FocusPolicy` implementation per scope.
  - Feeding navigation intents into the chosen policy.
  - Converting the resulting node id into a root→target path and updating `FocusState`.
  - Using `Router::dispatch_for` to route key events along the updated focus path.

Policy selection can use:

- `FocusProps::policy_hint`:
  - Example: if the container’s props include `HINT_GRID_POLICY`, use `GridDirectionalPolicy` for that scope; otherwise `DefaultPolicy`.
- `FocusProps::group`:
  - Example: only consider candidates with the same `group` as the current focus when using a grid policy.

This keeps `understory_focus` small and composable, while allowing toolkit/framework code to encode richer semantics.

## Plan and Next Steps

### Phase 1 (done)

- Introduce `understory_focus`:
  - `Navigation`, `FocusProps`, `FocusSymbol`, `FocusEntry`, `FocusSpace`, `WrapMode`, `FocusPolicy`.
  - `DefaultPolicy` with linear + directional behavior and wrap.
  - `adapters::box_tree::build_focus_space_for_scope`.
- Add unit tests for `DefaultPolicy` (linear order, disabled skipping, wrap/no‑wrap, directional behavior).
- Add adapter tests that exercise `Tree` → `FocusSpace` → `DefaultPolicy` end‑to‑end.

### Phase 2 (short‑term)

- Integrate `understory_focus` into the examples:
  - Extend the `responder_ui_events_bridge` example (or a new one) to:
    - Maintain `FocusProps` for `NodeId`s.
    - Build a `FocusSpace` for the active scope from the box tree.
    - Use `DefaultPolicy::next` to select next focus on Tab/arrow keys.
    - Update `FocusState` and route via `Router::dispatch_for`.
  - Add golden tests that assert focus traversal sequences over small layouts (including a simple grid).

### Phase 3 (medium‑term)

- Evaluate the need for additional policies based on real usage:
  - If we see patterns where “reading order only” is needed, add `ReadingOrderPolicy`.
  - Once we have a masonry/grid demo, consider `GridDirectionalPolicy` with group‑aware behavior.
- Keep the underlying `FocusPolicy` trait stable; policy types can be added without breaking callers.

### Non‑Goals

- The box tree will not gain focus‑specific policies or logic beyond `NodeFlags::FOCUSABLE`.
- `understory_focus` will not embed toolkit‑specific notions of widgets, roles, or a11y attributes; those integrate via AccessKit and higher layers.
- We will not introduce a large family of policies; the intention is a small set of well‑defined building blocks plus host‑side orchestration.

## Implementation Notes

For now, `understory_focus` keeps its core types and `DefaultPolicy` implementation together in
`src/lib.rs`. This keeps the crate easy to scan while the API and policy set are still small.

If and when we add additional concrete policies (`ReadingOrderPolicy`, `GridDirectionalPolicy`)
or more substantial helper code, we expect to split the implementation into submodules along
roughly these lines:

- `lib.rs`: crate docs, `Navigation`, `FocusSymbol`, `FocusProps`, `FocusEntry`, `FocusSpace`,
  `WrapMode`, `FocusPolicy`, and re-exports.
- `policy/default.rs`: `DefaultPolicy` and its helpers/tests.
- `policy/reading_order.rs`, `policy/grid.rs`: future policy implementations, only if needed.

This is a non-goal for the first iteration; the intent is to refactor only once we have real
usage that justifies the extra structure.

### Candidate Sets and Performance

`DefaultPolicy` currently operates over a `FocusSpace<'a, K>` that exposes `&[FocusEntry<K>]`.
Callers are expected to:

- Build this space per **scope** (window/panel/toolbar), not for the entire application.
- Use a reusable buffer (e.g., `Vec<FocusEntry<_>>` cleared and refilled as needed).
- Prune aggressively at the adapter level (for example, only visible + focusable + enabled nodes,
  and typically only nodes within the active viewport).

This push-style API is intentionally simple and adequate for most UIs. If we encounter real
workloads where even per-scope `FocusSpace` materialization becomes too expensive, we can add
an additional, more pull-friendly abstraction without breaking existing code, for example:

- A `FocusSource<K>` trait that yields `FocusEntry<K>`s on demand (with `FocusSpace` as one
  implementation).
- Or a “IDs + lookup” model where the policy iterates `&[K]` and pulls geometry/props via a
  `FocusLookup<K>` trait.

We are **not** committing to either shape yet; the plan is to keep the current `FocusSpace`
API, measure real usage, and only introduce a pull-based source abstraction if and when it
is clearly justified.
