// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Benchmarks for path-aware style matching.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use understory_property::{Property, PropertyMetadataBuilder, PropertyRegistry};
use understory_style::{
    ClassId, MatchState, PseudoClassId, Selector, SelectorInputs, SelectorStep, StyleBuilder,
    StyleCascade, StyleCascadeBuilder, StyleOrigin, TargetTag, TypeTag,
};

const BUTTON: TypeTag = TypeTag(1);
const TOGGLE: TypeTag = TypeTag(2);
const ROW: TypeTag = TypeTag(3);

const CHROME: TargetTag = TargetTag(10);
const CONTENT: TargetTag = TargetTag(11);
const TRACK: TargetTag = TargetTag(12);
const THUMB: TargetTag = TargetTag(13);
const TEXT: TargetTag = TargetTag(14);

const PRIMARY: ClassId = ClassId(20);
const ODD: ClassId = ClassId(21);
const EVEN: ClassId = ClassId(22);

const HOVER: PseudoClassId = PseudoClassId(30);
const CHECKED: PseudoClassId = PseudoClassId(31);

#[derive(Clone, Debug)]
struct Subject {
    parent: Option<usize>,
    type_tag: Option<TypeTag>,
    target_tag: Option<TargetTag>,
    classes: Vec<ClassId>,
    pseudos: Vec<PseudoClassId>,
}

impl Subject {
    fn root(type_tag: TypeTag) -> Self {
        Self {
            parent: None,
            type_tag: Some(type_tag),
            target_tag: None,
            classes: Vec::new(),
            pseudos: Vec::new(),
        }
    }

    fn part(parent: usize, target_tag: TargetTag) -> Self {
        Self {
            parent: Some(parent),
            type_tag: None,
            target_tag: Some(target_tag),
            classes: Vec::new(),
            pseudos: Vec::new(),
        }
    }

    fn inputs(&self) -> SelectorInputs<'_> {
        SelectorInputs::with_target(self.type_tag, self.target_tag, &self.classes, &self.pseudos)
    }
}

fn type_selector(type_tag: TypeTag) -> SelectorStep {
    SelectorStep::type_tag(type_tag)
}

fn target_selector(target_tag: TargetTag) -> SelectorStep {
    SelectorStep::target_tag(target_tag)
}

fn path(steps: impl IntoIterator<Item = SelectorStep>) -> Selector {
    Selector::from_steps(steps)
}

fn make_cascade(color: Property<u32>) -> StyleCascade {
    StyleCascadeBuilder::new()
        .push_rules(
            StyleOrigin::Sheet,
            [
                (
                    Selector::single(type_selector(BUTTON)),
                    StyleBuilder::new().set(color, 10_u32).build(),
                ),
                (
                    path([type_selector(BUTTON), target_selector(CHROME)]),
                    StyleBuilder::new().set(color, 11_u32).build(),
                ),
                (
                    path([
                        type_selector(BUTTON),
                        target_selector(CHROME),
                        target_selector(CONTENT),
                    ]),
                    StyleBuilder::new().set(color, 12_u32).build(),
                ),
                (
                    Selector::from(SelectorStep::type_tag(BUTTON).with_class(PRIMARY)),
                    StyleBuilder::new().set(color, 13_u32).build(),
                ),
                (
                    Selector::from(SelectorStep::type_tag(BUTTON).with_pseudo(HOVER)),
                    StyleBuilder::new().set(color, 14_u32).build(),
                ),
                (
                    Selector::single(type_selector(TOGGLE)),
                    StyleBuilder::new().set(color, 20_u32).build(),
                ),
                (
                    path([type_selector(TOGGLE), target_selector(TRACK)]),
                    StyleBuilder::new().set(color, 21_u32).build(),
                ),
                (
                    path([
                        type_selector(TOGGLE),
                        target_selector(TRACK),
                        target_selector(THUMB),
                    ]),
                    StyleBuilder::new().set(color, 22_u32).build(),
                ),
                (
                    path([
                        SelectorStep::type_tag(TOGGLE).with_pseudo(CHECKED),
                        target_selector(TRACK),
                    ]),
                    StyleBuilder::new().set(color, 23_u32).build(),
                ),
                (
                    Selector::from(SelectorStep::type_tag(ROW).with_class(ODD)),
                    StyleBuilder::new().set(color, 30_u32).build(),
                ),
                (
                    Selector::from(SelectorStep::type_tag(ROW).with_class(EVEN)),
                    StyleBuilder::new().set(color, 31_u32).build(),
                ),
                (
                    path([
                        type_selector(ROW),
                        target_selector(CONTENT),
                        target_selector(TEXT),
                    ]),
                    StyleBuilder::new().set(color, 32_u32).build(),
                ),
            ],
        )
        .build()
}

