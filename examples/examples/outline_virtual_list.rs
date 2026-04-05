// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Outline + virtual-list composition.
//!
//! Project visible rows from a domain model with `understory_outline`, then
//! keep `understory_virtual_list` in sync as expansion changes the visible-row
//! count.
//!
//! Run:
//! - `cargo run -p understory_examples --example outline_virtual_list`

use std::vec::Vec;

use understory_outline::{Outline, OutlineModel};
use understory_virtual_list::{FixedExtentModel, VirtualList};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RowKey {
    Section(usize),
    Field(usize),
}

struct Section<'a> {
    label: &'a str,
    first_field: Option<usize>,
    next_section: Option<usize>,
}

struct Field<'a> {
    label: &'a str,
    next_field: Option<usize>,
}

struct InspectorModel<'a> {
    sections: &'a [Section<'a>],
    fields: &'a [Field<'a>],
}

impl<'a> OutlineModel for InspectorModel<'a> {
    type Key = RowKey;
    type Item = &'a str;

    fn first_root_key(&self) -> Option<Self::Key> {
        (!self.sections.is_empty()).then_some(RowKey::Section(0))
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        match *key {
            RowKey::Section(index) => index < self.sections.len(),
            RowKey::Field(index) => index < self.fields.len(),
        }
    }

    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match *key {
            RowKey::Section(index) => self.sections[index].next_section.map(RowKey::Section),
            RowKey::Field(index) => self.fields[index].next_field.map(RowKey::Field),
        }
    }

    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match *key {
            RowKey::Section(index) => self.sections[index].first_field.map(RowKey::Field),
            RowKey::Field(_) => None,
        }
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        match *key {
            RowKey::Section(index) => self.sections.get(index).map(|section| section.label),
            RowKey::Field(index) => self.fields.get(index).map(|field| field.label),
        }
    }
}

fn main() {
    let sections = [
        Section {
            label: "Transforms",
            first_field: Some(0),
            next_section: Some(1),
        },
        Section {
            label: "Appearance",
            first_field: Some(3),
            next_section: None,
        },
    ];
    let fields = [
        Field {
            label: "Position",
            next_field: Some(1),
        },
        Field {
            label: "Rotation",
            next_field: Some(2),
        },
        Field {
            label: "Scale",
            next_field: None,
        },
        Field {
            label: "Fill",
            next_field: Some(4),
        },
        Field {
            label: "Stroke",
            next_field: None,
        },
    ];
    let model = InspectorModel {
        sections: &sections,
        fields: &fields,
    };
    let mut outline = Outline::new(model);
    let model = FixedExtentModel::new(outline.visible_len(), 18.0);
    let mut list = VirtualList::new(model, 36.0, 18.0);

    println!("Initially collapsed:");
    sync_list_from_outline(&mut outline, &mut list);
    print_visible_rows(&mut outline, &mut list);

    let _ = outline.set_expanded(RowKey::Section(0), true);
    println!("\nAfter expanding Transforms:");
    sync_list_from_outline(&mut outline, &mut list);
    print_visible_rows(&mut outline, &mut list);

    let _ = outline.set_expanded(RowKey::Section(1), true);
    println!("\nAfter expanding Appearance:");
    sync_list_from_outline(&mut outline, &mut list);
    println!("Host scrolls down by one row to inspect the newly added content.");
    list.set_scroll_offset(18.0);
    print_visible_rows(&mut outline, &mut list);

    let _ = outline.set_expanded(RowKey::Section(0), false);
    println!("\nAfter collapsing Transforms again:");
    sync_list_from_outline(&mut outline, &mut list);
    print_visible_rows(&mut outline, &mut list);
}

fn sync_list_from_outline(
    outline: &mut Outline<InspectorModel<'_>>,
    list: &mut VirtualList<FixedExtentModel<f64>>,
) {
    let visible_len = outline.visible_len();
    list.model_mut().set_len(visible_len);
    list.clamp_scroll_to_content();
}

fn print_visible_rows(
    outline: &mut Outline<InspectorModel<'_>>,
    list: &mut VirtualList<FixedExtentModel<f64>>,
) {
    let strip = list.visible_strip();
    println!(
        "rows={} realized={}..{} scroll={} viewport={} overscan_before={} overscan_after={} before={} after={}",
        outline.visible_len(),
        strip.start,
        strip.end,
        list.scroll_offset(),
        list.viewport_extent(),
        list.overscan_before(),
        list.overscan_after(),
        strip.before_extent,
        strip.after_extent
    );

    let visible_rows: Vec<_> = (strip.start..strip.end)
        .filter_map(|index| outline.visible_row(index).map(|row| (row.key, row.depth)))
        .collect();

    for (key, depth) in visible_rows {
        let label = outline.item(&key).expect("row key should resolve");
        let indent = "  ".repeat(depth);
        println!("{indent}- {label}");
    }
}
