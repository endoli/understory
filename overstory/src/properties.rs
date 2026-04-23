// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Built-in dependency properties and semantic theme tokens for Overstory.
//!
//! # Styling model
//!
//! Overstory now splits styling responsibility three ways:
//!
//! - [`ThemeKeys`] names semantic resources such as surfaces, controls, and
//!   accent colors.
//! - built-in style cascades choose which tokens apply to built-in element and
//!   widget states.
//! - app/host styles can further override properties through the normal
//!   `understory_style` cascade.
//!
//! `ThemeKeys` intentionally does **not** encode widget-state outputs such as
//! "button hover background" or "splitter active background". Those decisions
//! belong in selectors and cascades, not in the theme namespace.

use alloc::boxed::Box;

use invalidation::Channel;
use peniko::Color;
use understory_display::TextAlign;
use understory_property::{Property, PropertyMetadataBuilder, PropertyRegistry};
use understory_style::ResourceKey;

/// Coarse dirty channels used by the first Overstory slice.
#[derive(Copy, Clone, Debug, Default)]
pub struct DirtyChannels;

impl DirtyChannels {
    /// Tree structure changed.
    pub const STRUCTURE: Channel = Channel::new(0);
    /// Layout needs recomputation.
    pub const LAYOUT: Channel = Channel::new(1);
    /// Visual snapshot needs recomputation.
    pub const PAINT: Channel = Channel::new(2);
    /// Runtime interaction state changed.
    pub const INTERACTION: Channel = Channel::new(3);
}

/// Semantic theme resource keys used by the first Overstory slice.
///
/// These keys are design tokens, not widget-state-specific outputs. Built-in
/// Overstory styles map element type/class/pseudo combinations onto these
/// resources, and the active theme supplies the actual values.
#[derive(Copy, Clone, Debug, Default)]
pub struct ThemeKeys;

impl ThemeKeys {
    /// Application/workspace background behind retained surfaces.
    pub const APP_BACKGROUND: ResourceKey = ResourceKey::new(0);
    /// Default surface background for panels, inputs, and scroll containers.
    pub const SURFACE_BACKGROUND: ResourceKey = ResourceKey::new(1);
    /// Muted surface background for sidebars and secondary chrome.
    pub const SURFACE_MUTED_BACKGROUND: ResourceKey = ResourceKey::new(2);
    /// Default interactive control surface.
    pub const CONTROL_BACKGROUND: ResourceKey = ResourceKey::new(3);
    /// More emphasized control surface, suitable for hover-like states.
    pub const CONTROL_BACKGROUND_EMPHASIZED: ResourceKey = ResourceKey::new(4);
    /// Strong control surface, suitable for active/pressed states.
    pub const CONTROL_BACKGROUND_STRONG: ResourceKey = ResourceKey::new(5);
    /// Accent control surface.
    pub const ACCENT_BACKGROUND: ResourceKey = ResourceKey::new(6);
    /// More emphasized accent surface.
    pub const ACCENT_BACKGROUND_EMPHASIZED: ResourceKey = ResourceKey::new(7);
    /// Strong accent surface.
    pub const ACCENT_BACKGROUND_STRONG: ResourceKey = ResourceKey::new(8);
    /// Foreground used on accent surfaces.
    pub const ACCENT_FOREGROUND: ResourceKey = ResourceKey::new(9);
    /// Shared foreground/text color.
    pub const FOREGROUND: ResourceKey = ResourceKey::new(10);
    /// Shared border color.
    pub const BORDER_COLOR: ResourceKey = ResourceKey::new(11);
    /// Shared corner radius.
    pub const CORNER_RADIUS: ResourceKey = ResourceKey::new(12);
    /// Shared container padding.
    pub const PADDING: ResourceKey = ResourceKey::new(13);
    /// Shared vertical gap.
    pub const GAP: ResourceKey = ResourceKey::new(14);
    /// Default button height.
    pub const BUTTON_HEIGHT: ResourceKey = ResourceKey::new(15);
    /// Default font size.
    pub const FONT_SIZE: ResourceKey = ResourceKey::new(16);
    /// Default label padding.
    pub const LABEL_PADDING: ResourceKey = ResourceKey::new(17);
    /// Default font family.
    pub const FONT_FAMILY: ResourceKey = ResourceKey::new(18);
    /// Default text alignment.
    pub const TEXT_ALIGN: ResourceKey = ResourceKey::new(19);
    /// Emphasized divider fill for hover-like states.
    pub const DIVIDER_BACKGROUND_EMPHASIZED: ResourceKey = ResourceKey::new(20);
    /// Strong divider fill for active drag states.
    pub const DIVIDER_BACKGROUND_STRONG: ResourceKey = ResourceKey::new(21);
}

