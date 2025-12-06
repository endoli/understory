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
//!   [`PaintId`], [`PictureId`], [`FilterId`]) whose lifetimes are managed
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
pub use peniko::BlendMode;
use peniko::Brush;

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

/// Identifier for a filter resource.
///
/// Filters represent reusable image processing operations. The exact schema
/// is backend-defined in v1; the ID is stable for the lifetime of the filter.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FilterId(pub u32);

/// Affine transform type used by the imaging IR.
pub type Affine = kurbo::Affine;

/// Clip shape used by `StateOp::SetClip`.
///
/// Currently supports infinite, rectangular, rounded-rectangular, and
/// path-based clips; this may grow additional region types over time.
#[derive(Clone, Debug, PartialEq)]
pub enum ClipShape {
    /// Clip to an infinite region (equivalent to no clip).
    Infinite,
    /// Clip to an axis-aligned rectangle in local coordinates.
    Rect {
        /// Minimum X coordinate.
        x0: f32,
        /// Minimum Y coordinate.
        y0: f32,
        /// Maximum X coordinate.
        x1: f32,
        /// Maximum Y coordinate.
        y1: f32,
    },
    /// Clip to a rounded rectangle in local coordinates.
    ///
    /// Radii are specified as separate X/Y components and are applied
    /// uniformly to all four corners. Backends may choose an appropriate
    /// approximation when their native clip primitives differ.
    RoundedRect {
        /// Minimum X coordinate.
        x0: f32,
        /// Minimum Y coordinate.
        y0: f32,
        /// Maximum X coordinate.
        x1: f32,
        /// Maximum Y coordinate.
        y1: f32,
        /// X radius of the rounded corners.
        radius_x: f32,
        /// Y radius of the rounded corners.
        radius_y: f32,
    },
    /// Clip to the interior of a path resource.
    ///
    /// The path is interpreted in the same local coordinate system as other
    /// geometry, subject to the current transform.
    Path(PathId),
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
    /// Set the current clip shape.
    SetClip(ClipShape),
    /// Set the current paint resource.
    SetPaint(PaintId),
    /// Set the current stroke style.
    SetStroke(StrokeStyle),
    /// Set the current blend mode.
    SetBlendMode(BlendMode),
    /// Set the current opacity (0–1).
    SetOpacity(f32),
    /// Begin a compositing group with the given blend mode and opacity.
    ///
    /// Backends may implement this using a layer, offscreen surface, or an
    /// equivalent grouping primitive. Groups must be well-nested: every
    /// [`BeginGroup`] must eventually be matched by an [`EndGroup`].
    /// Transform, clip, and paint state inside the group are applied
    /// normally; the `opacity` is applied when compositing the group back
    /// into its parent.
    BeginGroup {
        /// Blend mode used when compositing the group back into the scene.
        blend: BlendMode,
        /// Opacity of the group (0–1).
        opacity: f32,
    },
    /// End the current compositing group.
    EndGroup,
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
    // Future versions may add an explicit pixel format. For now, the pixel
    // encoding is backend-defined and documented by the backend.
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
    /// Backends that populate [`acceleration`] are expected to set this to
    /// the current transform matrix (CTM) at the time the acceleration was
    /// created. Callers may then compare the CTM at reuse time against this
    /// value in conjunction with [`valid_under`] to decide whether the
    /// acceleration remains valid or needs to be regenerated.
    pub original_ctm: Option<Affine>,
}

impl RecordedOps {
    /// Returns `true` if backend-specific acceleration can be reused for
    /// the given current transform matrix.
    ///
    /// This consults [`original_ctm`] and [`valid_under`] to decide whether
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
}

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
    B: ImagingBackend,
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
    B: ImagingBackend,
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

        backend.state(StateOp::SetClip(ClipShape::Path(path)));

        assert_eq!(backend.ops.len(), 1);
        match backend.ops[0] {
            ImagingOp::State(StateOp::SetClip(ClipShape::Path(id))) => {
                assert_eq!(id, path);
            }
            ref other => panic!("expected clip state op, got {other:?}"),
        }
    }
}
