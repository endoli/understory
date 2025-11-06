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

- `understory_responder`
  - A deterministic event router that builds the responder chain sequence: capture → target → bubble.
  - Consumes pre‑resolved hits (from a picker or the box tree) and emits an ordered dispatch sequence.
  - Supports pointer capture with path reconstruction via a `ParentLookup` provider and bypasses scope filters.

Both crates are `#![no_std]` and use `alloc`.
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
