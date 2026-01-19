// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Understory Imaging: backend-agnostic imaging IR and backend traits.
//!
//! This crate defines a small, plain‑old‑data (POD) friendly imaging
//! intermediate representation and traits for backends that consume it.
//! It sits between higher-level presentation / display layers and concrete
//! renderers (Vello CPU / Hybrid / Classic, Skia, etc.).
//!
//! # Position in the stack
//!
//! Conceptually there are three layers:
//!
//! - **Presentation / display**: box trees, layout, styling, timelines,
//!   and interaction. This lives in other `understory_*` crates.
//! - **Imaging IR (this crate)**: paths, paints, images, and pictures
//!   expressed as POD state + draw operations, plus resource and backend
//!   traits.
//! - **Backends**: concrete renderers such as Vello CPU/Hybrid/Classic
//!   or Skia that implement [`ImagingBackend`] on top of wgpu, a CPU
//!   rasterizer, or other technology.
//!
//! # Core concepts
//!
//! - **Resources**: small, opaque handles ([`PathId`], [`ImageId`],
//!   [`PaintId`], [`PictureId`]) whose lifetimes are managed
//!   via [`ResourceBackend`].
//! - **Imaging operations**: [`StateOp`] (mutate state) and [`DrawOp`]
//!   (produce pixels), combined into [`ImagingOp`] for recording.
//! - **Backends**: [`ImagingBackend`] accepts imaging ops; helpers
//!   [`record_ops`] and [`record_picture`] turn short sequences into
//!   reusable recordings and picture resources.
//! - **Transform classes**: [`TransformClass`] and [`transform_diff_class`]
//!   provide a conservative language for deciding when cached results or
//!   recordings remain valid under a new transform.
//!
//! The current API is intentionally minimal and experimental. Caching,
//! recordings, and advanced backend semantics are expected to evolve as
//! we integrate real backends; expect breaking changes while the design
//! is still being iterated.
//!
//! # Recordings and resource environments
//!
//! In the v1 model, recordings are conceptually just sequences of [`ImagingOp`]
//! that reference external resources by handle ([`PathId`], [`ImageId`],
//! [`PaintId`], [`PictureId`]). A recording is therefore *bound to the resource
//! environment* in which it was produced:
//!
//! - The same resource IDs must exist and refer to compatible resources when
//!   the recording is replayed.
//! - Recordings do not embed inline resource data and cannot reconstruct
//!   missing resources on their own.
//!
//! This keeps the implementation simple and efficient for v1 and matches
//! existing renderer architectures. Future versions may experiment with
//! inline resource references, recording-local resource tables, or ephemeral
//! arenas, but those are intentionally out of scope here.
//!
//! # Example
//!
//! A minimal sketch of how a backend might be used looks like:
//!
//! ```ignore
//! # use understory_imaging::*;
//! # use peniko::{Brush, Color};
//! # struct MyBackend { /* implements ResourceBackend + ImagingBackend */ }
//! # impl ResourceBackend for MyBackend { /* ... */ }
//! # impl ImagingBackend for MyBackend { /* ... */ }
//! let mut backend = MyBackend { /* ... */ };
//!
//! let paint = backend.create_paint(PaintDesc {
//!     brush: Brush::Solid(Color::WHITE),
//! });
//! let path = backend.create_path(PathDesc {
//!     commands: Box::new([PathCmd::MoveTo { x: 0.0, y: 0.0 }]),
//! });
//!
//! backend.state(StateOp::SetPaint(paint));
//! backend.draw(DrawOp::FillPath(path));
//!
//! // Optionally capture a reusable recording:
//! let recording = record_ops(&mut backend, |b| {
//!     b.draw(DrawOp::StrokePath(path));
//! });
//! assert!(recording.ops.len() > 0);
//! ```
//!
//! For full design notes and background, see the `issue_understory_imaging.md`
//! RFC in the `docs/` directory of the Understory repository.

#![no_std]

extern crate alloc;

use alloc::{boxed::Box, sync::Arc};
use core::any::Any;
use peniko::Brush;
pub use peniko::{BlendMode, Fill as FillRule, ImageAlphaType, ImageFormat, ImageSampler};

/// Identifier for a path resource.
///
/// This is a small, opaque handle that is stable for the lifetime of the
/// resource. Paths are expected to be reused across frames and inside
/// recordings while they remain alive.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathId(pub u32);

/// Identifier for an image resource.
///
/// This is a small, opaque handle that is stable for the lifetime of the
/// resource. Images are typically created once and reused across frames and
/// recordings until explicitly destroyed.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u32);

/// Identifier for a paint resource.
///
/// This is a small, opaque handle that is stable for the lifetime of the
/// resource. Paints may be shared by many paths, images, and pictures.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaintId(pub u32);

/// Identifier for a picture resource (nested imaging program).
///
/// Pictures are created from [`RecordedOps`] and behave like reusable
/// sub-programs in the imaging IR. The ID is stable for the lifetime of the
/// picture.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PictureId(pub u32);

