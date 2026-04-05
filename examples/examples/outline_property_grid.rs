// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Outline property-grid basics.
//!
//! Drive `understory_outline` from a domain model directly rather than a
//! parallel `OutlineNode` array.
//!
//! Run:
//! - `cargo run -p understory_examples --example outline_property_grid`

use std::vec::Vec;

use understory_outline::{Outline, OutlineModel};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RowKey {
    Group(usize),
    Property(usize),
}

struct Group<'a> {
    label: &'a str,
    first_property: Option<usize>,
    next_group: Option<usize>,
}

struct Property<'a> {
    label: &'a str,
    next_property: Option<usize>,
}

struct PropertyGridModel<'a> {
    groups: &'a [Group<'a>],
    properties: &'a [Property<'a>],
}

impl<'a> OutlineModel for PropertyGridModel<'a> {
    type Key = RowKey;
    type Item = &'a str;

    fn first_root_key(&self) -> Option<Self::Key> {
        (!self.groups.is_empty()).then_some(RowKey::Group(0))
    }

    fn contains_key(&self, key: &Self::Key) -> bool {
        match *key {
            RowKey::Group(index) => index < self.groups.len(),
            RowKey::Property(index) => index < self.properties.len(),
        }
    }

    fn next_sibling_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match *key {
            RowKey::Group(index) => self.groups[index].next_group.map(RowKey::Group),
            RowKey::Property(index) => self.properties[index].next_property.map(RowKey::Property),
        }
    }

    fn first_child_key(&self, key: &Self::Key) -> Option<Self::Key> {
        match *key {
            RowKey::Group(index) => self.groups[index].first_property.map(RowKey::Property),
            RowKey::Property(_) => None,
        }
    }

    fn item(&self, key: &Self::Key) -> Option<Self::Item> {
        match *key {
            RowKey::Group(index) => self.groups.get(index).map(|group| group.label),
            RowKey::Property(index) => self.properties.get(index).map(|property| property.label),
        }
    }
}

fn main() {
    let groups = [
        Group {
            label: "Transform",
            first_property: Some(0),
            next_group: Some(1),
        },
        Group {
            label: "Appearance",
            first_property: Some(3),
            next_group: None,
        },
    ];
    let properties = [
        Property {
            label: "Position",
            next_property: Some(1),
        },
        Property {
            label: "Rotation",
            next_property: Some(2),
        },
        Property {
            label: "Scale",
            next_property: None,
        },
        Property {
            label: "Fill",
            next_property: Some(4),
        },
        Property {
            label: "Stroke",
            next_property: None,
        },
    ];
    let model = PropertyGridModel {
        groups: &groups,
        properties: &properties,
    };
    let mut outline = Outline::new(model);

    println!("Collapsed outline:");
    print_rows(&mut outline);

    let _ = outline.set_expanded(RowKey::Group(0), true);
    println!("\nAfter expanding Transform:");
    print_rows(&mut outline);

    let _ = outline.set_expanded(RowKey::Group(1), true);
    println!("\nAfter expanding Appearance:");
    print_rows(&mut outline);
}

fn print_rows(outline: &mut Outline<PropertyGridModel<'_>>) {
    let rows: Vec<_> = outline
        .visible_rows()
        .iter()
        .map(|row| (row.key, row.depth, row.has_children, row.is_expanded))
        .collect();

    for (key, depth, has_children, is_expanded) in rows {
        let label = outline.item(&key).expect("visible row should resolve");
        let indent = "  ".repeat(depth);
        let marker = match (has_children, is_expanded) {
            (true, true) => "[-]",
            (true, false) => "[+]",
            (false, _) => "   ",
        };
        println!("{indent}{marker} {label} ({key:?})");
    }
}
