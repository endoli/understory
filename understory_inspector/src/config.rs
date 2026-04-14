// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Inspector configuration.

use understory_virtual_list::ScrollAlign;

/// Fixed-row configuration for an [`crate::Inspector`].
///
/// v0 intentionally assumes a uniform row extent so the controller can remain
/// small and predictable. Richer row sizing can be added later if real call
/// sites demand it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InspectorConfig {
    /// Extent of each visible row along the scrolling axis.
    pub row_extent: f64,
    /// Extent of the viewport along the scrolling axis.
    pub viewport_extent: f64,
    /// Extra extent realized before the viewport.
    pub overscan_before: f64,
    /// Extra extent realized after the viewport.
    pub overscan_after: f64,
    /// Alignment used when scrolling the focused row into view.
    pub scroll_align: ScrollAlign,
}

impl InspectorConfig {
    /// Creates a new fixed-row configuration.
    #[must_use]
    pub const fn new(
        row_extent: f64,
        viewport_extent: f64,
        overscan_before: f64,
        overscan_after: f64,
        scroll_align: ScrollAlign,
    ) -> Self {
        Self {
            row_extent,
            viewport_extent,
            overscan_before,
            overscan_after,
            scroll_align,
        }
    }

    /// Creates a configuration with symmetric one-row overscan and nearest-row
    /// focus scrolling.
    #[must_use]
    pub fn fixed_rows(row_extent: f64, viewport_extent: f64) -> Self {
        Self::new(
            row_extent,
            viewport_extent,
            row_extent.max(0.0),
            row_extent.max(0.0),
            ScrollAlign::Nearest,
        )
    }
}