/// Description of a filter.
///
/// Filters are intended to be applied as a layer effect (see [`LayerOp::filter`]).
/// This API is experimental and the variant set is expected to grow.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterDesc {
    /// Fill the output with a solid color (aka `feFlood`).
    ///
    /// This ignores the layer’s source content. It is still affected by the layer’s clip.
    Flood {
        /// Flood color.
        color: peniko::Color,
    },
    /// Gaussian blur with separate X/Y standard deviation values in user space.
    ///
    /// Backends should scale these values using the current transform when the filter is applied.
    Blur {
        /// Standard deviation along the X axis (in user space units).
        std_deviation_x: f32,
        /// Standard deviation along the Y axis (in user space units).
        std_deviation_y: f32,
    },
    /// Drop shadow under the source content.
    DropShadow {
        /// Shadow offset along the X axis (in user space units).
        dx: f32,
        /// Shadow offset along the Y axis (in user space units).
        dy: f32,
        /// Blur standard deviation along the X axis (in user space units).
        std_deviation_x: f32,
        /// Blur standard deviation along the Y axis (in user space units).
        std_deviation_y: f32,
        /// Shadow color.
        color: peniko::Color,
    },
    /// Translate the layer output by a vector (aka `feOffset`).
    ///
    /// Offsets are specified in user space; backends should transform this vector using the
    /// current linear transform when the filter is applied.
    Offset {
        /// Offset along the X axis (in user space units).
        dx: f32,
        /// Offset along the Y axis (in user space units).
        dy: f32,
    },
}

impl FilterDesc {
    /// Create a flood filter.
    #[inline]
    pub const fn flood(color: peniko::Color) -> Self {
        Self::Flood { color }
    }

    /// Create a uniform Gaussian blur filter.
    #[inline]
    pub const fn blur(sigma: f32) -> Self {
        Self::Blur {
            std_deviation_x: sigma,
            std_deviation_y: sigma,
        }
    }

    /// Create a Gaussian blur filter with separate X/Y sigma values.
    #[inline]
    pub const fn blur_xy(std_deviation_x: f32, std_deviation_y: f32) -> Self {
        Self::Blur {
            std_deviation_x,
            std_deviation_y,
        }
    }

    /// Create an offset/translation filter.
    #[inline]
    pub const fn offset(dx: f32, dy: f32) -> Self {
        Self::Offset { dx, dy }
    }
}

/// Affine transform type used by the imaging IR.
pub type Affine = kurbo::Affine;

/// A simple axis-aligned rectangle in f32 coordinates.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RectF {
    /// Minimum X coordinate.
    pub x0: f32,
    /// Minimum Y coordinate.
    pub y0: f32,
    /// Maximum X coordinate.
    pub x1: f32,
    /// Maximum Y coordinate.
    pub y1: f32,
}

impl RectF {
    /// Create a new rectangle from min/max corners.
    #[inline]
    pub const fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Convert to kurbo's rectangle type.
    #[inline]
    pub fn to_kurbo(self) -> kurbo::Rect {
        kurbo::Rect::new(
            f64::from(self.x0),
            f64::from(self.y0),
            f64::from(self.x1),
            f64::from(self.y1),
        )
    }
}

/// Corner radii for a rounded rectangle in f32 coordinates.
///
/// Radii are specified clockwise starting from the top-left corner.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RoundedRectRadiiF {
    /// The radius of the top-left corner.
    pub top_left: f32,
    /// The radius of the top-right corner.
    pub top_right: f32,
    /// The radius of the bottom-right corner.
    pub bottom_right: f32,
    /// The radius of the bottom-left corner.
    pub bottom_left: f32,
}

impl RoundedRectRadiiF {
    /// Create radii with potentially different values per corner.
    #[inline]
    pub const fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    /// Create radii with a single value for all corners.
    #[inline]
    pub const fn from_single_radius(radius: f32) -> Self {
        Self::new(radius, radius, radius, radius)
    }

    /// Convert to kurbo's rounded-rect radii type.
    #[inline]
    pub fn to_kurbo(self) -> kurbo::RoundedRectRadii {
        kurbo::RoundedRectRadii::new(
            f64::from(self.top_left),
            f64::from(self.top_right),
            f64::from(self.bottom_right),
            f64::from(self.bottom_left),
        )
    }

    /// If all radii are equal, returns the uniform radius; otherwise returns `None`.
    #[inline]
    pub fn as_single_radius(self) -> Option<f32> {
        let epsilon = 1e-6_f32;
        if (self.top_left - self.top_right).abs() < epsilon
            && (self.top_right - self.bottom_right).abs() < epsilon
            && (self.bottom_right - self.bottom_left).abs() < epsilon
        {
            Some(self.top_left)
        } else {
            None
        }
    }
}

impl From<f32> for RoundedRectRadiiF {
    #[inline]
    fn from(radius: f32) -> Self {
        Self::from_single_radius(radius)
    }
}

impl From<(f32, f32, f32, f32)> for RoundedRectRadiiF {
    #[inline]
    fn from(radii: (f32, f32, f32, f32)) -> Self {
        Self::new(radii.0, radii.1, radii.2, radii.3)
    }
}

/// An axis-aligned rounded rectangle in f32 coordinates.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RoundedRectF {
    /// The underlying axis-aligned rectangle.
    pub rect: RectF,
    /// Radii of the rounded corners.
    pub radii: RoundedRectRadiiF,
}

impl RoundedRectF {
    /// Create a new rounded rectangle from corners and radii.
    #[inline]
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32, radii: impl Into<RoundedRectRadiiF>) -> Self {
        Self {
            rect: RectF { x0, y0, x1, y1 },
            radii: radii.into(),
        }
    }

    /// Convert to kurbo's rounded-rect type.
    #[inline]
    pub fn to_kurbo(self) -> kurbo::RoundedRect {
        kurbo::RoundedRect::new(
            f64::from(self.rect.x0),
            f64::from(self.rect.y0),
            f64::from(self.rect.x1),
            f64::from(self.rect.y1),
            self.radii.to_kurbo(),
        )
    }
}

