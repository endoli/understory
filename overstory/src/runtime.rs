// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runtime interaction state for the Overstory first slice.

use alloc::vec::Vec;

use understory_event_state::{click::ClickState, hover::HoverState};

use crate::ElementId;

/// High-level interactions emitted by [`crate::Ui::handle_pointer_event`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Interaction {
    /// Pointer entered an element.
    HoverEntered(ElementId),
    /// Pointer left an element.
    HoverLeft(ElementId),
    /// Primary press began on an element.
    PressStarted(ElementId),
    /// Primary press ended on an element.
    PressEnded(ElementId),
    /// Primary click completed on an element.
    Clicked(ElementId),
}

/// Batch of high-level interactions emitted during one event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InteractionBatch {
    events: Vec<Interaction>,
}

impl InteractionBatch {
    pub(crate) fn push(&mut self, interaction: Interaction) {
        self.events.push(interaction);
    }

    /// Returns `true` if the batch is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Returns the interactions in emission order.
    #[must_use]
    pub fn events(&self) -> &[Interaction] {
        &self.events
    }
}

/// Mutable runtime state for a retained Overstory UI.
#[derive(Clone, Debug)]
pub struct RuntimeState {
    pub(crate) hover: HoverState<ElementId>,
    pub(crate) clicks: ClickState<ElementId>,
    pub(crate) pressed_target: Option<ElementId>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeState {
    /// Creates empty interaction state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            hover: HoverState::new(),
            clicks: ClickState::new(),
            pressed_target: None,
        }
    }

    /// Returns the current hovered path.
    #[must_use]
    pub fn hovered_path(&self) -> &[ElementId] {
        self.hover.current_path()
    }

    /// Returns the active press target, if any.
    #[must_use]
    pub fn pressed_target(&self) -> Option<ElementId> {
        self.pressed_target
    }
}
