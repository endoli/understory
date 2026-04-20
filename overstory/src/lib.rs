// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Overstory: a retained UI/runtime layer built on top of Understory kernels.
//!
//! Overstory owns toolkit-facing retained state and runtime policy. It uses
//! Understory crates for the headless kernels:
//!
//! - [`understory_property`] for dependency-style property storage,
//! - [`understory_style`] for selector-based style and theme resolution,
//! - [`understory_box_tree`] for spatial indexing and hit testing,
//! - [`understory_responder`] for deterministic routing,
//! - [`ui_events`] for transport-agnostic input event types.
//!
//! This crate intentionally does **not** define a long-term display-list or
//! presentation-tree abstraction. The current visual snapshot is a retained,
//! toolkit-facing debug/projection layer used to pressure-test the runtime
//! shape until those Understory seam crates exist.
//!
//! ## First slice
//!
//! The initial crate is deliberately small:
//!
//! - append-only retained element tree with stable [`ElementId`]s,
//! - a built-in element vocabulary (`Root`, `Panel`, `Column`, `Button`,
//!   `Spacer`),
//! - built-in layout/visual dependency properties,
//! - a full rebuild path that resolves style, lays out elements, and projects
//!   them into an [`understory_box_tree::Tree`],
//! - a `ui-events` pointer runtime that updates hover/press state and emits
//!   high-level interactions.
//!
//! ## Non-goals
//!
//! This crate does not yet own:
//!
//! - text layout,
//! - accessibility bridges,
//! - platform event loops,
//! - a renderer-facing display list,
//! - a general widget authoring API.
//!
//! ## Example
//!
//! See `examples/overstory_showcase.rs` in the workspace examples crate.

#![no_std]

extern crate alloc;

mod color;
mod element;
mod properties;
mod runtime;
mod scene;
mod ui;

pub use color::Color;
pub use element::{
    ButtonClass, Element, ElementId, ElementKind, LayoutClass, PSEUDO_DISABLED, PSEUDO_HOVER,
    PSEUDO_PRESSED, PseudoState, TYPE_BUTTON, TYPE_COLUMN, TYPE_PANEL, TYPE_ROOT, TYPE_SPACER,
};
pub use properties::{BuiltInProperties, DirtyChannels, ThemeKeys};
pub use runtime::{Interaction, InteractionBatch, RuntimeState};
pub use scene::{BorderStyle, ResolvedElement, SceneSnapshot};
pub use ui::{Ui, default_theme};

/// Re-export the transport-agnostic event vocabulary used by Overstory.
pub use ui_events;