/// Clip shape used by [`LayerOp`].
///
/// Currently supports rectangular, rounded-rectangular, and path-based clips;
/// this may grow additional region types over time.
#[derive(Clone, Debug, PartialEq)]
pub enum ClipShape {
    /// Clip to an axis-aligned rectangle in local coordinates.
    Rect(RectF),
    /// Clip to an axis-aligned rounded rectangle in local coordinates.
    RoundedRect(RoundedRectF),
    /// Clip to the interior of a path resource.
    ///
    /// The path is interpreted in the same local coordinate system as other
    /// geometry, subject to the current transform.
    Path(PathId),
}

impl ClipShape {
    /// Create a rectangular clip from min/max corners.
    #[inline]
    pub fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::Rect(RectF::new(x0, y0, x1, y1))
    }

    /// Create a rounded-rect clip with the same radius on all corners.
    #[inline]
    pub fn rounded_rect(x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) -> Self {
        Self::RoundedRect(RoundedRectF::new(x0, y0, x1, y1, radius))
    }

    /// Create a rounded-rect clip with explicit per-corner radii.
    #[inline]
    pub fn rounded_rect_radii(
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        radii: impl Into<RoundedRectRadiiF>,
    ) -> Self {
        Self::RoundedRect(RoundedRectF::new(x0, y0, x1, y1, radii))
    }

    /// Create a clip from a path resource.
    #[inline]
    pub fn path(id: PathId) -> Self {
        Self::Path(id)
    }
}

/// Convert a [`ClipShape`] into a [`kurbo::BezPath`].
///
/// For [`ClipShape::Rect`] and [`ClipShape::RoundedRect`], this constructs a path from the
/// corresponding kurbo shape using `tolerance`.
///
/// For [`ClipShape::Path`], this calls `path_for_id` to retrieve the underlying path data.
#[inline]
pub fn clip_shape_to_bez_path(
    shape: &ClipShape,
    tolerance: f64,
    mut path_for_id: impl FnMut(PathId) -> Option<kurbo::BezPath>,
) -> Option<kurbo::BezPath> {
    use kurbo::Shape;

    match shape {
        ClipShape::Rect(rect) => Some(rect.to_kurbo().to_path(tolerance)),
        ClipShape::RoundedRect(rr) => Some(rr.to_kurbo().to_path(tolerance)),
        ClipShape::Path(id) => path_for_id(*id),
    }
}

/// Compute the filled outline of a stroked [`ClipShape`] for `ClipOp::Stroke`.
///
/// This is a backend-agnostic helper that turns stroke parameters (including dashes) into a
/// concrete outline path suitable for clipping.
#[inline]
pub fn stroke_outline_for_clip_shape(
    shape: &ClipShape,
    style: &StrokeStyle,
    tolerance: f64,
    mut path_for_id: impl FnMut(PathId) -> Option<kurbo::BezPath>,
) -> Option<kurbo::BezPath> {
    use kurbo::{Shape, StrokeOpts, stroke};

    let outline = match shape {
        ClipShape::Rect(rect) => stroke(
            rect.to_kurbo().path_elements(tolerance),
            style,
            &StrokeOpts::default(),
            tolerance,
        ),
        ClipShape::RoundedRect(rr) => stroke(
            rr.to_kurbo().path_elements(tolerance),
            style,
            &StrokeOpts::default(),
            tolerance,
        ),
        ClipShape::Path(id) => {
            let path = path_for_id(*id)?;
            stroke(path.iter(), style, &StrokeOpts::default(), tolerance)
        }
    };
    Some(outline)
}

/// A clipping operation attached to a pushed layer.
///
/// In the layer-only model, clipping is a property of a layer scope rather
/// than separate ambient state.
#[derive(Clone, Debug, PartialEq)]
pub enum ClipOp {
    /// Clip to the fill region of a shape with an explicit fill rule.
    Fill {
        /// Shape used to define the clip region.
        shape: ClipShape,
        /// Fill rule used for path-based clips.
        ///
        /// This is currently only meaningful for [`ClipShape::Path`].
        fill_rule: FillRule,
    },
    /// Clip to the filled outline of a stroked shape.
    ///
    /// Backends may implement this by stroking the shape into a temporary path
    /// and clipping to that fill region.
    Stroke {
        /// Shape whose stroked outline defines the clip region.
        shape: ClipShape,
        /// Stroke style used to compute the outline, including dashes.
        style: StrokeStyle,
    },
}

/// Parameters for a pushed compositing layer.
///
/// Layers are the primary scoping mechanism for clipping and compositing:
/// - `clip` optionally restricts drawing within the layer.
/// - `filter` optionally applies an image filter when compositing the layer into its parent.
/// - `blend`/`opacity` optionally control how the layer is composited into its parent.
///
/// Draw operations inside a layer use normal (`SrcOver`) compositing within that
/// layer. Per-draw blend modes are intentionally not part of the core IR.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerOp {
    /// Optional clip applied to this layer's contents.
    pub clip: Option<ClipOp>,
    /// Optional filter applied to the contents of this layer when compositing it into its parent.
    pub filter: Option<FilterDesc>,
    /// Optional blend mode used when compositing this layer into its parent.
    pub blend: Option<BlendMode>,
    /// Optional opacity (0–1) applied when compositing this layer into its parent.
    pub opacity: Option<f32>,
}

