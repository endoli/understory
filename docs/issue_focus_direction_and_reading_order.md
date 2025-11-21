# Understory Focus Direction & Reading Order Plan

This document narrows the design for two related pieces of the focus system:

- Directional traversal scoring (`Up`/`Down`/`Left`/`Right`).
- Reading order and LTR/RTL behavior for `Next`/`Prev`.

It builds on the higher-level plan in `docs/issue_focus_navigation_and_policies.md` and the
current implementation in `understory_focus`. For an overview of focus navigation, policies,
and the role of `DefaultPolicy` in the broader stack, start there; this document focuses on
the directional scoring and reading-order details.

The main goals are:

- Make the current behavior explicit and testable.
- Identify tunable points (weights, fallback, axes).
- Sketch how LTR/RTL should influence reading order and, when appropriate, directional semantics.

We **do not** intend to implement all of this immediately; this is a design target to guide gradual refinement.

## Current Behavior Summary

### Linear traversal (`Next` / `Prev`)

In `DefaultPolicy`:

- Candidate set:
  - Start from `FocusSpace.nodes`.
  - Filter to `enabled` entries.
- Ordering:
  1. If both entries have `order: Some(i32)`, compare by `order` (lower first).
  2. Otherwise fall back to **reading order** defined as:
     - Compare by `rect.y0` (ascending).
     - If `y0` is within a small epsilon, compare by `rect.x0` (ascending).
- Traversal:
  - `Next`: move forward in this sorted list.
  - `Prev`: move backward in this sorted list.
  - If the origin is not found in the list, `Next` picks the first; `Prev` picks the last.
  - `WrapMode` controls whether we wrap or stop at the edges.

### Directional traversal (arrows)

In `DefaultPolicy`:

- Candidate set:
  - Start from `FocusSpace.nodes`.
  - Filter to `enabled` entries.
  - Exclude the origin itself.
- Geometry:
  - Use `rect.center()` for both origin and candidates.
  - Compute `dx = cx - ox`, `dy = cy - oy`.
- Forward hemiplane filter:
  - `Right`: require `dx > 0`.
  - `Left`: require `dx < 0`.
  - `Down`: require `dy > 0`.
  - `Up`: require `dy < 0`.
- Scoring:
  - Map (`dx`, `dy`) into `(primary, secondary)` depending on direction:
    - Horizontal: `primary = dx`, `secondary = dy`.
    - Vertical: `primary = dy`, `secondary = dx`.
  - Score:
    - `score = |primary| + W * |secondary|`
    - Currently `W = 4.0`.
  - Pick the candidate with the smallest finite score.
- Fallback:
  - If no candidate survives the hemiplane filter:
    - `Up`/`Left` fall back to linear `Prev`.
    - `Down`/`Right` fall back to linear `Next`.
  - Fallback still uses `WrapMode` for wrapping/edge behavior.

### LTR/RTL

Today:

- Reading order is implicitly top-to-bottom, left-to-right.
- There is no explicit modeling of RTL; callers would have to invert coordinates or ordering externally.

## Directional Scoring: Design Targets

The current scoring is intentionally simple. We expect to refine it while keeping the same general shape:

- **Hemiplane filter stays**:
  - It matches intuition that "Right" should not move left of the origin, and vice versa.
  - It keeps the candidate set small for scoring.

- **Scoring function remains separable**:
  - We want `score = |primary| + W * |secondary|` or similar:
    - Primary axis: distance in the intended direction.
    - Secondary axis: lateral deviation penalty.
  - `W` should be a configurable constant in `DefaultPolicy` (and/or exposed via a constructor) rather than a hard-coded value.

- **No cone/angle logic for now**:
  - Cones (e.g., “within ±35° of the axis”) add complexity and require more nuanced tuning.
  - Sticking to hemiplane + weighted Manhattan distance keeps behavior predictable and easy to reason about.

### Tunables and future refinements

Possible extensions, if we find real use cases that demand them:

- **Different weights per direction**:
  - e.g., horizontal arrows might be more tolerant of vertical deviation than vertical arrows, or vice versa.
