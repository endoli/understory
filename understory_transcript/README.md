# Understory Transcript

Append-order interaction log primitives with generic payloads and explicit
updates for chat, shell, and agent-style systems.

`understory_transcript` provides a small core for recording ordered interaction
entries with stable ids, typed payloads, explicit parent/cause links, and
explicit streaming/status updates.

It is intended to sit below chat views, shell panes, and agent harness UIs.

## Scope

This crate owns:

- append-order transcript storage
- stable entry ids
- typed entry kinds
- explicit parent/cause relationships
- generic payloads, with `EntryBody` as the built-in text/bytes/empty default
- explicit status and chunk updates to existing entries

This is append-oriented, not a strict append-only event log. Entry order is
stable after append, but hosts may explicitly update an entry's body or status
as streaming output arrives.

`Transcript<P = EntryBody>` is generic over payload type, so richer hosts can
store fragment ids, structured objects, or other presentation-layer handles
without pulling layout or widget policy into the crate.

This crate does not own:

- text layout or styling
- rendering
- persistence backends
- transport/protocol semantics
- terminal emulation

## Minimal example

```rust
use understory_transcript::{
    EntryBody, EntryStatus, MessageRole, NewEntry, ProcessStream, ToolOutcome, Transcript,
};

let mut transcript = Transcript::new();

let user = transcript.append(NewEntry::message(MessageRole::User, "run tests"));
let tool = transcript.append(
    NewEntry::tool_call("cargo test", EntryBody::Empty)
        .with_cause(user)
        .with_status(EntryStatus::InProgress),
);
let stdout = transcript.append(
    NewEntry::process_output(ProcessStream::Stdout, "running")
        .with_parent(tool)
        .with_status(EntryStatus::InProgress),
);

transcript.append_chunk(stdout, " 58 tests").unwrap();
transcript.set_status(stdout, EntryStatus::Complete).unwrap();
transcript.append(
    NewEntry::tool_result("cargo test", ToolOutcome::Success, "ok")
        .with_cause(tool),
);
transcript.set_status(tool, EntryStatus::Complete).unwrap();

assert_eq!(transcript.children_of(tool), &[stdout]);
assert_eq!(transcript.entries().len(), 4);
```