impl LayerOp {
    /// Returns true if this layer changes how its contents are composited into its parent.
    ///
    /// This is a shorthand for checking whether any of `filter`, `blend`, or `opacity` are set.
    /// It does not consider `clip`, which constrains drawing inside the layer but does not affect
    /// the final compositing operation.
    #[inline]
    pub fn has_compositing_effects(&self) -> bool {
        self.filter.is_some() || self.blend.is_some() || self.opacity.is_some()
    }

    /// Returns true if this layer has no effect at all.
    ///
    /// Backends may use this to elide pushing/popping layers when `clip`, `filter`, `blend`, and
    /// `opacity` are all `None`.
    #[inline]
    pub fn is_noop(&self) -> bool {
        self.clip.is_none() && !self.has_compositing_effects()
    }
}

/// Stroke style used by `StateOp::SetStroke`.
///
/// This is currently a re-export of [`kurbo::Stroke`], which captures width,
/// joins, caps, dashes, and related stroke parameters.
pub type StrokeStyle = kurbo::Stroke;

/// State operations that mutate the current imaging state.
#[derive(Clone, Debug, PartialEq)]
pub enum StateOp {
    /// Set the current transform matrix.
    SetTransform(Affine),
    /// Set the current paint-space transform used when sampling brushes
    /// (e.g. gradients). This is separate from the geometry transform.
    SetPaintTransform(Affine),
    /// Push a new layer onto the layer stack.
    ///
    /// Layers are the primary scoping mechanism for clipping and compositing.
    /// They must be well-nested: every `PushLayer` must eventually be matched
    /// by a [`StateOp::PopLayer`].
    PushLayer(LayerOp),
    /// Pop the most recently pushed layer.
    PopLayer,
    /// Set the current paint resource.
    SetPaint(PaintId),
    /// Set the current stroke style.
    SetStroke(StrokeStyle),
    /// Set the current fill rule used for filling and clipping paths.
    ///
    /// This affects operations that determine an “inside” region from a path,
    /// such as [`DrawOp::FillPath`]. It does not affect stroking.
    ///
    /// The default fill rule is [`FillRule::NonZero`].
    SetFillRule(FillRule),
}

/// Draw operations that produce pixels given the current state.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawOp {
    /// Fill the given path with the current paint.
    FillPath(PathId),
    /// Stroke the given path with the current stroke and paint.
    StrokePath(PathId),
    /// Fill an axis-aligned rectangle with the current paint.
    FillRect {
        /// Minimum X coordinate.
        x0: f32,
        /// Minimum Y coordinate.
        y0: f32,
        /// Maximum X coordinate.
        x1: f32,
        /// Maximum Y coordinate.
        y1: f32,
    },
    /// Stroke an axis-aligned rectangle with the current stroke and paint.
    StrokeRect {
        /// Minimum X coordinate.
        x0: f32,
        /// Minimum Y coordinate.
        y0: f32,
        /// Maximum X coordinate.
        x1: f32,
        /// Maximum Y coordinate.
        y1: f32,
    },
    /// Draw an image with an explicit transform.
    DrawImage {
        /// Image resource to draw.
        image: ImageId,
        /// Transform applied to the image.
        transform: Affine,
        /// Parameters that specify how to sample the image.
        sampler: ImageSampler,
    },
    /// Draw an image mapped to a destination rect, optionally sampling from a source rect.
    ///
    /// - `dst` is in local coordinates (subject to the current transform).
    /// - `src` is in image pixel coordinates.
    ///
    /// # Sampling and out-of-bounds behavior
    ///
    /// Sampling behavior is controlled by `sampler`:
    ///
    /// - `sampler.quality` selects the filtering mode (e.g. nearest vs bilinear).
    /// - `sampler.x_extend`/`sampler.y_extend` define what happens when sampling outside the
    ///   image bounds (pad/clamp, repeat, reflect).
    ///
    /// Backends should interpret `src` as a sampling window: pixels in `dst` map into `src`
    /// and are sampled according to `sampler`. When `src` is `None`, the full image bounds
    /// are used.
    ///
    /// # Atlas padding guidance
    ///
    /// When sampling sub-rects from an atlas with filtered sampling (Medium/High), callers
    /// should pad the atlas region by at least 1 pixel (or use inset `src` rects) to avoid
    /// sampling from neighboring sprites due to filter footprints.
    ///
    /// Backends should treat `src` as a sampling window. v0 implementations may approximate
    /// this via clipping + transform; callers should pad atlas regions to avoid bleed when
    /// using filtered sampling.
    DrawImageRect {
        /// Image resource to draw.
        image: ImageId,
        /// Optional source rectangle in image pixel coordinates.
        src: Option<RectF>,
        /// Destination rectangle in local coordinates.
        dst: RectF,
        /// Parameters that specify how to sample the image.
        sampler: ImageSampler,
    },
    /// Draw a nested picture with an explicit transform.
    DrawPicture {
        /// Picture resource to draw.
        picture: PictureId,
        /// Transform applied to the picture.
        transform: Affine,
    },
}

/// Description of a path resource.
#[derive(Clone, Debug)]
pub struct PathDesc {
    /// Command buffer describing the path geometry.
    pub commands: Box<[PathCmd]>,
}

