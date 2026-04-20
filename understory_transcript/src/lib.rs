// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_transcript --heading-base-level=0

#![cfg_attr(not(feature = "std"), no_std)]

//! Understory Transcript: append-order interaction log primitives.
//!
//! This crate provides a small, renderer-agnostic core for recording chat,
//! shell, and agent-style interactions as a stable transcript.
//!
//! The core concepts are:
//!
//! - [`Transcript`]: append-order storage with id and parent-child indices.
//! - [`EntryId`]: stable identity for one recorded entry.
//! - [`TranscriptEntry`]: stored normalized entry record.
//! - [`NewEntry`]: append-time constructor for a new entry.
//! - [`EntryKind`]: typed entry payloads for messages, tools, process output,
//!   annotations, and state changes.
//! - [`EntryBody`]: the built-in text/bytes/empty payload type used by default.
//!
//! This crate deliberately does **not** know about:
//!
//! - text layout or styling,
//! - rendering or widgets,
//! - transport or protocol semantics,
//! - persistence backends,
//! - terminal emulation.
//!
//! ## Overview
//!
//! Goal:
//! provide a calm append-order interaction substrate for chat, shell, and
//! agent-style systems.
//!
//! Non-goals:
//! own layout, rendering, persistence, or text styling.
//!
//! ## Fence
//!
//! This crate owns append-order interaction records, their explicit update
//! semantics, and their structural links; it explicitly does not own layout,
//! rendering, protocol semantics, or text styling.
//!
//! ## Invariants
//!
//! - Entry ids are assigned by the transcript and remain stable afterward.
//! - Entries preserve append order even when later statuses or chunks are added.
//! - Bodies and statuses may be updated in place through explicit mutation APIs;
//!   this crate is append-oriented, not a strict append-only event log.
//! - Transcript payloads are generic. [`EntryBody`] is the built-in default,
//!   but hosts may store fragment ids, object handles, or other structured
//!   payloads instead.
//! - `parent` and `cause` are explicit links; they are never inferred from
//!   position alone.
//! - The built-in [`EntryBody`] payload is intentionally simple: text, bytes,
//!   or empty.
//! - Chunk appends are only available for the built-in [`EntryBody`] payload
//!   today and reject text/byte kind mismatches.
//!
//! ## Why not just use a message list?
//!
//! Because agent and shell systems quickly need more than plain messages:
//!
//! - tool calls and results,
//! - stdout/stderr-like output,
//! - status transitions,
//! - structural nesting,
//! - causal links.
//!
//! A transcript is a better center than a chat-only message model.
//!
//! ## Payloads
//!
//! `Transcript` is generic over payload type and defaults to [`EntryBody`]:
//!
//! ```rust
//! use understory_transcript::{MessageRole, NewEntry, Transcript};
//!
//! #[derive(Clone, Debug, PartialEq, Eq)]
//! enum Payload {
//!     Fragment(u32),
//!     PresentedObject(&'static str),
//! }
//!
//! let mut transcript = Transcript::<Payload>::new();
//! let entry = transcript.append(NewEntry::message(MessageRole::Assistant, Payload::Fragment(7)));
//!
//! assert_eq!(transcript.entry(entry).unwrap().body(), Some(&Payload::Fragment(7)));
//! ```
//!
//! ## Minimal example
//!
//! ```rust
//! use understory_transcript::{
//!     EntryBody, EntryStatus, MessageRole, NewEntry, ProcessStream, ToolOutcome, Transcript,
//! };
//!
//! let mut transcript = Transcript::new();
//!
//! let user = transcript.append(NewEntry::message(MessageRole::User, "run tests"));
//! let tool = transcript.append(
//!     NewEntry::tool_call("cargo test", EntryBody::Empty)
//!         .with_cause(user)
//!         .with_status(EntryStatus::InProgress),
//! );
//! let stdout = transcript.append(
//!     NewEntry::process_output(ProcessStream::Stdout, "running")
//!         .with_parent(tool)
//!         .with_status(EntryStatus::InProgress),
//! );
//!
//! transcript.append_chunk(stdout, " 58 tests").unwrap();
//! transcript.set_status(stdout, EntryStatus::Complete).unwrap();
//! transcript.append(
//!     NewEntry::tool_result("cargo test", ToolOutcome::Success, "ok")
//!         .with_cause(tool),
//! );
//! transcript.set_status(tool, EntryStatus::Complete).unwrap();
//!
//! assert_eq!(transcript.children_of(tool), &[stdout]);
//! assert_eq!(transcript.entries().len(), 4);
//! ```

extern crate alloc;

pub mod body;
pub mod entry;
pub mod ids;
pub mod status;
pub mod transcript;

pub use body::{BodyAppendError, EntryBody};
pub use entry::{
    AnnotationEntry, AnnotationLevel, EntryKind, MessageEntry, MessageRole, NewEntry,
    ProcessOutputEntry, ProcessStream, StateEntry, ToolCallEntry, ToolOutcome, ToolResultEntry,
    TranscriptEntry,
};
pub use ids::{EntryId, Timestamp};
pub use status::EntryStatus;
pub use transcript::{Transcript, TranscriptError};
