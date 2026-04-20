// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transcript storage and mutation APIs.

extern crate alloc;

use alloc::vec::Vec;

use hashbrown::HashMap;

use crate::body::{BodyAppendError, EntryBody};
use crate::entry::{NewEntry, TranscriptEntry};
use crate::ids::EntryId;
use crate::status::EntryStatus;

/// Append-order transcript storage with explicit in-place updates.
///
/// `Transcript` stores entries in append order and keeps lightweight indices
/// for id lookup and parent-child traversal. It supports explicit chunk and
/// status updates for streaming or long-running entries without taking on UI,
/// persistence, or append-only event-log policy.
#[derive(Clone, Debug, Default)]
pub struct Transcript<P = EntryBody> {
    entries: Vec<TranscriptEntry<P>>,
    indices: HashMap<EntryId, usize>,
    children: HashMap<EntryId, Vec<EntryId>>,
    next_id: u64,
}

impl<P> Transcript<P> {
    /// Creates an empty transcript.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            indices: HashMap::new(),
            children: HashMap::new(),
            next_id: 0,
        }
    }

    /// Returns the number of stored entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the transcript has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Appends a new entry and returns its assigned id.
    pub fn append(&mut self, entry: NewEntry<P>) -> EntryId {
        let id = EntryId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("transcript entry id overflow");

        let stored = TranscriptEntry {
            id,
            timestamp: entry.timestamp,
            parent: entry.parent,
            cause: entry.cause,
            status: entry.status,
            kind: entry.kind,
        };

        let index = self.entries.len();
        self.entries.push(stored);
        self.indices.insert(id, index);

        if let Some(parent) = self.entries[index].parent {
            self.children.entry(parent).or_default().push(id);
        }

        id
    }

    /// Sets the lifecycle status for an entry.
    pub fn set_status(
        &mut self,
        id: EntryId,
        status: EntryStatus,
    ) -> Result<bool, TranscriptError> {
        let entry = self.entry_mut(id)?;
        if entry.status == status {
            return Ok(false);
        }
        entry.status = status;
        Ok(true)
    }

    /// Returns one entry by id.
    #[must_use]
    pub fn entry(&self, id: EntryId) -> Option<&TranscriptEntry<P>> {
        self.indices
            .get(&id)
            .and_then(|index| self.entries.get(*index))
    }

    /// Returns the append-order index for an entry id.
    #[must_use]
    pub fn index_of(&self, id: EntryId) -> Option<usize> {
        self.indices.get(&id).copied()
    }

    /// Returns all transcript entries in append order.
    #[must_use]
    pub fn entries(&self) -> &[TranscriptEntry<P>] {
        self.entries.as_slice()
    }

    /// Iterates over transcript entries in append order.
    pub fn iter(&self) -> impl Iterator<Item = &TranscriptEntry<P>> {
        self.entries.iter()
    }

    /// Returns the direct children of a parent entry in append order.
    #[must_use]
    pub fn children_of(&self, id: EntryId) -> &[EntryId] {
        self.children.get(&id).map_or(&[], Vec::as_slice)
    }

    fn entry_mut(&mut self, id: EntryId) -> Result<&mut TranscriptEntry<P>, TranscriptError> {
        let Some(index) = self.indices.get(&id).copied() else {
            return Err(TranscriptError::UnknownEntry { entry: id });
        };
        Ok(&mut self.entries[index])
    }
}

impl Transcript<EntryBody> {
    /// Appends a text or byte chunk to an existing entry body.
    pub fn append_chunk(
        &mut self,
        id: EntryId,
        chunk: impl Into<EntryBody>,
    ) -> Result<(), TranscriptError> {
        let entry = self.entry_mut(id)?;
        let body = entry
            .kind
            .body_mut()
            .ok_or(TranscriptError::BodyKindMismatch { entry: id })?;
        body.append(chunk.into())
            .map_err(
                |BodyAppendError::KindMismatch| TranscriptError::BodyKindMismatch { entry: id },
            )
    }
}