/// Simple path command enumeration.
#[derive(Copy, Clone, Debug)]
pub enum PathCmd {
    /// Move the current point without drawing.
    MoveTo {
        /// X coordinate of the new point.
        x: f32,
        /// Y coordinate of the new point.
        y: f32,
    },
    /// Draw a line from the current point to the given point.
    LineTo {
        /// X coordinate of the line end.
        x: f32,
        /// Y coordinate of the line end.
        y: f32,
    },
    /// Draw a quadratic Bézier curve from the current point to the given
    /// point, using a single control point.
    QuadTo {
        /// X coordinate of the control point.
        x1: f32,
        /// Y coordinate of the control point.
        y1: f32,
        /// X coordinate of the curve end.
        x: f32,
        /// Y coordinate of the curve end.
        y: f32,
    },
    /// Draw a cubic Bézier curve from the current point to the given point,
    /// using two control points.
    CurveTo {
        /// X coordinate of the first control point.
        x1: f32,
        /// Y coordinate of the first control point.
        y1: f32,
        /// X coordinate of the second control point.
        x2: f32,
        /// Y coordinate of the second control point.
        y2: f32,
        /// X coordinate of the curve end.
        x: f32,
        /// Y coordinate of the curve end.
        y: f32,
    },
    /// Close the current subpath.
    Close,
}

/// Description of an image resource.
#[derive(Clone, Debug)]
pub struct ImageDesc {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Pixel format of the image buffer.
    pub format: ImageFormat,
    /// Alpha encoding of the pixels (straight vs premultiplied).
    pub alpha_type: ImageAlphaType,
}

/// Description of a paint resource.
#[derive(Clone, Debug)]
pub struct PaintDesc {
    /// Brush used when rendering (solid color, gradient, image, etc.).
    ///
    /// This is a [`peniko::Brush`], so backends can directly map it onto their
    /// native paint representation.
    pub brush: Brush,
}

/// Description of a picture resource (nested imaging program).
///
/// This is a thin wrapper around [`RecordedOps`]: a picture is effectively a
/// recording that has been installed as a reusable resource.
#[derive(Debug)]
pub struct PictureDesc {
    /// Underlying recorded imaging program and optional backend-specific
    /// acceleration.
    pub recording: RecordedOps,
}

/// Resource lifetime interface.
///
/// Backends implement this to manage their own resource storage.
///
/// Implementations are free to choose how resources are allocated and stored,
/// but they must ensure that IDs remain valid and refer to the same logical
/// resource until the corresponding `destroy_*` function is called. Any
/// [`RecordedOps`] or [`PictureId`] that reference these IDs assume they stay
/// compatible for as long as they are used.
pub trait ResourceBackend {
    /// Create a path resource.
    fn create_path(&mut self, desc: PathDesc) -> PathId;
    /// Destroy a previously created path.
    fn destroy_path(&mut self, id: PathId);

    /// Create an image resource from raw pixels.
    ///
    /// The `pixels` slice is expected to contain tightly packed, row-major
    /// image data in a backend-defined format (typically premultiplied RGBA8).
    /// Backends should document their accepted formats and any alignment
    /// requirements.
    fn create_image(&mut self, desc: ImageDesc, pixels: &[u8]) -> ImageId;
    /// Destroy a previously created image.
    fn destroy_image(&mut self, id: ImageId);

    /// Create a paint resource.
    fn create_paint(&mut self, desc: PaintDesc) -> PaintId;
    /// Destroy a previously created paint.
    fn destroy_paint(&mut self, id: PaintId);

    /// Create a picture resource.
    fn create_picture(&mut self, desc: PictureDesc) -> PictureId;
    /// Destroy a previously created picture.
    fn destroy_picture(&mut self, id: PictureId);
}

/// Unified imaging operation used by recordings and picture descriptions.
#[derive(Clone, Debug, PartialEq)]
pub enum ImagingOp {
    /// State-changing operation.
    State(StateOp),
    /// Drawing operation.
    Draw(DrawOp),
}

/// Transform class describing when cached or recorded content remains valid.
///
/// These variants form a conservative hierarchy describing increasing
/// flexibility: `Exact ⊆ TranslateOnly ⊆ Orthonormal ⊆ Affine`. A cache
/// entry that is valid under a “larger” class may be reused for any
/// compatible “smaller” transform difference.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TransformClass {
    /// Valid only when the current transform is exactly equal.
    Exact,
    /// Valid when transforms differ by a pure translation.
    TranslateOnly,
    /// Valid under orthonormal transforms (rotation/reflection, no shear, uniform scale).
    Orthonormal,
    /// Valid under any affine transform.
    Affine,
}

impl TransformClass {
    /// Returns `true` if a cache entry that is valid under `self` can be
    /// safely reused for a transform difference classified as `diff`.
    pub fn supports(self, diff: Self) -> bool {
        match self {
            Self::Exact => matches!(diff, Self::Exact),
            Self::TranslateOnly => {
                matches!(diff, Self::Exact | Self::TranslateOnly)
            }
            Self::Orthonormal => !matches!(diff, Self::Affine),
            Self::Affine => true,
        }
    }
}

/// Classify the difference between two transforms as a [`TransformClass`].
///
/// This is a conservative classifier intended for cache reuse decisions:
/// - If `current` is exactly equal to `original`, the result is `Exact`.
/// - If they differ only by translation (identical linear part), the result
///   is `TranslateOnly`.
/// - Otherwise the result is `Affine`.
///
/// At the moment this function never returns [`TransformClass::Orthonormal`];
/// future versions may refine it to detect orthonormal transforms.
pub fn transform_diff_class(original: Affine, current: Affine) -> TransformClass {
    if current == original {
        return TransformClass::Exact;
    }

    let o = original.as_coeffs();
    let c = current.as_coeffs();
    let same_linear = o[0] == c[0] && o[1] == c[1] && o[2] == c[2] && o[3] == c[3];
    if same_linear {
        TransformClass::TranslateOnly
    } else {
        TransformClass::Affine
    }
}

