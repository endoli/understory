// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integrated outline inspector host loop.
//!
//! This example keeps four layers in sync:
//! - `understory_outline` projects visible rows from a domain model,
//! - `understory_virtual_list` realizes only the current viewport slice,
//! - `understory_selection` owns anchor/primary bookkeeping,
//! - `understory_inspector` wraps that glue in a small host-facing controller.
//!
//! Run:
//! - `cargo run -p understory_examples --example outline_inspector`

use std::vec::Vec;

use understory_inspector::{Inspector, InspectorConfig, InspectorModel};
use understory_outline::OutlineModel;

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
    parent_section: usize,
}

struct PropertyInspectorModel<'a> {
    sections: &'a [Section<'a>],
    fields: &'a [Field<'a>],
}

impl<'a> OutlineModel for PropertyInspectorModel<'a> {
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

impl InspectorModel for PropertyInspectorModel<'_> {
    fn parent_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match *key {
            RowKey::Section(_) => None,
            RowKey::Field(index) => self
                .fields
                .get(index)
                .map(|field| RowKey::Section(field.parent_section)),
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
            next_section: Some(2),
        },
        Section {
            label: "Layout",
            first_field: Some(5),
            next_section: None,
        },
    ];
    let fields = [
        Field {
            label: "Position",
            next_field: Some(1),
            parent_section: 0,
        },
        Field {
            label: "Rotation",
            next_field: Some(2),
            parent_section: 0,
        },
        Field {
            label: "Scale",
            next_field: None,
            parent_section: 0,
        },
        Field {
            label: "Fill",
            next_field: Some(4),
            parent_section: 1,
        },
        Field {
            label: "Stroke",
            next_field: None,
            parent_section: 1,
        },
        Field {
            label: "Width",
            next_field: Some(6),
            parent_section: 2,
        },
        Field {
            label: "Height",
            next_field: None,
            parent_section: 2,
        },
    ];

    let model = PropertyInspectorModel {
        sections: &sections,
        fields: &fields,
    };
    let mut inspector = Inspector::new(model, InspectorConfig::fixed_rows(18.0, 54.0));
    let _ = inspector.focus_first();
    let _ = inspector.select_only_focused();

    print_state("Initial host state", &mut inspector);

    println!("\nKeyboard Next moves focus from Transforms to Appearance.");
    let _ = inspector.focus_next();
    let _ = inspector.select_only_focused();
    print_state("After moving focus", &mut inspector);

    println!("\nHost expands the focused section and resyncs the virtual list length.");
    let _ = inspector.expand_focused();
    print_state("After expanding Appearance", &mut inspector);

    println!(
        "\nKeyboard Next follows the visible-row order into the first child; selection stays anchored on Appearance."
    );
    let _ = inspector.focus_next();
    print_state("After moving focus into the first child", &mut inspector);

    println!("\nShift+Next extends selection by visible-row order from the Appearance anchor.");
    let _ = inspector.extend_selection_next();
    print_state("After range select", &mut inspector);

    println!("\nKeyboard Next moves focus to Layout; the host scrolls to keep it fully visible.");
    let _ = inspector.focus_next();
    print_state("After moving focus to Layout", &mut inspector);

    println!(
        "\nHost collapses Appearance; hidden child selection is pruned and focus stays valid."
    );
    let _ = inspector.collapse(RowKey::Section(1));
    print_state("After collapsing Appearance", &mut inspector);
}

fn print_state(title: &str, inspector: &mut Inspector<PropertyInspectorModel<'_>>) {
    let focus = inspector.focus().copied();
    let (selection_items, selection_primary, selection_anchor) = {
        let selection = inspector.selection();
        (
            selection.items().to_vec(),
            selection.primary().copied(),
            selection.anchor().copied(),
        )
    };
    let realized = inspector.realized_range();
    let rows = inspector
        .visible_rows()
        .iter()
        .map(|row| (row.key, row.depth, row.has_children, row.is_expanded))
        .collect::<Vec<_>>();

    println!("{title}:");
    println!(
        "focus={focus:?} selection={selection_items:?} primary={selection_primary:?} anchor={selection_anchor:?} realized={}..{} scroll={:?} viewport={:?} overscan_before={:?} overscan_after={:?}",
        realized.start,
        realized.end,
        inspector.scroll_offset(),
        inspector.viewport_extent(),
        inspector.overscan_before(),
        inspector.overscan_after()
    );

    for (index, (key, depth, has_children, is_expanded)) in rows.iter().enumerate() {
        let label = inspector.item(key).expect("visible row should resolve");
        let indent = "  ".repeat(*depth);
        let focus_marker = if Some(*key) == focus { ">" } else { " " };
        let selection_marker = if selection_items.contains(key) {
            "*"
        } else {
            " "
        };
        let realized_marker = if index >= realized.start && index < realized.end {
            "R"
        } else {
            " "
        };
        let disclosure = match (*has_children, *is_expanded) {
            (true, true) => "[-]",
            (true, false) => "[+]",
            (false, _) => "   ",
        };
        println!(
            "  [{realized_marker}{focus_marker}{selection_marker}] {indent}{disclosure} {label} ({key:?})"
        );
    }
}
