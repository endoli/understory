// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use understory_transcript::{MessageRole, NewEntry, Transcript, TranscriptEntry};
use understory_virtual_list::{PrefixSumExtentModel, TailAnchoredExtentModel, VirtualList};

fn main() {
    let mut transcript = Transcript::new();
    transcript.append(NewEntry::message(MessageRole::User, "hello"));
    transcript.append(NewEntry::message(
        MessageRole::Assistant,
        "hi there\nhow can I help?",
    ));
    transcript.append(NewEntry::message(
        MessageRole::User,
        "show me the last three commits",
    ));

    let inner = PrefixSumExtentModel::<f32>::new();
    let model = TailAnchoredExtentModel::with_default_epsilon(inner);
    let mut list = VirtualList::new(model, 4.0_f32, 0.5_f32);

    sync_tail_anchored(&transcript, &mut list);
    list.scroll_to_tail();

    println!("== Initially anchored to tail ==");
    print_tail_state(&transcript, &mut list);

    transcript.append(NewEntry::message(
        MessageRole::Assistant,
        "commit a1b2c3\ncommit d4e5f6\ncommit 123abc",
    ));
    sync_tail_anchored(&transcript, &mut list);

    println!();
    println!("== Appended while anchored ==");
    print_tail_state(&transcript, &mut list);

    list.set_scroll_offset(0.0_f32);
    transcript.append(NewEntry::message(
        MessageRole::Assistant,
        "another line that should not yank the user's scroll position",
    ));
    sync_tail_anchored(&transcript, &mut list);

    println!();
    println!("== Appended while user was reading earlier entries ==");
    print_tail_state(&transcript, &mut list);
}

fn sync_tail_anchored(
    transcript: &Transcript,
    list: &mut VirtualList<TailAnchoredExtentModel<PrefixSumExtentModel<f32>>>,
) {
    let was_at_tail = list.is_at_tail();
    list.model_mut()
        .inner_mut()
        .rebuild(transcript.iter().cloned(), &entry_extent);
    if was_at_tail {
        list.scroll_to_tail();
    }
}

fn entry_extent(entry: &TranscriptEntry) -> f32 {
    let lines = entry
        .body()
        .and_then(|body| body.as_text())
        .map_or(1, |text| text.lines().count().max(1));
    1.0_f32 + ((lines.saturating_sub(1)) as f32 * 0.8_f32)
}

fn print_tail_state(
    transcript: &Transcript,
    list: &mut VirtualList<TailAnchoredExtentModel<PrefixSumExtentModel<f32>>>,
) {
    let strip = list.visible_strip();
    println!(
        "scroll={:.1} is_at_tail={} realized={}..{} before={:.1} after={:.1} content={:.1}",
        list.scroll_offset(),
        list.is_at_tail(),
        strip.start,
        strip.end,
        strip.before_extent,
        strip.after_extent,
        strip.content_extent
    );

    for index in list.visible_indices() {
        let entry = &transcript.entries()[index];
        println!("- [{}] {:?}", index, entry.kind);
    }
}