/// Recorded imaging operations plus optional backend-specific acceleration.
///
/// In the v1 model, recordings are *environment-bound*: all resource IDs
/// referenced by the contained [`ImagingOp`]s must resolve to compatible
/// resources in the backend that replays them.
#[derive(Debug)]
pub struct RecordedOps {
    /// Plain‑old‑data (POD) imaging operations that can be replayed on any
    /// backend that has a compatible resource environment.
    pub ops: Arc<[ImagingOp]>,
    /// Optional backend-specific acceleration data (e.g. pre-baked GPU commands).
    pub acceleration: Option<Box<dyn Any>>,
    /// Transform class under which this recording remains valid.
    pub valid_under: TransformClass,
    /// Original transform matrix used when preparing any acceleration.
    ///
    /// Backends that populate [`RecordedOps::acceleration`] are expected to set this to
    /// the current transform matrix (CTM) at the time the acceleration was
    /// created. Callers may then compare the CTM at reuse time against this
    /// value in conjunction with [`RecordedOps::valid_under`] to decide whether the
    /// acceleration remains valid or needs to be regenerated.
    pub original_ctm: Option<Affine>,
}

impl RecordedOps {
    /// Returns `true` if backend-specific acceleration can be reused for
    /// the given current transform matrix.
    ///
    /// This consults [`RecordedOps::original_ctm`] and [`RecordedOps::valid_under`] to decide whether
    /// any cached acceleration remains valid under `current_ctm`. If no
    /// acceleration is present or `original_ctm` is `None`, this returns
    /// `false`.
    pub fn can_reuse(&self, current_ctm: Affine) -> bool {
        if self.acceleration.is_none() {
            return false;
        }
        let Some(original) = self.original_ctm else {
            return false;
        };
        let diff = transform_diff_class(original, current_ctm);
        self.valid_under.supports(diff)
    }
}

/// Minimal imaging backend trait.
///
/// This is expected to grow as we add support for passes, effects, and
/// recordings. For now it exposes state/draw entry points and a basic
/// recording API.
pub trait ImagingBackend: ResourceBackend {
    /// Apply a state operation.
    ///
    /// When called inside an active recording, the operation must both be
    /// applied to the backend and appended to the recording.
    fn state(&mut self, op: StateOp);

    /// Apply a draw operation.
    fn draw(&mut self, op: DrawOp);

    /// Begin capturing subsequent imaging operations into a recording.
    ///
    /// Implementations may choose how many nested recordings they support;
    /// callers should assume at most a single active recording for v1.
    /// Operations issued after this call must continue to affect the backend
    /// normally while also being appended to the recording.
    fn begin_record(&mut self);

    /// End the current recording and return the captured operations.
    ///
    /// The returned [`RecordedOps`] is environment-bound: it assumes that
    /// the same resource IDs are valid and refer to compatible resources
    /// when the recording is replayed.
    fn end_record(&mut self) -> RecordedOps;

    /// Push a new layer onto the layer stack.
    ///
    /// This is equivalent to `self.state(StateOp::PushLayer(op))`.
    #[inline]
    fn layer_push(&mut self, op: LayerOp) {
        self.state(StateOp::PushLayer(op));
    }

    /// Pop the most recently pushed layer.
    ///
    /// This is equivalent to `self.state(StateOp::PopLayer)`.
    #[inline]
    fn layer_pop(&mut self) {
        self.state(StateOp::PopLayer);
    }

    /// Push a `FillRule::NonZero` rectangular clip layer.
    ///
    /// This is equivalent to `self.layer_push(LayerOp { clip: Some(...), blend: None, opacity: None })`.
    ///
    /// The clip scope ends when you call [`ImagingBackend::layer_pop`].
    #[inline]
    fn clip_to_rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.layer_push(LayerOp {
            clip: Some(ClipOp::Fill {
                shape: ClipShape::rect(x0, y0, x1, y1),
                fill_rule: FillRule::NonZero,
            }),
            filter: None,
            blend: None,
            opacity: None,
        });
    }

    /// Push a `FillRule::NonZero` rounded-rectangular clip layer with uniform corner radius.
    ///
    /// The clip scope ends when you call [`ImagingBackend::layer_pop`].
    #[inline]
    fn clip_to_rounded_rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) {
        self.layer_push(LayerOp {
            clip: Some(ClipOp::Fill {
                shape: ClipShape::rounded_rect(x0, y0, x1, y1, radius),
                fill_rule: FillRule::NonZero,
            }),
            filter: None,
            blend: None,
            opacity: None,
        });
    }

    /// Push a path clip layer with an explicit fill rule.
    ///
    /// The clip scope ends when you call [`ImagingBackend::layer_pop`].
    #[inline]
    fn clip_to_path(&mut self, path: PathId, fill_rule: FillRule) {
        self.layer_push(LayerOp {
            clip: Some(ClipOp::Fill {
                shape: ClipShape::path(path),
                fill_rule,
            }),
            filter: None,
            blend: None,
            opacity: None,
        });
    }

    /// Push a clip shape layer with an explicit fill rule.
    ///
    /// The clip scope ends when you call [`ImagingBackend::layer_pop`].
    #[inline]
    fn clip_to_shape(&mut self, shape: ClipShape, fill_rule: FillRule) {
        self.layer_push(LayerOp {
            clip: Some(ClipOp::Fill { shape, fill_rule }),
            filter: None,
            blend: None,
            opacity: None,
        });
    }

    /// Push a stroked clip layer.
    ///
    /// The clip scope ends when you call [`ImagingBackend::layer_pop`].
    #[inline]
    fn clip_to_stroke(&mut self, shape: ClipShape, style: StrokeStyle) {
        self.layer_push(LayerOp {
            clip: Some(ClipOp::Stroke { shape, style }),
            filter: None,
            blend: None,
            opacity: None,
        });
    }
}

