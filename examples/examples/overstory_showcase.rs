// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Overstory retained UI showcase.
//!
//! This example composes:
//! - `overstory` for retained widgets/runtime,
//! - `understory_property` + `understory_style` through the Overstory API,
//! - `understory_box_tree` through the derived scene snapshot,
//! - `ui-events` via Overstory's pointer runtime.
//!
//! Run:
//! - `cargo run -p understory_examples --example overstory_showcase`

use kurbo::Rect;
use overstory::ui_events::pointer::{
    PointerButton, PointerButtonEvent, PointerButtons, PointerEvent, PointerId, PointerInfo,
    PointerState, PointerType, PointerUpdate,
};
use overstory::{ButtonClass, ElementId, LayoutClass, Ui, default_theme};
use understory_style::{
    IdSet, Selector, StyleBuilder, StyleCascadeBuilder, StyleOrigin, StyleSheetBuilder,
};

fn main() {
    let mut ui = build_showcase_ui();

    println!("== Initial Scene ==");
    print_scene(&mut ui);

    let compose = find_by_label(&ui, "Compose").expect("compose button");
    let compose_rect = ui
        .scene()
        .resolved_element(compose)
        .expect("compose resolved")
        .rect;
    let compose_center = compose_rect.center();

    println!("\n== Move to Compose ==");
    let move_batch = ui.handle_pointer_event(&PointerEvent::Move(PointerUpdate {
        pointer: primary_pointer(),
        current: pointer_state(compose_center.x, compose_center.y, 1),
        coalesced: Vec::new(),
        predicted: Vec::new(),
    }));
    print_interactions(&move_batch);
    print_scene(&mut ui);

    println!("\n== Press Compose ==");
    let down_batch = ui.handle_pointer_event(&PointerEvent::Down(PointerButtonEvent {
        button: Some(PointerButton::Primary),
        pointer: primary_pointer(),
        state: pointer_state(compose_center.x, compose_center.y, 2),
    }));
    print_interactions(&down_batch);
    print_scene(&mut ui);

    println!("\n== Release Compose ==");
    let up_batch = ui.handle_pointer_event(&PointerEvent::Up(PointerButtonEvent {
        button: Some(PointerButton::Primary),
        pointer: primary_pointer(),
        state: pointer_state(compose_center.x, compose_center.y, 3),
    }));
    print_interactions(&up_batch);
    print_scene(&mut ui);

    println!("\n== Pointer leaves the UI ==");
    let leave_batch = ui.handle_pointer_event(&PointerEvent::Leave(primary_pointer()));
    print_interactions(&leave_batch);
    print_scene(&mut ui);
}

fn build_showcase_ui() -> Ui {
    let mut ui = Ui::new(default_theme());
    ui.set_view_rect(Rect::new(0.0, 0.0, 420.0, 320.0));
    ui.set_local(ui.root(), ui.properties().padding, 20.0);
    ui.set_local(ui.root(), ui.properties().gap, 16.0);

    let button_cascade = make_button_cascade(&ui);

    let shell = ui.append_child(ui.root(), overstory::TYPE_ROW);
    ui.set_local(shell, ui.properties().padding, 0.0);
    ui.set_local(shell, ui.properties().gap, 16.0);

    let sidebar = ui.append_child(shell, overstory::TYPE_PANEL);
    ui.add_layout_class(sidebar, LayoutClass::Sidebar);
    ui.set_local(sidebar, ui.properties().width, 150.0);
    ui.set_local(sidebar, ui.properties().padding, 18.0);
    ui.set_local(sidebar, ui.properties().gap, 10.0);

    let actions = ui.append_child(sidebar, overstory::TYPE_COLUMN);
    ui.set_local(actions, ui.properties().padding, 0.0);
    ui.set_local(actions, ui.properties().gap, 10.0);

    let compose = ui.append_child(actions, overstory::TYPE_BUTTON);
    ui.set_label(compose, "Compose");
    ui.add_button_class(compose, ButtonClass::Primary);
    ui.set_style(compose, button_cascade.clone());

    let archive = ui.append_child(actions, overstory::TYPE_BUTTON);
    ui.set_label(archive, "Archive");
    ui.set_style(archive, button_cascade.clone());

    let content = ui.append_child(shell, overstory::TYPE_PANEL);
    ui.set_local(content, ui.properties().padding, 18.0);
    ui.set_local(content, ui.properties().gap, 12.0);

    let content_column = ui.append_child(content, overstory::TYPE_COLUMN);
    ui.set_local(content_column, ui.properties().padding, 0.0);
    ui.set_local(content_column, ui.properties().gap, 12.0);

    let search = ui.append_child(content_column, overstory::TYPE_BUTTON);
    ui.set_label(search, "Search");
    ui.set_style(search, button_cascade.clone());
    ui.set_local(search, ui.properties().height, 52.0);

    let settings = ui.append_child(content_column, overstory::TYPE_BUTTON);
    ui.set_label(settings, "Settings");
    ui.set_style(settings, button_cascade);

    ui
}

