# understory_transcript plan

## Goal

Add a small `understory_transcript` crate for append-oriented interaction logs
with stable entry ids, explicit status, typed entries, and projection-friendly
links for chat, shell, and agent-style systems.

## Non-goals

- text layout or attributed text
- widgets or rendering
- persistence backends
- transport/protocol integration
- terminal emulation

## First slice

1. Add `understory_transcript` as a `no_std` + `alloc` core crate in the workspace.
2. Implement the core model:
   - `EntryId`
   - `Timestamp`
   - `EntryBody`
   - `EntryStatus`
   - `EntryKind`
   - `TranscriptEntry`
   - `NewEntry`
   - `Transcript`
3. Support practical append/update flows:
   - append new entries
   - append text/bytes chunks to an existing entry
   - set status
   - query by id and in append order
   - query children by parent
4. Add rustdoc and a runnable example in `understory_examples`.
5. Keep the surface calm and leave richer projections for later.

## Risks

- Overfitting to “chat messages” instead of a general interaction log.
- Making the content model too rich too early.
- Letting streaming/update semantics become clever or implicit.

## Design fence

This crate owns append-oriented interaction records and their structural links;
it explicitly does not own layout, rendering, protocol semantics, or text styling.