/// Convenience helpers for `ImagingBackend` implementations and callers.
///
/// This is separate from [`ImagingBackend`] so that methods can accept closures and return values
/// without complicating trait object usage (`&mut dyn ImagingBackend`).
pub trait ImagingBackendExt: ImagingBackend {
    /// Run `f` inside a pushed layer, popping it afterwards.
    ///
    /// This is a convenience wrapper around [`ImagingBackend::layer_push`] /
    /// [`ImagingBackend::layer_pop`].
    ///
    /// Note: if `f` panics, the layer will not be popped.
    #[inline]
    fn with_layer<R>(&mut self, op: LayerOp, f: impl FnOnce(&mut Self) -> R) -> R {
        self.layer_push(op);
        let out = f(self);
        self.layer_pop();
        out
    }

    /// Run `f` inside a layer with the given clip, blend mode, and opacity.
    ///
    /// Note: if `f` panics, the layer will not be popped.
    #[inline]
    fn with_layer_params<R>(
        &mut self,
        clip: Option<ClipOp>,
        blend: Option<BlendMode>,
        opacity: Option<f32>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.with_layer(
            LayerOp {
                clip,
                filter: None,
                blend,
                opacity,
            },
            f,
        )
    }

    /// Run `f` inside a `FillRule::NonZero` rectangular clip layer.
    ///
    /// Note: if `f` panics, the layer will not be popped.
    #[inline]
    fn with_clip_rect<R>(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.with_layer_params(
            Some(ClipOp::Fill {
                shape: ClipShape::rect(x0, y0, x1, y1),
                fill_rule: FillRule::NonZero,
            }),
            None,
            None,
            f,
        )
    }

    /// Run `f` inside a `FillRule::NonZero` rounded-rectangular clip layer with uniform corner radius.
    ///
    /// Note: if `f` panics, the layer will not be popped.
    #[inline]
    fn with_clip_rounded_rect<R>(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        radius: f32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.with_layer_params(
            Some(ClipOp::Fill {
                shape: ClipShape::rounded_rect(x0, y0, x1, y1, radius),
                fill_rule: FillRule::NonZero,
            }),
            None,
            None,
            f,
        )
    }

    /// Run `f` inside a path clip layer with explicit fill rule.
    ///
    /// Note: if `f` panics, the layer will not be popped.
    #[inline]
    fn with_clip_path<R>(
        &mut self,
        path: PathId,
        fill_rule: FillRule,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.with_layer_params(
            Some(ClipOp::Fill {
                shape: ClipShape::path(path),
                fill_rule,
            }),
            None,
            None,
            f,
        )
    }

    /// Run `f` inside a clip shape layer with explicit fill rule.
    ///
    /// Note: if `f` panics, the layer will not be popped.
    #[inline]
    fn with_clip_shape<R>(
        &mut self,
        shape: ClipShape,
        fill_rule: FillRule,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.with_layer_params(Some(ClipOp::Fill { shape, fill_rule }), None, None, f)
    }

    /// Run `f` inside a stroked clip layer.
    ///
    /// Note: if `f` panics, the layer will not be popped.
    #[inline]
    fn with_clip_stroke<R>(
        &mut self,
        shape: ClipShape,
        style: StrokeStyle,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.with_layer_params(Some(ClipOp::Stroke { shape, style }), None, None, f)
    }

    /// Run `f` inside an opacity layer.
    #[inline]
    fn with_opacity_layer<R>(&mut self, opacity: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        self.with_layer_params(None, None, Some(opacity), f)
    }

    /// Run `f` inside a blend layer.
    #[inline]
    fn with_blend_layer<R>(&mut self, blend: BlendMode, f: impl FnOnce(&mut Self) -> R) -> R {
        self.with_layer_params(None, Some(blend), None, f)
    }

    /// Run `f` inside a filter layer.
    ///
    /// Note: if `f` panics, the layer will not be popped.
    #[inline]
    fn with_filter_layer<R>(&mut self, filter: FilterDesc, f: impl FnOnce(&mut Self) -> R) -> R {
        self.with_layer(
            LayerOp {
                clip: None,
                filter: Some(filter),
                blend: None,
                opacity: None,
            },
            f,
        )
    }

    /// Run `f` inside a layer with optional clip, blend mode, and opacity.
    #[inline]
    fn with_composite_layer<R>(
        &mut self,
        clip: Option<ClipOp>,
        blend: BlendMode,
        opacity: f32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.with_layer(
            LayerOp {
                clip,
                filter: None,
                blend: Some(blend),
                opacity: Some(opacity),
            },
            f,
        )
    }
}

impl<B: ImagingBackend + ?Sized> ImagingBackendExt for B {}

