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
//! This crate intentionally does **not** own a renderer or presentation system.
//! It resolves retained UI/runtime state into:
//!
//! - a debug/projection [`SceneSnapshot`], and
//! - a retained `understory_display::DisplayTree` that embedders can lay out
//!   and lower into paint backends.
//!
//! ## First slice
//!
//! The initial crate is deliberately small:
//!
//! - append-only retained element tree with stable [`ElementId`]s,
//! - a built-in element vocabulary (`Root`, `Panel`, `Row`, `Column`, `Button`,
//!   `Spacer`),
//! - built-in layout/visual dependency properties,
//! - a full rebuild path that resolves style, lays out elements, projects them
//!   into an [`understory_box_tree::Tree`], and can build a retained
//!   `understory_display::DisplayTree`,
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
//! - a renderer-facing paint backend,
//! - a general widget authoring API.
//!
//! ## Example
//!
//! See `examples/overstory_showcase.rs` in the workspace examples crate.

#![no_std]

extern crate alloc;

mod display;
mod element;
mod properties;
mod runtime;
mod scene;
mod surface;
mod ui;
mod widget;
pub mod widgets;

pub use element::{
    ButtonClass, Element, ElementId, LayoutClass, MessageClass, PSEUDO_DISABLED, PSEUDO_FOCUSED,
    PSEUDO_HOVER, PSEUDO_PRESSED, PseudoState, TYPE_BUTTON, TYPE_COLUMN, TYPE_PANEL, TYPE_ROOT,
    TYPE_ROW, TYPE_SCROLL_VIEW, TYPE_SPACER, TYPE_TEXT_BLOCK, TYPE_TEXT_INPUT, TYPE_TOOLTIP,
};
/// Re-export `peniko` so Overstory callers can use the shared color vocabulary
/// and palettes without adding another direct dependency for basic styling.
pub use peniko::{self, Color};
pub use properties::{BuiltInProperties, DirtyChannels, ThemeKeys};
pub(crate) use runtime::RuntimeState;
pub use runtime::{Interaction, InteractionBatch};
pub use scene::{BorderStyle, ResolvedElement, SceneSnapshot};
pub use surface::{
    AnchorKind, BlendModeHint, ExternalSurface, ExternalSurfaceKind, SurfaceAnchor, SurfaceContent,
    SurfaceEntry, SurfacePlan, SurfaceRole,
};
pub use ui::{Ui, default_theme};
pub use widget::{MeasureCtx, Widget, WidgetArena, WidgetHandle};

/// Re-export the transport-agnostic event vocabulary used by Overstory.
pub use ui_events;
