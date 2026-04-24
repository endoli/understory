// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Overstory autocomplete basics.
//!
//! Run:
//! - `cargo run -p understory_examples --example overstory_autocomplete_basics`

use kurbo::Rect;
use overstory::ui_events::keyboard::{Code, Key, KeyboardEvent, NamedKey};
use overstory::{Column, Interaction, TextInput, Ui, default_theme};
use overstory_autocomplete::{AutocompleteAction, AutocompleteController, AutocompleteOption};

fn main() {
    let mut ui = Ui::new(default_theme());
    ui.set_view_rect(Rect::new(0.0, 0.0, 420.0, 220.0));
    ui.set_local(ui.root(), ui.properties().padding, 16.0);
    ui.set_local(ui.root(), ui.properties().gap, 0.0);

    let column = ui.append(ui.root(), Column::new().padding(0.0).gap(8.0));
    let mut autocomplete = AutocompleteController::<u32>::append(
        &mut ui,
        column,
        TextInput::new(16.0)
            .width(220.0)
            .padding(8.0)
            .border_width(1.0)
            .corner_radius(6.0)
            .placeholder("Type a command"),
    );
    ui.set_focus(autocomplete.ids().input);

    for (time, ch, code) in [(1, "d", Code::KeyD), (2, "e", Code::KeyE)] {
        ui.set_now(time);
        let batch =
            ui.handle_keyboard_event(&KeyboardEvent::key_down(Key::Character(ch.into()), code));
        autocomplete.sync_query_from_input(&mut ui, time);
        print_batch(&batch);
    }

    println!("query={:?}", ui.text_buffer(autocomplete.ids().input));
    println!("next_deadline={:?}", autocomplete.next_deadline());

    let Some(AutocompleteAction::RefreshQuery(query)) = autocomplete
        .next_deadline()
        .and_then(|deadline| autocomplete.take_due_query(deadline))
    else {
        panic!("expected debounced query refresh");
    };
    println!("refresh_query={query:?}");

    autocomplete.set_options(
        &mut ui,
        &[
            AutocompleteOption::new(1, "Deploy"),
            AutocompleteOption::new(2, "Debug"),
            AutocompleteOption::new(3, "Delete"),
        ],
    );

    let plan = ui.surface_plan();
    let dropdown = plan
        .overlay_surfaces()
        .find(|surface| surface.element_id == autocomplete.ids().dropdown)
        .expect("autocomplete dropdown surface");
    println!(
        "dropdown bounds=({:.0},{:.0})-({:.0},{:.0}) options={} focused={:?}",
        dropdown.bounds.x0,
        dropdown.bounds.y0,
        dropdown.bounds.x1,
        dropdown.bounds.y1,
        autocomplete.option_count(),
        autocomplete.focused_key()
    );

    for event in [
        KeyboardEvent::key_down(Key::Named(NamedKey::ArrowDown), Code::ArrowDown),
        KeyboardEvent::key_down(Key::Named(NamedKey::Enter), Code::Enter),
    ] {
        let outcome = autocomplete.handle_keyboard_event(&mut ui, &event);
        println!("autocomplete outcome={outcome:?}");
        if !outcome.consumed {
            let batch = ui.handle_keyboard_event(&event);
            print_batch(&batch);
        }
    }

    println!(
        "accepted query={:?}",
        ui.text_buffer(autocomplete.ids().input)
    );
}

fn print_batch(batch: &overstory::InteractionBatch) {
    if batch.is_empty() {
        println!("interactions=[]");
        return;
    }
    let events: Vec<_> = batch
        .events()
        .iter()
        .map(|event| match event {
            Interaction::HoverEntered(id) => format!("HoverEntered({id:?})"),
            Interaction::HoverLeft(id) => format!("HoverLeft({id:?})"),
            Interaction::PressStarted(id) => format!("PressStarted({id:?})"),
            Interaction::PressEnded(id) => format!("PressEnded({id:?})"),
            Interaction::Clicked(id) => format!("Clicked({id:?})"),
            Interaction::Scrolled(id) => format!("Scrolled({id:?})"),
            Interaction::Submitted(id) => format!("Submitted({id:?})"),
            Interaction::FocusChanged(id) => format!("FocusChanged({id:?})"),
        })
        .collect();
    println!("interactions={events:?}");
}
