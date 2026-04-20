// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use understory_transcript::{
    AnnotationLevel, EntryBody, EntryStatus, MessageRole, NewEntry, ProcessStream, Timestamp,
    ToolOutcome, Transcript,
};

fn main() {
    let mut transcript = Transcript::new();

    let user = transcript.append(
        NewEntry::message(MessageRole::User, "run cargo test").with_timestamp(Timestamp(1)),
    );
    let tool = transcript.append(
        NewEntry::tool_call("cargo test", EntryBody::Empty)
            .with_cause(user)
            .with_status(EntryStatus::InProgress)
            .with_timestamp(Timestamp(2)),
    );
    let stdout = transcript.append(
        NewEntry::process_output(ProcessStream::Stdout, "running")
            .with_parent(tool)
            .with_status(EntryStatus::InProgress)
            .with_timestamp(Timestamp(3)),
    );

    transcript.append_chunk(stdout, " 58 tests").unwrap();
    transcript.append_chunk(stdout, "\n").unwrap();
    transcript
        .set_status(stdout, EntryStatus::Complete)
        .unwrap();

    let note = transcript.append(
        NewEntry::annotation(AnnotationLevel::Info, "tool output captured")
            .with_parent(tool)
            .with_timestamp(Timestamp(4)),
    );

    transcript.append(
        NewEntry::tool_result("cargo test", ToolOutcome::Success, "58 passed")
            .with_cause(tool)
            .with_timestamp(Timestamp(5)),
    );
    transcript.set_status(tool, EntryStatus::Complete).unwrap();

    println!("Transcript: {} entries", transcript.len());
    println!("children(tool): {:?}", transcript.children_of(tool));
    println!("children(note): {:?}", transcript.children_of(note));
    println!();

    for entry in transcript.iter() {
        println!(
            "#{} ts={:?} parent={:?} cause={:?} status={:?}",
            entry.id.0,
            entry.timestamp.map(|ts| ts.0),
            entry.parent.map(|id| id.0),
            entry.cause.map(|id| id.0),
            entry.status
        );
        println!("  {:?}", entry.kind);
        if let Some(body) = entry.body() {
            println!("  body={:?}", body);
        }
    }
}
