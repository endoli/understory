// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use understory_transcript::{
    AnnotationLevel, EntryBody, EntryKind, EntryStatus, MessageRole, NewEntry, ProcessStream,
    Timestamp, ToolOutcome, Transcript, TranscriptEntry,
};
use understory_virtual_list::{PrefixSumExtentModel, ScrollAlign, VirtualList};

fn main() {
    let transcript = build_transcript();
    let mut model = PrefixSumExtentModel::<f32>::new();
    model.rebuild(transcript.iter().cloned(), &entry_extent);

    let mut list = VirtualList::new(model, 5.0_f32, 1.0_f32);

    println!("== Transcript as variable-height virtual list ==");
    print_strip(&transcript, &mut list);

    println!();
    println!("== Scroll to tool output ==");
    list.scroll_to_index(2, ScrollAlign::Start);
    print_strip(&transcript, &mut list);

    println!();
    println!("== Center final result ==");
    list.scroll_to_index(transcript.len() - 1, ScrollAlign::Center);
    print_strip(&transcript, &mut list);
}

fn build_transcript() -> Transcript {
    let mut transcript = Transcript::new();

    let user = transcript.append(
        NewEntry::message(MessageRole::User, "run cargo test --workspace")
            .with_timestamp(Timestamp(1)),
    );
    let tool = transcript.append(
        NewEntry::tool_call("cargo test", EntryBody::Empty)
            .with_cause(user)
            .with_status(EntryStatus::InProgress)
            .with_timestamp(Timestamp(2)),
    );
    let stdout = transcript.append(
        NewEntry::process_output(
            ProcessStream::Stdout,
            "running 58 tests\nall green\nmerged doctests compilation took 0.24s",
        )
        .with_parent(tool)
        .with_status(EntryStatus::Complete)
        .with_timestamp(Timestamp(3)),
    );
    transcript.append(
        NewEntry::annotation(
            AnnotationLevel::Info,
            "workspace pass finished without warnings",
        )
        .with_parent(tool)
        .with_timestamp(Timestamp(4)),
    );
    transcript.append(
        NewEntry::tool_result("cargo test", ToolOutcome::Success, "58 passed")
            .with_cause(tool)
            .with_timestamp(Timestamp(5)),
    );

    transcript
        .append_chunk(stdout, "\nartifacts up to date")
        .unwrap();
    transcript.set_status(tool, EntryStatus::Complete).unwrap();

    transcript
}

fn entry_extent(entry: &TranscriptEntry) -> f32 {
    let base = match entry.kind {
        EntryKind::Message(_) => 1.4_f32,
        EntryKind::ToolCall(_) | EntryKind::ToolResult(_) => 1.2_f32,
        EntryKind::ProcessOutput(_) => 1.0_f32,
        EntryKind::Annotation(_) => 0.9_f32,
        EntryKind::State(_) => 1.0_f32,
    };

    let body_lines = match entry.body() {
        Some(EntryBody::Text(text)) => text.lines().count().max(1),
        Some(EntryBody::Bytes(_)) => 1,
        Some(EntryBody::Empty) | None => 1,
    };

    base + ((body_lines.saturating_sub(1)) as f32 * 0.8_f32)
}

fn print_strip(transcript: &Transcript, list: &mut VirtualList<PrefixSumExtentModel<f32>>) {
    let strip = list.visible_strip();
    println!(
        "scroll={:.1} viewport={:.1} overscan=({:.1},{:.1}) realized={}..{} before={:.1} after={:.1} content={:.1}",
        list.scroll_offset(),
        list.viewport_extent(),
        list.overscan_before(),
        list.overscan_after(),
        strip.start,
        strip.end,
        strip.before_extent,
        strip.after_extent,
        strip.content_extent
    );

    for index in list.visible_indices() {
        let entry = &transcript.entries()[index];
        println!("- [{}] {:.1} {:?}", index, entry_extent(entry), entry.kind);
    }
}