fn make_buttons(count: usize, hovered: Option<usize>) -> Vec<Subject> {
    let mut subjects = Vec::with_capacity(count * 3);
    for i in 0..count {
        let root = subjects.len();
        let mut button = Subject::root(BUTTON);
        if i % 4 == 0 {
            button.classes.push(PRIMARY);
        }
        if hovered == Some(i) {
            button.pseudos.push(HOVER);
        }
        subjects.push(button);

        let chrome = subjects.len();
        subjects.push(Subject::part(root, CHROME));
        subjects.push(Subject::part(chrome, CONTENT));
    }
    subjects
}

fn make_toggles(count: usize, checked: Option<usize>) -> Vec<Subject> {
    let mut subjects = Vec::with_capacity(count * 4);
    for i in 0..count {
        let root = subjects.len();
        let mut toggle = Subject::root(TOGGLE);
        if checked == Some(i) {
            toggle.pseudos.push(CHECKED);
        }
        subjects.push(toggle);

        let track = subjects.len();
        subjects.push(Subject::part(root, TRACK));
        subjects.push(Subject::part(track, THUMB));
        subjects.push(Subject::part(root, CONTENT));
    }
    subjects
}

fn make_rows(count: usize) -> Vec<Subject> {
    let mut subjects = Vec::with_capacity(count * 3);
    for i in 0..count {
        let root = subjects.len();
        let mut row = Subject::root(ROW);
        if i % 2 == 0 {
            row.classes.push(EVEN);
        } else {
            row.classes.push(ODD);
        }
        subjects.push(row);

        let content = subjects.len();
        subjects.push(Subject::part(root, CONTENT));
        subjects.push(Subject::part(content, TEXT));
    }
    subjects
}

fn restyle(cascade: &StyleCascade, subjects: &[Subject], color: Property<u32>) -> u64 {
    let mut states = vec![MatchState::default(); subjects.len()];
    let mut sum = 0_u64;

    for (index, subject) in subjects.iter().enumerate() {
        let parent = subject
            .parent
            .map_or_else(|| cascade.root_state(), |parent| states[parent]);
        let state = cascade.enter_subject(parent, &subject.inputs());
        states[index] = state;

        if let Some(value) = cascade.get_value_ref(state, color) {
            sum = sum.wrapping_add(u64::from(*value));
        }
    }

    sum
}

fn bench_style_matching(c: &mut Criterion) {
    let mut registry = PropertyRegistry::new();
    let color = registry.register("Color", PropertyMetadataBuilder::new(0_u32).build());
    let cascade = make_cascade(color);

    let mut group = c.benchmark_group("style_matching/full_restyle");
    for (name, subjects) in [
        ("buttons_1000", make_buttons(1_000, Some(125))),
        ("toggles_1000", make_toggles(1_000, Some(125))),
        ("rows_10000", make_rows(10_000)),
    ] {
        group.throughput(Throughput::Elements(subjects.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &subjects,
            |b, subjects| {
                b.iter(|| black_box(restyle(&cascade, subjects, color)));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_style_matching);
criterion_main!(benches);
