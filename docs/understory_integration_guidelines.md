# Integrating Understory crates without losing your identity

As more crates in the Understory ecosystem stabilize and external toolkits adopt them, it's
important that hosts can reuse the building blocks without having their public API surface
turn into "Understory with some extra types" *or* over-wrap everything and lose the benefits
of shared types.

This document sketches guidelines for how to integrate Understory crates while keeping a clear,
framework-centric identity, while still allowing some Understory types to act as a lingua franca
for tooling and cross-toolkit interop.

## 1. Keep your own public types (where it buys you something)

There are two broad buckets of types:

- **Infrastructure types** (internals you might want to swap or fork):
  index backends, box-tree internals, helper data structures.
- **Protocol / platform types** (shared concepts that benefit from interop):
  hit results, responder events, accessibility roles, recording formats, etc.

For infrastructure types, prefer host-side wrappers:

- Prefer host-side newtypes over re-exporting core ids and structs at your root when you want
  the freedom to change implementation later:
  - `pub struct MyNodeId(understory_box_tree::NodeId);`
  - `pub struct MyHit(understory_responder::Hit<MyNodeId>);`
- Expose your own `Result`/`Error` types and translate from Understory errors internally.
- Avoid `pub use understory_*::*;` at the top level of your crate; re-export only the smallest set
  of types that you intentionally want in your public surface.

For protocol / platform types that are meant to be shared (for example, stable responder event
types or recording formats), it can be *good* to expose the Understory types directly so that tools
and libraries can interoperate across frameworks.

Being explicit about which bucket a type is in will help avoid both accidental leaking and
unnecessary wrapping.

## 2. Use adapter traits and bridges

- Integrate through traits and adapters instead of baking your widget/scene types into core crates:
  - Implement responder traits (e.g. widget lookup) in your host crate.
  - Keep mapping from your widget/tree ids to `NodeId` inside an adapter module.
- Prefer small bridge crates like `myui-understory-box-tree` over pushing host-specific logic down
  into core Understory crates.

This keeps core crates generic and lets multiple toolkits coexist without leaking host concepts.

## 3. Treat Understory as infrastructure, not your brand

- In your public docs and examples, talk in terms of your framework's concepts:
  - "scene graph", "widget tree", "layout tree", etc.
  - Mention Understory crates as implementation details or optional extension points.
- In logging and error messages, use neutral phrasing:
  - "box tree: dangling node id" instead of "Understory: dangling node id" unless the distinction
    is important for debugging.

This helps your users build a mental model around *your* toolkit, with Understory as an internal
building block rather than the primary brand.

## 4. Respect layering and responsibilities

Use Understory crates where they fit, but keep higher-level semantics in your own code:

- `understory_box_tree`:
  - Use for geometry, transforms, clips, hit testing, and visibility queries.
  - Do **not** push layout policies, paint ordering, or stacking-context rules into the box tree.
  - Keep your own layout tree and render/paint model on top.
- `understory_index`:
  - Use as a spatial acceleration structure; keep it agnostic of widgets and painting.

Having a clear layering story makes it easier for multiple frameworks to share the same crates
without stepping on each other's abstractions.

## 5. Be deliberate about re-exports

Sometimes it does make sense to re-export an Understory type; treat this as a conscious API
decision:

- For **protocol / platform types** that you expect tools and other libraries to consume directly
  (e.g., responder events, recording formats), it is fine – and often desirable – to expose the
  Understory type in your public API so it can serve as a lingua franca.
- For **infrastructure types**, prefer:
  - Re-exporting from an internal module, not from your crate root, so you can move it later:
    - `pub use understory_box_tree::NodeFlags;` inside `myui::internals::spatial`.
  - Calling out any direct re-exports in your own API docs so downstream users know which parts of
    your surface are tied to Understory semver.

This keeps the integration explicit and easier to evolve over time, without giving up on shared
types where they provide real interop value.

