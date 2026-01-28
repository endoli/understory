---
id: und-789b
status: open
deps: []
links: []
created: 2026-01-27T07:04:01Z
type: feature
priority: 2
assignee: Bruce Mitchener
tags: [dirty, perf, api]
---
# understory_dirty: batch marking API (coalesce generation + fewer lookups)

Add mark_many / mark_set style APIs to reduce overhead for many marks per frame and coalesce generation bumps.

## Acceptance Criteria

- Provide `mark_many(keys, channel)` and/or `mark_many_with(policy)`
- Generation increments once per batch
- Unit test covers semantics vs repeated mark
- No new deps
