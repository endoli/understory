# Understory

Foundational spatial and scene data structures for user interfaces, graphics editors, and CAD viewers.

Understory is a small family of crates designed to be combined in different stacks over time.
The focus is on clean separation of concerns, pluggable performance trade‑offs, and long‑term architectural stability.

## Crates

- `understory_index`
  - A generic 2D AABB index with pluggable backends: FlatVec (linear scan), R‑tree, and BVH.
  - Works across `f32`/`f64`/`i64` coordinate spaces with widened accumulator metrics for robust splits.
  - Point and rectangle queries.
  - Batched updates via `commit()` with coarse damage (added, removed, moved).

- `understory_box_tree`
  - A Kurbo‑native, spatially indexed box tree for scene geometry: local bounds, transforms, optional clips, and z‑order.
  - Computes world‑space AABBs and synchronizes them into `understory_index` for fast hit‑testing and visibility.
  - Not a layout engine.
  - Upstream code (your layout system) decides sizes and positions and then updates this tree.

- `understory_focus`
  - Focus navigation primitives: navigation intents, per‑node focus properties, and a spatial view of focusable candidates.
  - Provides pluggable policies for directional and ordered navigation, and an optional adapter for integrating with `understory_box_tree`.
  - Designed to be independent of any particular widget toolkit or event system.

- `understory_precise_hit`
  - Geometry‑level, narrow‑phase hit testing for shapes in local 2D coordinates, built on `kurbo`.
  - Provides a small `PreciseHitTest` trait with `HitParams`/`HitScore` helpers and default impls for `Rect`, `Circle`, `RoundedRect`, and fill‑only `BezPath`.
  - Designed to be paired with a broad‑phase index (e.g. `understory_index` + `understory_box_tree`) and event routing (`understory_responder`), with rich metadata carried in responder `meta` types.

- `understory_responder`
  - A deterministic event router that builds the responder chain sequence: capture → target → bubble.
  - Consumes pre‑resolved hits (from a picker or the box tree) and emits an ordered dispatch sequence.
  - Includes a tiny dispatcher helper (`dispatcher::run`) for executing handlers and honoring stop/cancelation.
  - Supports pointer capture with path reconstruction via a `ParentLookup` provider and bypasses scope filters.

- `understory_selection`
  - Generic selection container that tracks a set of keys plus an optional primary and anchor, plus a revision counter for change detection.
  - Generic over the key type `T` (no `Hash`/`Ord` requirement; only `PartialEq`), suitable for list selections, canvases, and other selection UIs.
  - Intended to pair with `understory_box_tree` / `understory_precise_hit` for hit testing and `understory_responder` for event routing.

- `understory_view2d`
  - 2D and 1D view/viewport primitives:
    - `Viewport2D` for canvas/CAD‑style views: camera state (pan + zoom), coordinate conversion between world and device space, view fitting (center vs align‑min), and simple clamping against optional world bounds.
    - `Viewport1D` for timeline/axis‑style views: X‑only zoom/pan with range fitting and clamping, plus helpers for suggesting grid spacing.
  - Headless and renderer‑agnostic; intended to be driven by `ui-events` at higher layers and paired with `understory_box_tree` / imaging crates for canvas and timeline applications.

All core crates are `#![no_std]` and use `alloc`.
Examples and tests use `std`.

## Why this separation?

We aim for a three‑tree model that scales and composes well.

1) Widget tree — state and interaction
2) Box tree — geometry and spatial indexing
3) Render tree — display list (future crate)

This split makes debugging easier, enables incremental updates, and lets each layer evolve and be swapped independently.
For example, a canvas or DWG or DXF viewer can reuse the box and index layers without any UI toolkit.

## Design principles

- Pluggable backends and scalars.
- Choose trade‑offs per product or view.
- Predictable updates.
- Batch with `commit()` and use coarse damage for bounding paint.
- Conservative geometry.
- Use world AABBs for transforms and rounded clips and apply precise filtering where cheap.
- No surprises.
- `no_std` + `alloc`, minimal dependencies, and partial concise `Debug` by default.

## Performance notes

- Arena‑backed R‑tree and BVH reduce allocations and pointer chasing.
- STR and SAH‑like builds are available.
- Benchmarks live under `benches/` and compare backends across distributions and sizes.
- Choose R‑tree or BVH for general scenes.
- Choose FlatVec for tiny sets.

## Roadmap (sketch)

- Render tree crate and composition and layering utilities.
- Backend tuning (SAH weights, fanout/leaf sizes), bulk builders, hygiene/rotation, and churn optimizations.
- Extended benches such as update mixes, overlap stress, and external comparisons.
- Integration examples with upstream toolkits.

## Getting started

- Read the crate READMEs.
  - `understory_index/README.md` has the API and a “Choosing a backend” guide.
  - `understory_box_tree/README.md` has usage, hit‑testing, and visible‑set examples.
  - `understory_responder/README.md` explains routing, capture, and how to integrate with a picker.
  - `understory_focus/README.md` covers focus navigation policies and adapters.
  - `understory_selection/README.md` documents the selection container, anchor/revision semantics, and click helpers.
  - `understory_view2d/README.md` documents the 2D and 1D viewport types, clamping/fit modes, and examples of using visible regions for culling.
- Run examples.
  - `cargo run -p understory_examples --example index_basics`
  - `cargo run -p understory_examples --example box_tree_basics`
  - `cargo run -p understory_examples --example box_tree_visible_list`
  - `cargo run -p understory_examples --example responder_basics`
  - `cargo run -p understory_examples --example responder_hover`
  - `cargo run -p understory_examples --example responder_box_tree`

## MSRV & License

- Minimum supported Rust: 1.88.
- Dual‑licensed under Apache‑2.0 and MIT.