fn make_button_cascade(ui: &Ui) -> understory_style::StyleCascade {
    let base = StyleBuilder::new()
        .set(ui.properties().border_width, 1.0)
        .set(ui.properties().padding, 12.0)
        .build();
    let hover = StyleBuilder::new()
        .set(ui.properties().border_width, 2.0)
        .build();
    let pressed = StyleBuilder::new()
        .set(ui.properties().border_width, 3.0)
        .build();
    let selector_hover = Selector {
        type_tag: Some(overstory::TYPE_BUTTON),
        required_classes: IdSet::default(),
        required_pseudos: IdSet::from_ids([overstory::PSEUDO_HOVER]),
    };
    let selector_pressed = Selector {
        type_tag: Some(overstory::TYPE_BUTTON),
        required_classes: IdSet::default(),
        required_pseudos: IdSet::from_ids([overstory::PSEUDO_PRESSED]),
    };
    let sheet = StyleSheetBuilder::new()
        .rule(selector_hover, hover)
        .rule(selector_pressed, pressed)
        .build();
    StyleCascadeBuilder::new()
        .push_style(StyleOrigin::Base, base)
        .push_sheet(StyleOrigin::Sheet, sheet)
        .build()
}

fn print_scene(ui: &mut Ui) {
    for element in ui.scene().resolved() {
        let indent = "  ".repeat(element.depth as usize);
        let background = element.background.to_rgba8();
        println!(
            "{}{:?} {:?} rect=({:.0},{:.0})-({:.0},{:.0}) bg=rgba({:02x},{:02x},{:02x},{:02x}) border={} hover={} pressed={} label={}",
            indent,
            element.type_tag,
            element.id,
            element.rect.x0,
            element.rect.y0,
            element.rect.x1,
            element.rect.y1,
            background.r,
            background.g,
            background.b,
            background.a,
            element.border.width,
            element.hovered,
            element.pressed,
            element.label.as_deref().unwrap_or("-"),
        );
    }
}

fn print_interactions(batch: &overstory::InteractionBatch) {
    if batch.is_empty() {
        println!("(no interactions)");
        return;
    }
    for event in batch.events() {
        println!("{event:?}");
    }
}

fn find_by_label(ui: &Ui, label: &str) -> Option<ElementId> {
    ui.elements()
        .iter()
        .find(|element| element.label() == Some(label))
        .map(|element| element.id())
}

fn primary_pointer() -> PointerInfo {
    PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Mouse,
    }
}

#[allow(
    clippy::field_reassign_with_default,
    reason = "Constructing dpi physical positions without a direct example dependency is awkward."
)]
fn pointer_state(x: f64, y: f64, time: u64) -> PointerState {
    let mut state = PointerState::default();
    state.time = time;
    state.position.x = x;
    state.position.y = y;
    state.buttons = PointerButtons::new();
    state.count = 1;
    state.scale_factor = 1.0;
    state
}