- **Softening the hemiplane when there are gaps**:
  - If no candidate survives the strict hemiplane filter, we could consider:
    - Allowing candidates with small negative `primary` (i.e., slightly behind the origin).
    - Using a “soft” cone before falling back to linear order.
- **Minimum “forward distance”**:
  - Ignore candidates that are extremely close to the origin along the primary axis to avoid jittering between overlapping entries.

These all fit into the existing structure; they’re refinements of scoring and filtering, not a new policy type.

## Reading Order and LTR/RTL

We want reading order and directional movement to agree with locale expectations where it matters, without complicating the core `FocusPolicy` trait.

### Linear reading order

Current:

- Linear reading order is (row, column) with row = `rect.y0`, column = `rect.x0` ascending.
- This matches LTR languages but not RTL.

Design direction:

- Introduce a simple reading-direction parameter at the policy or scope level:

  ```rust
  pub enum ReadingDirection {
      Ltr,
      Rtl,
  }
  ```

- For linear ordering:
  - Always sort by `y0` ascending first (top to bottom).
  - For columns:
    - LTR: `x0` ascending.
    - RTL: `x0` descending.

  This keeps the vertical “line” concept intact while flipping left/right expectations.

### Interaction with directional arrows

We don’t want directional behavior to become locale-dependent in a surprising way, but there are a couple of reasonable alignments:

- Horizontal arrows:

  - In LTR:
    - `Right` tends to feel like `Next` along a row.
    - `Left` tends to feel like `Prev`.
  - In RTL:
    - `Left` tends to feel like `Next`.
    - `Right` tends to feel like `Prev`.

  We **do not** plan to swap arrow meanings by locale; instead:

  - The hemiplane filter and scoring continue to operate in the geometric coordinate system.
  - The *fallback* mapping to linear order can consider reading direction:
    - For example, in RTL, `Right` might fall back to linear `Prev` if blocked, while `Left` falls back to `Next`, mirroring the role of arrows in LTR.

- Vertical arrows:

  - Vertical behavior (`Up`/`Down`) is mostly unaffected by LTR/RTL.
  - They continue to work in terms of rows (y-axis) with the same hemiplane and scoring logic.

### Where to store reading direction

We do **not** want to bake a `ReadingDirection` into `FocusSpace` itself. Instead:

- Reading direction is a property of:
  - The *scope* (e.g., a container, panel, or surface).
  - Or the `DefaultPolicy` configuration used for that scope.

Possible shapes:

- `DefaultPolicy::new(reading: ReadingDirection, wrap: WrapMode)`:
  - Keeps a `ReadingDirection` internally.
  - Uses it in `compare_linear` and in deciding the linear fallback for horizontal arrows.

This lets callers choose direction per scope without touching the trait signature.

## Plan of Record

Short-term (what we have now):

- Keep the current behavior:
  - Hemiplane + weighted Manhattan distance for arrows (with a fixed weight).
  - LTR-only reading order using `(y0, x0)` for linear traversal.
  - Locale-agnostic arrow semantics with simple linear fallback.
- Make sure tests cover:
  - Directional movement between three or more entries (already done in `focus_basics`).
  - Fallback behavior when the hemiplane is empty.

Next steps (non-breaking refinements):

- Introduce `ReadingDirection` and a `DefaultPolicy::new(..)` constructor with:
  - Internal fields for `reading_direction` and `wrap`.
  - Adjust `compare_linear` to flip the column ordering in RTL.
  - Optionally adjust horizontal directional *fallback* mapping to align better with reading order.
- Expose a score weight parameter on `DefaultPolicy` (e.g., via constructor or builder) to make directional tuning explicit.

Longer-term:

- Only consider more advanced features (cones, softened hemiplane, row/column snapping) once we have real-world layouts (e.g., masonry grids, multi-row menus) that demonstrate a need.
- If those needs are grid-specific and significantly different, address them in a dedicated policy (see `GridDirectionalPolicy` in `issue_focus_navigation_and_policies.md`) rather than complicating `DefaultPolicy`.

Non-goals:

- No locale-aware bidi text analysis in the focus layer; reading direction is a coarse per-scope setting.
- Do not swap arrow semantics (e.g., treat “Right” as “Up”) based on locale; arrows continue to be geometric operations with simple locale-aware fallbacks.