/// Record a sequence of imaging operations into a [`RecordedOps`].
///
/// This helper wraps [`ImagingBackend::begin_record`] / `end_record` and
/// ensures that any state or draw operations issued by `f` are captured
/// in a single recording while still being applied to `backend`.
///
/// The returned recording is environment-bound: any resource IDs referenced
/// by the captured operations must remain valid and compatible for as long
/// as the recording is used.
pub fn record_ops<B, F>(backend: &mut B, f: F) -> RecordedOps
where
    B: ImagingBackend + ?Sized,
    F: FnOnce(&mut B),
{
    backend.begin_record();
    f(backend);
    backend.end_record()
}

/// Record imaging operations into a new picture resource.
///
/// This helper captures the operations issued by `f` into a [`RecordedOps`]
/// and then installs them as a [`PictureDesc`] via
/// [`ResourceBackend::create_picture`], returning the allocated [`PictureId`].
///
/// All resource IDs referenced by the captured operations must remain valid
/// and compatible for as long as the returned [`PictureId`] may be drawn.
pub fn record_picture<B, F>(backend: &mut B, f: F) -> PictureId
where
    B: ImagingBackend + ?Sized,
    F: FnOnce(&mut B),
{
    let recording = record_ops(backend, f);
    backend.create_picture(PictureDesc { recording })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use peniko::Color;

    /// Trivial in-memory backend that records operations for testing.
    #[derive(Default)]
    struct RecordingBackend {
        next_path: u32,
        next_image: u32,
        next_paint: u32,
        next_picture: u32,
        ops: Vec<ImagingOp>,
        recording_start: Option<usize>,
    }

    impl ResourceBackend for RecordingBackend {
        fn create_path(&mut self, _desc: PathDesc) -> PathId {
            let id = self.next_path;
            self.next_path += 1;
            PathId(id)
        }

        fn destroy_path(&mut self, _id: PathId) {}

        fn create_image(&mut self, _desc: ImageDesc, _pixels: &[u8]) -> ImageId {
            let id = self.next_image;
            self.next_image += 1;
            ImageId(id)
        }

        fn destroy_image(&mut self, _id: ImageId) {}

        fn create_paint(&mut self, _desc: PaintDesc) -> PaintId {
            let id = self.next_paint;
            self.next_paint += 1;
            PaintId(id)
        }

        fn destroy_paint(&mut self, _id: PaintId) {}

        fn create_picture(&mut self, _desc: PictureDesc) -> PictureId {
            let id = self.next_picture;
            self.next_picture += 1;
            PictureId(id)
        }

        fn destroy_picture(&mut self, _id: PictureId) {}
    }

    impl ImagingBackend for RecordingBackend {
        fn state(&mut self, op: StateOp) {
            self.ops.push(ImagingOp::State(op));
        }

        fn draw(&mut self, op: DrawOp) {
            self.ops.push(ImagingOp::Draw(op));
        }

        fn begin_record(&mut self) {
            self.recording_start = Some(self.ops.len());
        }

        fn end_record(&mut self) -> RecordedOps {
            let start = self.recording_start.take().unwrap_or(self.ops.len());
            let slice = &self.ops[start..];
            RecordedOps {
                ops: Arc::from(slice),
                acceleration: None,
                valid_under: TransformClass::Exact,
                original_ctm: None,
            }
        }
    }

    #[test]
    fn record_basic_ops() {
        let mut backend = RecordingBackend::default();

        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::WHITE),
        });
        let path = backend.create_path(PathDesc {
            commands: vec![PathCmd::MoveTo { x: 0.0, y: 0.0 }].into_boxed_slice(),
        });

        backend.state(StateOp::SetPaint(paint));
        backend.draw(DrawOp::FillPath(path));

        assert_eq!(backend.ops.len(), 2);
    }

    #[test]
    fn record_segment_of_ops() {
        let mut backend = RecordingBackend::default();

        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::WHITE),
        });
        let path = backend.create_path(PathDesc {
            commands: vec![PathCmd::MoveTo { x: 0.0, y: 0.0 }].into_boxed_slice(),
        });

        // First op outside the recording.
        backend.state(StateOp::SetPaint(paint));

        backend.begin_record();
        backend.draw(DrawOp::FillPath(path));
        backend.draw(DrawOp::StrokePath(path));
        let recorded = backend.end_record();

        assert_eq!(backend.ops.len(), 3);
        assert_eq!(recorded.ops.len(), 2);
        matches!(recorded.valid_under, TransformClass::Exact);
    }

    #[test]
    fn clip_shape_round_trips_through_ops() {
        let mut backend = RecordingBackend::default();
        let path = backend.create_path(PathDesc {
            commands: vec![PathCmd::MoveTo { x: 0.0, y: 0.0 }].into_boxed_slice(),
        });

        backend.state(StateOp::PushLayer(LayerOp {
            clip: Some(ClipOp::Fill {
                shape: ClipShape::Path(path),
                fill_rule: FillRule::NonZero,
            }),
            filter: None,
            blend: None,
            opacity: None,
        }));

        assert_eq!(backend.ops.len(), 1);
        match backend.ops[0] {
            ImagingOp::State(StateOp::PushLayer(LayerOp {
                clip:
                    Some(ClipOp::Fill {
                        shape: ClipShape::Path(id),
                        fill_rule,
                    }),
                filter: None,
                blend: None,
                opacity: None,
            })) => {
                assert_eq!(id, path);
                assert_eq!(fill_rule, FillRule::NonZero);
            }
            ref other => panic!("expected clip state op, got {other:?}"),
        }
    }
}
