// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Style subject walk.
//!
//! Build a tiny style cascade, walk an embedder-owned template tree, inspect
//! matched rules, and restyle one subject after owner state changes.
//!
//! Run:
//! - `cargo run -p understory_examples --example style_subject_walk`

use invalidation::Channel;
use understory_property::{PropertyMetadataBuilder, PropertyRegistry};
use understory_style::{
    MatchState, PartTag, PseudoClassId, Selector, SelectorInputs, SelectorInputsOwned,
    SelectorStep, StyleBuilder, StyleCascade, StyleCascadeBuilder, StyleOrigin, TypeTag,
    WinningStyleSource,
};

const PAINT: Channel = Channel::new(1);

const TOGGLE: TypeTag = TypeTag(1);

const TRACK: PartTag = PartTag(1);
const THUMB: PartTag = PartTag(2);

const CHECKED: PseudoClassId = PseudoClassId(1);
const HOVER: PseudoClassId = PseudoClassId(2);

fn main() {
    let mut registry = PropertyRegistry::new();
    let background = registry.register(
        "Background",
        PropertyMetadataBuilder::new(0_u32)
            .affects_channels(PAINT.into_set())
            .build(),
    );
    let radius = registry.register("Radius", PropertyMetadataBuilder::new(0_u32).build());

    let cascade = StyleCascadeBuilder::new()
        .push_rules(
            StyleOrigin::Sheet,
            [
                (
                    Selector::child(
                        SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
                        SelectorStep::part_tag(TRACK),
                    ),
                    StyleBuilder::new().set(background, 0x00ff00_u32).build(),
                ),
                (
                    Selector::builder(SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED))
                        .descendant(SelectorStep::part_tag(THUMB).with_pseudo(HOVER))
                        .build(),
                    StyleBuilder::new().set(radius, 8_u32).build(),
                ),
            ],
        )
        .build();

    let unchecked_owner = enter(
        &cascade,
        cascade.root_state(),
        "Toggle",
        &SelectorInputs::typed(TOGGLE),
    );
    let unchecked_track = enter(
        &cascade,
        unchecked_owner,
        "Toggle > track",
        &SelectorInputs::part(TRACK),
    );

    let checked = [CHECKED];
    let checked_owner = enter(
        &cascade,
        cascade.root_state(),
        "Toggle:checked",
        &SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
    );

    let restyle = cascade.restyle_subject(
        &registry,
        unchecked_track,
        checked_owner,
        &SelectorInputs::part(TRACK),
    );
    println!(
        "restyled track: changed_properties={:?} paint_changed={}",
        restyle.changed_properties().property_ids(),
        restyle.changed_channels().contains(PAINT)
    );

    if let Some(WinningStyleSource::Rule(rule)) =
        cascade.winning_source(restyle.state(), background)
    {
        println!("background came from selector {:?}", rule.selector());
    }

    let thumb_owned = SelectorInputsOwned::with_part(None, Some(THUMB), [], [HOVER, HOVER]);
    let thumb = enter(
        &cascade,
        restyle.state(),
        "Toggle > track > thumb:hover",
        &thumb_owned.as_inputs(),
    );
    println!("thumb radius={:?}", cascade.get_value_ref(thumb, radius));

    let hover = [HOVER];
    let wrong_path = [
        SelectorInputs::typed_with_pseudos(TOGGLE, &checked),
        SelectorInputs::part(TRACK),
        SelectorInputs::with_part(None, Some(THUMB), &[], &hover),
    ];
    let wrong_selector = Selector::child(
        SelectorStep::type_tag(TOGGLE),
        SelectorStep::part_tag(THUMB),
    );
    println!(
        "diagnose Toggle > thumb against full path: {:?}",
        wrong_selector.diagnose_path(&wrong_path)
    );
}

fn enter(
    cascade: &StyleCascade,
    parent: MatchState,
    label: &str,
    inputs: &SelectorInputs<'_>,
) -> MatchState {
    let state = cascade.enter_subject(parent, inputs);
    println!(
        "{label}: matched_rules={}",
        cascade.matching_rules(state).count()
    );
    state
}
