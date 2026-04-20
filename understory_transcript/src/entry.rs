// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transcript entry types and constructors.

extern crate alloc;

use alloc::string::String;

use crate::body::EntryBody;
use crate::ids::{EntryId, Timestamp};
use crate::status::EntryStatus;

/// One stored transcript entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptEntry<P = EntryBody> {
    /// Stable identifier assigned by the transcript.
    pub id: EntryId,
    /// Optional host-defined timestamp.
    pub timestamp: Option<Timestamp>,
    /// Structural parent link, when the entry is nested under another entry.
    pub parent: Option<EntryId>,
    /// Causal link, when the entry was produced because of another entry.
    pub cause: Option<EntryId>,
    /// Current lifecycle status.
    pub status: EntryStatus,
    /// Typed entry payload.
    pub kind: EntryKind<P>,
}

/// Append-time description of a new transcript entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewEntry<P = EntryBody> {
    pub(crate) timestamp: Option<Timestamp>,
    pub(crate) parent: Option<EntryId>,
    pub(crate) cause: Option<EntryId>,
    pub(crate) status: EntryStatus,
    pub(crate) kind: EntryKind<P>,
}

impl<P> NewEntry<P> {
    /// Creates a message entry with a role and body.
    #[must_use]
    pub fn message(role: MessageRole, body: impl Into<P>) -> Self {
        Self::new(EntryKind::Message(MessageEntry {
            role,
            body: body.into(),
        }))
    }

    /// Creates a tool-call entry.
    #[must_use]
    pub fn tool_call(tool_name: impl Into<String>, input: impl Into<P>) -> Self {
        Self::new(EntryKind::ToolCall(ToolCallEntry {
            tool_name: tool_name.into(),
            input: input.into(),
        }))
    }

    /// Creates a tool-result entry.
    #[must_use]
    pub fn tool_result(
        tool_name: impl Into<String>,
        outcome: ToolOutcome,
        output: impl Into<P>,
    ) -> Self {
        Self::new(EntryKind::ToolResult(ToolResultEntry {
            tool_name: tool_name.into(),
            outcome,
            output: output.into(),
        }))
    }

    /// Creates a process-output entry.
    #[must_use]
    pub fn process_output(stream: ProcessStream, body: impl Into<P>) -> Self {
        Self::new(EntryKind::ProcessOutput(ProcessOutputEntry {
            stream,
            body: body.into(),
        }))
    }

    /// Creates an annotation entry.
    #[must_use]
    pub fn annotation(level: AnnotationLevel, body: impl Into<P>) -> Self {
        Self::new(EntryKind::Annotation(AnnotationEntry {
            level,
            body: body.into(),
        }))
    }

    /// Creates a state entry.
    #[must_use]
    pub fn state(label: impl Into<String>, body: impl Into<P>) -> Self {
        Self::new(EntryKind::State(StateEntry {
            label: label.into(),
            body: body.into(),
        }))
    }

    /// Adds a structural parent link.
    #[must_use]
    pub fn with_parent(mut self, parent: EntryId) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Adds a causal link.
    #[must_use]
    pub fn with_cause(mut self, cause: EntryId) -> Self {
        self.cause = Some(cause);
        self
    }

    /// Sets an explicit entry status.
    #[must_use]
    pub fn with_status(mut self, status: EntryStatus) -> Self {
        self.status = status;
        self
    }

    /// Sets a host-defined timestamp.
    #[must_use]
    pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    fn new(kind: EntryKind<P>) -> Self {
        Self {
            timestamp: None,
            parent: None,
            cause: None,
            status: EntryStatus::Complete,
            kind,
        }
    }
}

/// Typed payload for one transcript entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKind<P = EntryBody> {
    /// Conversational or user/system-style message.
    Message(MessageEntry<P>),
    /// Tool invocation request.
    ToolCall(ToolCallEntry<P>),
    /// Tool invocation result.
    ToolResult(ToolResultEntry<P>),
    /// Process or terminal output.
    ProcessOutput(ProcessOutputEntry<P>),
    /// Host annotation such as warnings, notes, or status lines.
    Annotation(AnnotationEntry<P>),
    /// Named state change or checkpoint entry.
    State(StateEntry<P>),
}

impl<P> EntryKind<P> {
    /// Returns the payload body for entry kinds that store one.
    #[must_use]
    pub fn body(&self) -> Option<&P> {
        match self {
            Self::Message(entry) => Some(&entry.body),
            Self::ToolCall(entry) => Some(&entry.input),
            Self::ToolResult(entry) => Some(&entry.output),
            Self::ProcessOutput(entry) => Some(&entry.body),
            Self::Annotation(entry) => Some(&entry.body),
            Self::State(entry) => Some(&entry.body),
        }
    }
}

impl EntryKind<EntryBody> {
    pub(crate) fn body_mut(&mut self) -> Option<&mut EntryBody> {
        match self {
            Self::Message(entry) => Some(&mut entry.body),
            Self::ToolCall(entry) => Some(&mut entry.input),
            Self::ToolResult(entry) => Some(&mut entry.output),
            Self::ProcessOutput(entry) => Some(&mut entry.body),
            Self::Annotation(entry) => Some(&mut entry.body),
            Self::State(entry) => Some(&mut entry.body),
        }
    }
}

/// Message-style entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageEntry<P = EntryBody> {
    /// Semantic role of the message.
    pub role: MessageRole,
    /// Message payload body.
    pub body: P,
}

/// Role associated with a message entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MessageRole {
    /// System instruction or setup.
    System,
    /// End-user input.
    User,
    /// Assistant or model output.
    Assistant,
    /// Tool-originated message.
    Tool,
    /// Unclassified message role.
    Other,
}

/// Tool invocation request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCallEntry<P = EntryBody> {
    /// Tool name or operation identifier.
    pub tool_name: String,
    /// Tool input payload.
    pub input: P,
}

/// Tool invocation result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResultEntry<P = EntryBody> {
    /// Tool name or operation identifier.
    pub tool_name: String,
    /// Tool completion status.
    pub outcome: ToolOutcome,
    /// Tool output payload.
    pub output: P,
}

/// Outcome reported by a tool result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolOutcome {
    /// The tool completed successfully.
    Success,
    /// The tool reported failure.
    Failure,
    /// The tool was cancelled.
    Cancelled,
}

/// Process or terminal output entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessOutputEntry<P = EntryBody> {
    /// Logical output stream.
    pub stream: ProcessStream,
    /// Output payload body.
    pub body: P,
}

/// Stream classification for process output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProcessStream {
    /// Standard input.
    Stdin,
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// Annotation entry for notes, warnings, and similar host-generated records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnnotationEntry<P = EntryBody> {
    /// Annotation severity or tone.
    pub level: AnnotationLevel,
    /// Annotation payload body.
    pub body: P,
}

/// Severity or tone for annotation entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnnotationLevel {
    /// Informational note.
    Info,
    /// Non-fatal warning.
    Warning,
    /// Error or failure note.
    Error,
}

/// Named state change or checkpoint entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateEntry<P = EntryBody> {
    /// Host-defined label for the state update.
    pub label: String,
    /// Additional state payload.
    pub body: P,
}

impl<P> TranscriptEntry<P> {
    /// Returns the payload body for entry kinds that store one.
    #[must_use]
    pub fn body(&self) -> Option<&P> {
        self.kind.body()
    }
}
