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

- responder_precise_hit
  - Combine `understory_box_tree` (broad phase) with `understory_precise_hit` (precise geometry hits) and route the result through the responder.
  - Run: `cargo run -p understory_examples --example responder_precise_hit`

- responder_focus
  - Dispatch to focused target via `dispatch_for` and compute focus transitions with `FocusState`.
  - Run: `cargo run -p understory_examples --example responder_focus`

- index_basics
  - Insert, update, commit damage, and query using `understory_index`.
  - Run: `cargo run -p understory_examples --example index_basics`

- box_tree_basics
  - Build a small scene, commit, move a node, compute damage, and hit-test using `understory_box_tree`.
  - Run: `cargo run -p understory_examples --example box_tree_basics`

- box_tree_visible_list
  - Use `intersect_rect` to compute a simple visible window (like a virtualized list) using `understory_box_tree`.
  - Run: `cargo run -p understory_examples --example box_tree_visible_list`

- outline_property_grid
  - Build a grouped property-grid-style outline, expand/collapse groups, and inspect the visible row projection.
  - Run: `cargo run -p understory_examples --example outline_property_grid`

- outline_virtual_list
  - Compose `understory_outline` with `understory_virtual_list` by virtualizing the visible rows of an expanded outline.
  - Run: `cargo run -p understory_examples --example outline_virtual_list`

- outline_inspector
  - Drive `understory_inspector` over a property-grid-style domain model, then inspect expansion sync, visible-row focus, range selection, and collapse pruning.
  - Run: `cargo run -p understory_examples --example outline_inspector`

- transcript_agent_run
  - Record a small agent-style run with user input, tool lifecycle, streamed stdout, and annotation entries using `understory_transcript`.
  - Run: `cargo run -p understory_examples --example transcript_agent_run`

- transcript_virtual_list
  - Virtualize a variable-height transcript with `PrefixSumExtentModel`, then scroll to interesting entries with `VirtualList`.
  - Run: `cargo run -p understory_examples --example transcript_virtual_list`

- transcript_tail_anchored
  - Keep a chat/log-style transcript pinned to the tail with `TailAnchoredExtentModel`, but only while the user is already anchored there.
  - Run: `cargo run -p understory_examples --example transcript_tail_anchored`

Notes
- Examples live in a separate crate (`understory_examples`) so that published crates stay free of example-only dependencies.
- Output is formatted with section headers to make sequences easy to follow.
