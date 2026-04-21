// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Surface plan — segmented visual output for compositor integration.
//!
//! The `SurfacePlan` is the semantic visual output of Overstory. It describes
//! one or more visual surfaces in painter/compositing order, each carrying
//! either framework-owned display content or a placeholder for externally
//! realized content (video, 3D, native views, etc.).
//!
//! Existing hosts that don't support layered composition can flatten all
//! display surfaces back into one scene via the compatibility helpers.
//! Compositor-backed hosts can consume surfaces directly, mapping them
//! to compositor layers.

use alloc::vec::Vec;

use kurbo::{Affine, Rect};
use understory_display::DisplayTree;

use crate::ElementId;

/// Segmented visual output from one Overstory frame.
///
/// Surfaces are in painter/compositing order, back to front.
#[derive(Debug)]
pub struct SurfacePlan {
    surfaces: Vec<SurfaceEntry>,
}

impl SurfacePlan {
    /// Creates an empty surface plan.
    pub(crate) fn new() -> Self {
        Self {
            surfaces: Vec::new(),
        }
    }

    /// Adds a surface entry to the plan.
    pub(crate) fn push(&mut self, entry: SurfaceEntry) {
        self.surfaces.push(entry);
    }

    /// Returns all surface entries in painter order.
    #[must_use]
    pub fn surfaces(&self) -> &[SurfaceEntry] {
        &self.surfaces
    }

    /// Returns the root surface, if any.
    #[must_use]
    pub fn root_surface(&self) -> Option<&SurfaceEntry> {
        self.surfaces
            .iter()
            .find(|s| matches!(s.role, SurfaceRole::Root))
    }

    /// Returns overlay surfaces (popups, tooltips, dropdowns).
    pub fn overlay_surfaces(&self) -> impl Iterator<Item = &SurfaceEntry> {
        self.surfaces.iter().filter(|s| {
            matches!(
                s.role,
                SurfaceRole::Overlay
                    | SurfaceRole::Popup
                    | SurfaceRole::Tooltip
                    | SurfaceRole::Dropdown
            )
        })
    }

    /// Returns only surfaces with framework-owned display content.
    pub fn display_surfaces(&self) -> impl Iterator<Item = &SurfaceEntry> {
        self.surfaces
            .iter()
            .filter(|s| matches!(s.content, SurfaceContent::Display(_)))
    }

    /// Flattens all display surfaces into a single `DisplayTree` for
    /// compatibility with hosts that don't support layered composition.
    ///
    /// External surfaces are skipped. Overlay surfaces are composited
    /// on top of the root surface in painter order.
    #[must_use]
    pub fn flatten_to_display_tree(&self) -> Option<(DisplayTree, Rect)> {
        // For now, return the root surface's display tree directly.
        // When overlays are added, this will composite them on top.
        let root = self.root_surface()?;
        match &root.content {
            SurfaceContent::Display(tree) => Some(((**tree).clone(), root.bounds)),
            SurfaceContent::External(_) => None,
        }
    }
}

/// One visual surface in the composited output.
#[derive(Debug)]
pub struct SurfaceEntry {
    /// Source element that owns this visual segment.
    pub element_id: ElementId,
    /// Role in the composed output.
    pub role: SurfaceRole,
    /// Surface-local to root/window transform.
    pub transform: Affine,
    /// Bounds in root/window coordinates.
    pub bounds: Rect,
    /// Optional clip in surface-local coordinates.
    pub clip: Option<Rect>,
    /// Opacity for isolated composition (1.0 = fully opaque).
    pub opacity: f32,
    /// Blend/isolation hint.
    pub blend: BlendModeHint,
    /// Optional anchor for popups/overlays.
    pub anchor: Option<SurfaceAnchor>,
    /// What this surface contains.
    pub content: SurfaceContent,
}

/// Role of a surface in the composed output.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SurfaceRole {
    /// Main application content.
    Root,
    /// Generic overlay above main content.
    Overlay,
    /// Popup menu or dialog.
    Popup,
    /// Tooltip.
    Tooltip,
    /// Dropdown/combo box list.
    Dropdown,
    /// Embedded external content slot.
    Embedded,
    /// High-frequency canvas (e.g., node graph, drawing surface).
    Canvas,
    /// Application-defined role.
    Custom,
}

/// What a surface contains.
#[derive(Debug)]
pub enum SurfaceContent {
    /// Framework-owned retained visual content.
    Display(alloc::boxed::Box<DisplayTree>),
    /// Placeholder for externally realized content.
    External(ExternalSurface),
}

/// Placeholder for externally realized content.
#[derive(Clone, Debug)]
pub struct ExternalSurface {
    /// Bounds in surface-local coordinates.
    pub bounds: Rect,
    /// Type of external realization.
    pub kind: ExternalSurfaceKind,
}

/// Classification of external content.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ExternalSurfaceKind {
    /// Video playback surface.
    Video,
    /// 3D rendering surface.
    ThreeD,
    /// Platform native view.
    NativeView,
    /// Embedded web content.
    WebView,
    /// High-frequency canvas (custom rendering loop).
    HighFrequencyCanvas,
    /// Application-defined external content.
    Custom,
}

/// Blend/isolation hint for compositor integration.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BlendModeHint {
    /// Standard source-over compositing.
    #[default]
    Normal,
    /// Isolated group (opacity/effects applied to group result).
    Isolated,
    /// Application-defined blend behavior.
    Custom,
}

/// Anchor information for overlay surfaces.
#[derive(Clone, Debug)]
pub struct SurfaceAnchor {
    /// Element that owns/triggered this overlay.
    pub owner: ElementId,
    /// Type of anchoring relationship.
    pub kind: AnchorKind,
    /// Anchor rectangle in root/window coordinates.
    pub rect_in_root: Rect,
}

/// Type of anchoring relationship for overlays.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AnchorKind {
    /// Popup menu anchor.
    Popup,
    /// Tooltip anchor.
    Tooltip,
    /// Dropdown list anchor.
    Dropdown,
    /// Application-defined anchor.
    Custom,
}
