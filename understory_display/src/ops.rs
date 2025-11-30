// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Core display operations and headers.

use kurbo::{Point, Rect};

use crate::{
    BlendMode, ClipId, GroupId, ImageId, PaintId, PathId, RunId, SemanticRegionId, StrokeId,
};

/// Common header for all ops.
///
/// `bounds` is a conservative world-space bounding box used for damage culling.
#[derive(Copy, Clone, Debug)]
pub struct OpHeader {
    /// Stable identifier for this operation within a [`crate::DisplayList`].
    pub id: crate::OpId,
    /// Logical stacking group for this op.
    pub group: GroupId,
    /// Z-order within a group; higher values are drawn later.
    pub z: i32,
    /// Optional semantic region associated with this op, used for a11y/provenance.
    pub semantic: Option<SemanticRegionId>,
    /// Conservative world-space bounds for damage culling.
    pub bounds: Rect,
}

/// A single render operation in the display list.
#[derive(Clone, Debug)]
pub enum Op {
    /// Fill a path with a paint.
    FillPath {
        /// Common header metadata.
        header: OpHeader,
        /// Path to fill.
        path: PathId,
        /// Paint to use for the fill.
        paint: PaintId,
    },
    /// Stroke a path with a stroke style and paint.
    StrokePath {
        /// Common header metadata.
        header: OpHeader,
        /// Path to stroke.
        path: PathId,
        /// Stroke style (width, joins, caps, dashes).
        stroke: StrokeId,
        /// Paint to use for the stroke.
        paint: PaintId,
    },
    /// Draw a positioned glyph run.
    GlyphRun {
        /// Common header metadata.
        header: OpHeader,
        /// Identifier for the glyph run in a text system.
        run: RunId,
        /// Origin of the run in world coordinates.
        origin: Point,
        /// Paint to use for the glyphs.
        paint: PaintId,
    },
    /// Draw an image into a destination rectangle.
    Image {
        /// Common header metadata.
        header: OpHeader,
        /// Identifier for the image resource.
        image: ImageId,
        /// Destination rectangle for the image.
        dest: Rect,
        /// Optional overall opacity multiplier in [0, 1].
        opacity: f32,
    },
    /// Begin a clip stack entry.
    PushClip {
        /// Common header metadata.
        header: OpHeader,
        /// Identifier of the clip shape to push.
        clip: ClipId,
    },
    /// End a clip stack entry.
    PopClip {
        /// Common header metadata.
        header: OpHeader,
    },
    /// Begin a compositing group with optional opacity and blend mode.
    Group {
        /// Common header metadata.
        header: OpHeader,
        /// Optional opacity multiplier in [0, 1] for the group.
        opacity: f32,
        /// Blend mode used when compositing the group into its parent.
        blend: BlendMode,
    },
}

impl Op {
    /// Return this op's header.
    pub fn header(&self) -> &OpHeader {
        match self {
            Self::FillPath { header, .. }
            | Self::StrokePath { header, .. }
            | Self::GlyphRun { header, .. }
            | Self::Image { header, .. }
            | Self::PushClip { header, .. }
            | Self::PopClip { header, .. }
            | Self::Group { header, .. } => header,
        }
    }
}
