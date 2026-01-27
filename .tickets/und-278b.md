---
id: und-278b
status: open
deps: []
links: []
created: 2026-01-27T07:04:05Z
type: feature
priority: 3
assignee: Bruce Mitchener
tags: [dirty, perf, scheduling]
---
# understory_dirty: work-budgeted / resumable draining

Allow draining in chunks (work budget per frame) while preserving topo constraints and cycle reporting.

## Acceptance Criteria

- Provide API to drain up to N keys and resume
- Clearly define interaction with `DirtySet` clearing and new marks
- Unit test for resume behavior
