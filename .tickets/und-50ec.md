---
id: und-50ec
status: open
deps: [und-3bb1]
links: []
created: 2026-01-27T07:03:50Z
type: feature
priority: 1
assignee: Bruce Mitchener
parent: und-3bb1
tags: [dirty, debug, graph]
---
# understory_dirty: cycle diagnostics (extract one cycle / SCC)

When a drain stalls or cycles are allowed, provide a way to diagnose cycles (e.g. return one cycle path or SCCs) for debugging/telemetry.

## Design Notes

- Scope: cycles are channel-specific (edges are per-channel), so diagnostics should take a `Channel`.
- Two useful outputs:
  - “One cycle” path: a `Vec<K>` showing a concrete loop for quick debugging.
  - SCC listing: `Vec<Vec<K>>` of strongly-connected components for more complete analysis.
- Implementation approach (suggested): Tarjan SCC (single pass, O(V+E), `no_std` + `alloc` friendly).
- Keep this off the hot path: do not compute SCCs during normal drain; only when explicitly called (or behind a debug feature).
- Integrate with existing behavior: when a drain stalls, the API can optionally operate on “remaining” keys only.

## Acceptance Criteria

- Add API to return one concrete cycle (`Vec<K>`) or an SCC set for a channel
- Works in `no_std` + `alloc`
- Covered by unit test with a known cycle
- Does not affect existing fast paths unless invoked