/// Built-in dependency properties for layout and visuals.
#[derive(Clone, Debug)]
pub struct BuiltInProperties {
    /// Explicit width; `0.0` means fill available width.
    pub width: Property<f64>,
    /// Explicit height; `0.0` means auto-size for containers.
    pub height: Property<f64>,
    /// Uniform inner padding.
    pub padding: Property<f64>,
    /// Vertical stack gap.
    pub gap: Property<f64>,
    /// Fill/background color.
    pub background: Property<Color>,
    /// Foreground/text color.
    pub foreground: Property<Color>,
    /// Border color.
    pub border_color: Property<Color>,
    /// Border width.
    pub border_width: Property<f64>,
    /// Corner radius.
    pub corner_radius: Property<f64>,
    /// Visibility toggle.
    pub visible: Property<bool>,
    /// Pickable toggle.
    pub pickable: Property<bool>,
    /// Focusable toggle.
    pub focusable: Property<bool>,
    /// Fill remaining space in the parent container.
    pub fill: Property<bool>,
    /// Font size for label text.
    pub font_size: Property<f64>,
    /// Horizontal label padding.
    pub label_padding: Property<f64>,
    /// Font family for label text.
    pub font_family: Property<Box<str>>,
    /// Text alignment for label text.
    pub text_align: Property<TextAlign>,
}

impl BuiltInProperties {
    pub(crate) fn register(registry: &mut PropertyRegistry) -> Self {
        let layout = DirtyChannels::LAYOUT.into_set();
        let paint = DirtyChannels::PAINT.into_set();
        let layout_paint = layout | paint;

        Self {
            width: registry.register(
                "Width",
                PropertyMetadataBuilder::new(0.0_f64)
                    .coerce(|value| value.max(0.0))
                    .affects_channels(layout)
                    .build(),
            ),
            height: registry.register(
                "Height",
                PropertyMetadataBuilder::new(0.0_f64)
                    .coerce(|value| value.max(0.0))
                    .affects_channels(layout)
                    .build(),
            ),
            padding: registry.register(
                "Padding",
                PropertyMetadataBuilder::new(0.0_f64)
                    .coerce(|value| value.max(0.0))
                    .affects_channels(layout)
                    .build(),
            ),
            gap: registry.register(
                "Gap",
                PropertyMetadataBuilder::new(0.0_f64)
                    .coerce(|value| value.max(0.0))
                    .affects_channels(layout)
                    .build(),
            ),
            background: registry.register(
                "Background",
                PropertyMetadataBuilder::new(Color::TRANSPARENT)
                    .affects_channels(paint)
                    .build(),
            ),
            foreground: registry.register(
                "Foreground",
                PropertyMetadataBuilder::new(Color::BLACK)
                    .inherits(true)
                    .affects_channels(paint)
                    .build(),
            ),
            border_color: registry.register(
                "BorderColor",
                PropertyMetadataBuilder::new(Color::TRANSPARENT)
                    .affects_channels(paint)
                    .build(),
            ),
            border_width: registry.register(
                "BorderWidth",
                PropertyMetadataBuilder::new(0.0_f64)
                    .coerce(|value| value.max(0.0))
                    .affects_channels(layout_paint)
                    .build(),
            ),
            corner_radius: registry.register(
                "CornerRadius",
                PropertyMetadataBuilder::new(0.0_f64)
                    .coerce(|value| value.max(0.0))
                    .affects_channels(paint)
                    .build(),
            ),
            visible: registry.register(
                "Visible",
                PropertyMetadataBuilder::new(true)
                    .affects_channels(layout_paint)
                    .build(),
            ),
            pickable: registry.register(
                "Pickable",
                PropertyMetadataBuilder::new(false)
                    .affects_channels(DirtyChannels::INTERACTION.into_set())
                    .build(),
            ),
            focusable: registry.register(
                "Focusable",
                PropertyMetadataBuilder::new(false)
                    .affects_channels(DirtyChannels::INTERACTION.into_set())
                    .build(),
            ),
            fill: registry.register(
                "Fill",
                PropertyMetadataBuilder::new(false)
                    .affects_channels(layout)
                    .build(),
            ),
            font_size: registry.register(
                "FontSize",
                PropertyMetadataBuilder::new(0.0_f64)
                    .coerce(|value| value.max(0.0))
                    .inherits(true)
                    .affects_channels(layout_paint)
                    .build(),
            ),
            label_padding: registry.register(
                "LabelPadding",
                PropertyMetadataBuilder::new(0.0_f64)
                    .coerce(|value| value.max(0.0))
                    .affects_channels(layout)
                    .build(),
            ),
            font_family: registry.register(
                "FontFamily",
                PropertyMetadataBuilder::new(Box::<str>::from(""))
                    .inherits(true)
                    .affects_channels(layout_paint)
                    .build(),
            ),
            text_align: registry.register(
                "TextAlign",
                PropertyMetadataBuilder::new(TextAlign::Start)
                    .inherits(true)
                    .affects_channels(layout_paint)
                    .build(),
            ),
        }
    }
}
