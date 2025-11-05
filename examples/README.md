# Understory Examples

These examples form a short, progressive walkthrough from routing basics to integrating the box tree adapter.

- responder_basics
  - Rank hits by depth, reconstruct a path via parents, and emit the capture → target → bubble sequence.
  - Run: `cargo run -p understory_examples --example responder_basics`

- responder_hover
  - Derive hover enter/leave by comparing successive dispatch paths using the least common ancestor (LCA).
  - Run: `cargo run -p understory_examples --example responder_hover`

- responder_box_tree
  - Resolve hits from `understory_box_tree`, route them, and compute hover transitions. Includes a tiny ASCII tree and prints box rects and query coordinates.
  - Run: `cargo run -p understory_examples --example responder_box_tree`

Notes
- Examples live in a separate crate (`understory_examples`) so that published crates stay free of example-only dependencies.
- Output is formatted with section headers to make sequences easy to follow.
