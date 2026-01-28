---
id: und-3bb1
status: open
deps: []
links: []
created: 2026-01-27T07:03:44Z
type: epic
priority: 1
assignee: Bruce Mitchener
tags: [dirty, dx, debug]
---
# understory_dirty: graph inspection + debug tooling

Developer experience tooling for inspecting DirtyGraph/DirtyTracker state (CLI + API helpers).

## Design Notes

- Keep core `no_std`; any “pretty printing” lives in a separate workspace crate or behind a debug-only feature.
- Prefer a small, structured “snapshot” API over ad-hoc `Debug` output:
  - Nodes: `Vec<K>` (or index-mapped), per-channel edges, per-channel dirty keys, cycle-handling mode.
  - Output adapters can render text tables and DOT without adding new production deps.
- Provide ergonomic filters in the tooling layer (channel, key, reachable-from key) rather than bloating core graph APIs.
- Plan for huge graphs: avoid O(V²) inspection routines by default; make expensive queries explicit.

## Acceptance Criteria

- Provide an API to snapshot graph + dirty state into a structured form
- Provide a small CLI/demo that can print: keys, edges, per-channel dirty keys
- Support filtering by channel and by key
- No new production deps in core crates
