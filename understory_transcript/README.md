<div align="center">

# Understory Transcript

**Append-order interaction log primitives with generic payloads and explicit updates**

[![Latest published version.](https://img.shields.io/crates/v/understory_transcript.svg)](https://crates.io/crates/understory_transcript)
[![Documentation build status.](https://img.shields.io/docsrs/understory_transcript.svg)](https://docs.rs/understory_transcript)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/forest-rs/understory/ci.yml?logo=github&label=CI)](https://github.com/forest-rs/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_transcript --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Transcript: append-order interaction log primitives.

This crate provides a small, renderer-agnostic core for recording chat,
shell, and agent-style interactions as a stable transcript.

The core concepts are:

- [`Transcript`]: append-order storage with id and parent-child indices.
- [`EntryId`]: stable identity for one recorded entry.
- [`TranscriptEntry`]: stored normalized entry record.
- [`NewEntry`]: append-time constructor for a new entry.
- [`EntryKind`]: typed entry payloads for messages, tools, process output,
  annotations, and state changes.
- [`EntryBody`]: the built-in text/bytes/empty payload type used by default.

This crate deliberately does **not** know about:

- text layout or styling,
- rendering or widgets,
- transport or protocol semantics,
- persistence backends,
- terminal emulation.

## Overview

Goal:
provide a calm append-order interaction substrate for chat, shell, and
agent-style systems.

Non-goals:
own layout, rendering, persistence, or text styling.

## Fence

This crate owns append-order interaction records, their explicit update
semantics, and their structural links; it explicitly does not own layout,
rendering, protocol semantics, or text styling.

## Invariants

- Entry ids are assigned by the transcript and remain stable afterward.
- Entries preserve append order even when later statuses or chunks are added.
- Bodies and statuses may be updated in place through explicit mutation APIs;
  this crate is append-oriented, not a strict append-only event log.
- Transcript payloads are generic. [`EntryBody`] is the built-in default,
  but hosts may store fragment ids, object handles, or other structured
  payloads instead.
- `parent` and `cause` are explicit links; they are never inferred from
  position alone.
- The built-in [`EntryBody`] payload is intentionally simple: text, bytes,
  or empty.
- Chunk appends are only available for the built-in [`EntryBody`] payload
  today and reject text/byte kind mismatches.

## Why not just use a message list?

Because agent and shell systems quickly need more than plain messages:

- tool calls and results,
- stdout/stderr-like output,
- status transitions,
- structural nesting,
- causal links.

A transcript is a better center than a chat-only message model.

## Payloads

`Transcript` is generic over payload type and defaults to [`EntryBody`]:

```rust
use understory_transcript::{MessageRole, NewEntry, Transcript};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Payload {
    Fragment(u32),
    PresentedObject(&'static str),
}

let mut transcript = Transcript::<Payload>::new();
let entry = transcript.append(NewEntry::message(MessageRole::Assistant, Payload::Fragment(7)));

assert_eq!(transcript.entry(entry).unwrap().body(), Some(&Payload::Fragment(7)));
```

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

<!-- cargo-rdme end -->

[`EntryBody`]: https://docs.rs/understory_transcript/latest/understory_transcript/enum.EntryBody.html
[`EntryId`]: https://docs.rs/understory_transcript/latest/understory_transcript/struct.EntryId.html
[`EntryKind`]: https://docs.rs/understory_transcript/latest/understory_transcript/enum.EntryKind.html
[`NewEntry`]: https://docs.rs/understory_transcript/latest/understory_transcript/struct.NewEntry.html
[`Transcript`]: https://docs.rs/understory_transcript/latest/understory_transcript/struct.Transcript.html
[`TranscriptEntry`]: https://docs.rs/understory_transcript/latest/understory_transcript/struct.TranscriptEntry.html

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE] or <http://www.apache.org/licenses/LICENSE-2.0>), or
- MIT license ([LICENSE-MIT] or <http://opensource.org/licenses/MIT>),

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Contribution

Contributions are welcome by pull request. The [Rust code of conduct] applies.
Please feel free to add your name to the [AUTHORS] file in any substantive pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[LICENSE-APACHE]: https://github.com/forest-rs/understory/blob/main/LICENSE-APACHE
[LICENSE-MIT]: https://github.com/forest-rs/understory/blob/main/LICENSE-MIT
[Rust code of conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: https://github.com/forest-rs/understory/blob/main/AUTHORS