/// Errors returned by transcript mutation APIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptError {
    /// No entry exists for the provided id.
    UnknownEntry {
        /// Missing entry id.
        entry: EntryId,
    },
    /// The targeted entry body cannot accept the provided chunk type.
    BodyKindMismatch {
        /// Entry whose stored body kind rejected the chunk append.
        entry: EntryId,
    },
}

#[cfg(test)]
mod tests {
    use crate::entry::{AnnotationLevel, MessageRole, NewEntry, ProcessStream, ToolOutcome};

    use super::*;

    #[test]
    fn append_assigns_ids_and_parent_children() {
        let mut transcript = Transcript::<EntryBody>::new();

        let parent = transcript.append(NewEntry::message(MessageRole::User, "run tests"));
        let child = transcript
            .append(NewEntry::annotation(AnnotationLevel::Info, "queued").with_parent(parent));

        assert_eq!(parent, EntryId(0));
        assert_eq!(child, EntryId(1));
        assert_eq!(transcript.children_of(parent), &[child]);
        assert_eq!(transcript.index_of(child), Some(1));
    }

    #[test]
    fn append_chunk_extends_text_entries() {
        let mut transcript = Transcript::<EntryBody>::new();
        let entry = transcript.append(
            NewEntry::process_output(ProcessStream::Stdout, "running")
                .with_status(EntryStatus::InProgress),
        );

        transcript.append_chunk(entry, " tests").unwrap();
        transcript.append_chunk(entry, EntryBody::Empty).unwrap();

        let stored = transcript.entry(entry).unwrap();
        let EntryBody::Text(text) = stored.body().cloned().unwrap_or(EntryBody::Empty) else {
            panic!("expected text body");
        };
        assert_eq!(text, "running tests");
    }

    #[test]
    fn append_chunk_rejects_body_kind_mismatch() {
        let mut transcript = Transcript::<EntryBody>::new();
        let entry = transcript.append(NewEntry::tool_call("cargo", "test"));

        let error = transcript
            .append_chunk(entry, vec![1_u8, 2, 3])
            .unwrap_err();
        assert_eq!(error, TranscriptError::BodyKindMismatch { entry });
    }

    #[test]
    fn set_status_reports_change() {
        let mut transcript = Transcript::<EntryBody>::new();
        let entry = transcript.append(
            NewEntry::tool_result("cargo", ToolOutcome::Success, "ok")
                .with_status(EntryStatus::InProgress),
        );

        assert!(transcript.set_status(entry, EntryStatus::Complete).unwrap());
        assert!(!transcript.set_status(entry, EntryStatus::Complete).unwrap());
        assert_eq!(
            transcript.entry(entry).unwrap().status,
            EntryStatus::Complete
        );
    }

    #[test]
    fn unknown_entry_errors_are_explicit() {
        let mut transcript = Transcript::<EntryBody>::new();
        let id = EntryId(77);

        assert_eq!(
            transcript.append_chunk(id, "hello").unwrap_err(),
            TranscriptError::UnknownEntry { entry: id }
        );
        assert_eq!(
            transcript.set_status(id, EntryStatus::Failed).unwrap_err(),
            TranscriptError::UnknownEntry { entry: id }
        );
    }

    #[test]
    fn transcript_can_store_host_defined_payloads() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum Payload {
            Fragment(u32),
            Object(&'static str),
        }

        let mut transcript = Transcript::<Payload>::new();
        let entry = transcript.append(NewEntry::message(
            MessageRole::Assistant,
            Payload::Fragment(7),
        ));
        let annotation = transcript.append(
            NewEntry::annotation(AnnotationLevel::Info, Payload::Object("commit:abc123"))
                .with_parent(entry),
        );

        assert_eq!(
            transcript.entry(entry).unwrap().body(),
            Some(&Payload::Fragment(7))
        );
        assert_eq!(transcript.children_of(entry), &[annotation]);
        assert_eq!(
            transcript.entry(annotation).unwrap().body(),
            Some(&Payload::Object("commit:abc123"))
        );
    }
}
