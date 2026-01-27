---
id: und-eac0
status: open
deps: [und-3bb1]
links: []
created: 2026-01-27T07:03:56Z
type: feature
priority: 2
assignee: Bruce Mitchener
parent: und-3bb1
tags: [dirty, debug, determinism]
---
# understory_dirty: deterministic topological drain (opt-in)

Optional deterministic ordering when multiple keys are ready, for reproducible debugging/profiling.

## Design Notes

- Today, order among simultaneously-ready keys depends on hash iteration order; this makes perf debugging and tests noisy.
- Define determinism precisely: “for equal readiness, order by `K`”.
  - This likely requires `K: Ord` (or a user-provided key function) for the deterministic variant.
- Candidate implementation: replace the “ready queue” with a sorted structure:
  - Collect ready keys into a `Vec<K>`, `sort_unstable()`, then pop in order; when new keys become ready, insert and keep the vec sorted (or use a binary heap with reversed ordering).
- Keep the default fast path unchanged; deterministic drain should be an explicit API (and documented as slower).
- Add a benchmark to quantify overhead and validate that determinism doesn’t accidentally become the default.

## Acceptance Criteria

- Add an opt-in deterministic drain variant (e.g. stable by key)
- Document nondeterminism vs deterministic tradeoff
- Bench to quantify overhead
