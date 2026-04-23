# Overstory Transcript Plan

## Goal

Turn the current chat/transcript composition in `overstory_visual_demo` into a
real reusable surface instead of leaving transcript row construction and update
policy inside one example.

## Non-goals

- Full chat application framework.
- Rich text or markdown rendering.
- Virtualized transcript rendering.
- Tool-specific custom widgets for every transcript entry kind.

## Shape

Add a small `overstory_transcript` integration crate that depends on:

- `overstory`
- `understory_transcript`

It will own transcript-specific UI composition and sync logic while keeping
`overstory` itself generic.

## First slice

1. Add `overstory_transcript` to the workspace.
2. Add a `TranscriptViewController` that:
   - binds to one `ScrollView` element,
   - appends rows for transcript entries,
   - keeps a per-entry row/text/spinner mapping,
   - projects in-progress assistant entries with a spinner,
   - updates existing rows as transcript entries mutate.
3. Add a small style/config struct for row spacing and basic appearance.
4. Move the visual demo off its local transcript row plumbing onto the new
   controller.

## Risks

- If the controller owns too much app-specific policy, it will just relocate
  demo glue instead of removing it.
- If it tries to model every transcript entry kind up front, the API will bloat
  before real usage justifies it.

## Guardrails

- Keep the controller append-oriented and honest.
- Support message entries first, plus a calm fallback text projection for other
  entry kinds.
- Keep styling override points explicit and small.
