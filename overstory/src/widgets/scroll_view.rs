// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Scroll view widget with vertical scroll offset and content tracking.

use crate::{TextServices, Widget};

/// Scrollable container widget that tracks scroll offset, content height,
/// and viewport height.
#[derive(Clone, Debug, Default)]
pub struct ScrollViewWidget {
    scroll_offset: f64,
    content_height: f64,
    viewport_height: f64,
}

impl ScrollViewWidget {
    /// Creates a new scroll view widget.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current vertical scroll offset.
    #[must_use]
    pub fn scroll_offset(&self) -> f64 {
        self.scroll_offset
    }

    /// Sets the scroll offset, clamping to valid bounds.
    pub fn set_scroll_offset(&mut self, offset: f64) {
        let max_offset = (self.content_height - self.viewport_height).max(0.0);
        self.scroll_offset = offset.clamp(0.0, max_offset);
    }

    /// Adjusts the scroll offset by a delta.
    pub fn scroll_by(&mut self, delta: f64) {
        self.set_scroll_offset(self.scroll_offset + delta);
    }

    /// Returns the measured content height from the last layout.
    #[must_use]
    pub fn content_height(&self) -> f64 {
        self.content_height
    }

    /// Returns the viewport height from the last layout.
    #[must_use]
    pub fn viewport_height(&self) -> f64 {
        self.viewport_height
    }

    /// Updates the content and viewport heights from layout metrics.
    pub(crate) fn set_layout_metrics(&mut self, content_height: f64, viewport_height: f64) {
        self.content_height = content_height;
        self.viewport_height = viewport_height;
        // Re-clamp offset in case content shrunk.
        let max_offset = (content_height - viewport_height).max(0.0);
        self.scroll_offset = self.scroll_offset.clamp(0.0, max_offset);
    }
}

impl Widget for ScrollViewWidget {
    fn default_pickable(&self) -> bool {
        true
    }

    fn refresh_layout(&mut self, _text: &mut TextServices<'_>) {}

    crate::impl_widget_any!();
}
