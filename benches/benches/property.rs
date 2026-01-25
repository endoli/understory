// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Benchmarks for `understory_property` + `understory_style`.

use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::sync::Once;
use std::{string::String, vec::Vec};

use understory_property::{
    DependencyObject, DependencyObjectExt, Property, PropertyMetadataBuilder, PropertyRegistry,
    PropertyStore,
};
use understory_style::{ResolveCx, ResourceKey, StyleBuilder, ThemeBuilder};

#[derive(Clone)]
struct Elem {
    key: u32,
    parent: Option<u32>,
    store: PropertyStore<u32>,
}

impl Elem {
    fn new(key: u32, parent: Option<u32>) -> Self {
        Self {
            key,
            parent,
            store: PropertyStore::new(key),
        }
    }
}

impl DependencyObject<u32> for Elem {
    fn property_store(&self) -> &PropertyStore<u32> {
        &self.store
    }

    fn property_store_mut(&mut self) -> &mut PropertyStore<u32> {
        &mut self.store
    }

    fn key(&self) -> u32 {
        self.key
    }

    fn parent_key(&self) -> Option<u32> {
        self.parent
    }
}

fn bench_property(c: &mut Criterion) {
    static PRINT_SIZES: Once = Once::new();
    PRINT_SIZES.call_once(|| {
        eprintln!(
            "sizes: PropertyStore<u32>={} Elem={} ErasedValue={}",
            core::mem::size_of::<PropertyStore<u32>>(),
            core::mem::size_of::<Elem>(),
            core::mem::size_of::<understory_property::ErasedValue>(),
        );
    });

    let mut registry = PropertyRegistry::new();
    let width: Property<f64> =
        registry.register("Width", PropertyMetadataBuilder::new(0.0_f64).build());
    let font_size: Property<f64> = registry.register(
        "FontSize",
        PropertyMetadataBuilder::new(12.0_f64)
            .inherits(true)
            .build(),
    );

    let style = StyleBuilder::new().set(width, 50.0).build();
    const WIDTH_RESOURCE: ResourceKey = ResourceKey::new(0);
    let theme = ThemeBuilder::new().set(WIDTH_RESOURCE, 75.0_f64).build();

    // A small inheritance chain: 0 <- 1 <- ... <- N-1
    let chain_len: u32 = 16;
    let mut nodes: Vec<Elem> = (0..chain_len)
        .map(|i| Elem::new(i, if i == 0 { None } else { Some(i - 1) }))
        .collect();
    nodes[0].store.set_local(font_size, 16.0);
    let leaf = &nodes[(chain_len - 1) as usize];

    let mut group = c.benchmark_group("property/resolve");

    group.bench_function("local", |b| {
        let mut element = Elem::new(1, None);
        element.store.set_local(width, 100.0);
        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        b.iter(|| black_box(cx.get_value(&element, width, None)))
    });

    group.bench_function("local_ref", |b| {
        let mut element = Elem::new(1, None);
        element.store.set_local(width, 100.0);
        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        b.iter(|| black_box(*cx.get_value_ref(&element, width, None)))
    });

    group.bench_function("animation", |b| {
        let mut element = Elem::new(1, None);
        element.store.set_local(width, 100.0);
        element.store.set_animation(width, 200.0);
        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        b.iter(|| black_box(cx.get_value(&element, width, None)))
    });

    group.bench_function("style", |b| {
        let element = Elem::new(1, None);
        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        b.iter(|| black_box(cx.get_value(&element, width, Some(&style))))
    });

    group.bench_function("default", |b| {
        let element = Elem::new(1, None);
        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        b.iter(|| black_box(cx.get_value(&element, width, None)))
    });

    group.bench_function(BenchmarkId::new("inherited", chain_len), |b| {
        let cx = ResolveCx::new(&registry, &theme, |key: u32| {
            nodes
                .get(key as usize)
                .map(|e| (e.property_store(), e.parent_key()))
        });
        b.iter(|| black_box(cx.get_value(leaf, font_size, None)))
    });

    group.bench_function("theme_resource", |b| {
        let element = Elem::new(1, None);
        let cx = ResolveCx::new(&registry, &theme, |_: u32| None);
        b.iter(|| black_box(cx.get_value_with_theme(&element, width, None, Some(WIDTH_RESOURCE))))
    });

    group.finish();

    let mut group = c.benchmark_group("property/resolve_string");

    let mut registry_string = PropertyRegistry::new();
    let text: Property<String> =
        registry_string.register("Text", PropertyMetadataBuilder::new(String::new()).build());
    let theme_string = ThemeBuilder::new().build();

    group.bench_function("local_clone", |b| {
        let mut element = Elem::new(1, None);
        element
            .store
            .set_local(text, "hello world hello world hello world".to_string());
        let cx = ResolveCx::new(&registry_string, &theme_string, |_: u32| None);
        b.iter(|| black_box(cx.get_value(&element, text, None)))
    });

    group.bench_function("local_ref", |b| {
        let mut element = Elem::new(1, None);
        element
            .store
            .set_local(text, "hello world hello world hello world".to_string());
        let cx = ResolveCx::new(&registry_string, &theme_string, |_: u32| None);
        b.iter(|| black_box(cx.get_value_ref(&element, text, None).len()))
    });

    group.finish();

    let mut group = c.benchmark_group("property/mutate");

    group.bench_function("set_local_notifying/f64/no_callback", |b| {
        b.iter_batched(
            || Elem::new(1, None),
            |mut element| {
                let channels = element.set_local_notifying(width, 123.0_f64, &registry);
                black_box(channels);
                black_box(element);
            },
            BatchSize::SmallInput,
        )
    });

    let mut registry_with_cb = PropertyRegistry::new();
    let width_cb: Property<f64> = registry_with_cb.register(
        "Width",
        PropertyMetadataBuilder::new(0.0_f64)
            .on_changed(|_old, _new| {})
            .build(),
    );
    group.bench_function("set_local_notifying/f64/with_callback", |b| {
        b.iter_batched(
            || Elem::new(1, None),
            |mut element| {
                let channels = element.set_local_notifying(width_cb, 123.0_f64, &registry_with_cb);
                black_box(channels);
                black_box(element);
            },
            BatchSize::SmallInput,
        )
    });

    let mut registry_string = PropertyRegistry::new();
    let text: Property<String> =
        registry_string.register("Text", PropertyMetadataBuilder::new(String::new()).build());
    group.bench_function("set_local_notifying/string", |b| {
        b.iter_batched(
            || Elem::new(1, None),
            |mut element| {
                let channels = element.set_local_notifying(
                    text,
                    String::from("hello world"),
                    &registry_string,
                );
                black_box(channels);
                black_box(element);
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_property);
criterion_main!(benches);
