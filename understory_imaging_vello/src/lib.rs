// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Vello backend for `understory_imaging`.
//!
//! This crate provides a minimal implementation of [`understory_imaging::ImagingBackend`]
//! that records imaging ops into a Vello [`vello::Scene`]. It is intended for
//! experiments and examples rather than as a final, feature-complete backend.
//!
//! The current implementation:
//! - Supports `FillPath` and `StrokePath` using `kurbo::Stroke`.
//! - Ignores paints for now and uses a solid white brush.
//! - Stores path resources as `PathDesc` and converts them to `BezPath` on draw.
//! - Treats `DrawImage` and `DrawPicture` as no-ops (TBD).

extern crate alloc;

use alloc::{sync::Arc, vec::Vec};
use core::any::Any;
use core::fmt;

use kurbo::{Affine, BezPath, Rect, RoundedRect};
use understory_imaging::{
    BlendMode, ClipShape, DrawOp, ImageDesc, ImageId, ImagingBackend, ImagingOp, PaintDesc,
    PaintId, PathCmd, PathDesc, PathId, PictureDesc, PictureId, ResourceBackend, StateOp,
    StrokeStyle, TransformClass,
};
use vello::Scene;
use vello::peniko::{Blob, Brush, Color, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat};

/// Simple Vello-backed imaging backend that draws into a Vello [`Scene`].
pub struct VelloImagingBackend<'s> {
    /// Underlying Vello scene to draw into.
    pub scene: &'s mut Scene,
    paths: Vec<Option<PathDesc>>,
    images: Vec<Option<(ImageDesc, Vec<u8>)>>,
    paints: Vec<Option<PaintDesc>>,
    pictures: Vec<Option<PictureDesc>>,

    current_transform: Affine,
    current_stroke: Option<StrokeStyle>,
    current_brush: Brush,
    current_clip: Option<ClipShape>,
    current_blend: BlendMode,
    current_opacity: f32,
    current_paint_transform: Affine,

    /// Buffered imaging ops captured between `begin_record`/`end_record`.
    recording_ops: Vec<ImagingOp>,
    /// Whether recording is currently active.
    recording_active: bool,
}

impl fmt::Debug for VelloImagingBackend<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VelloImagingBackend { .. }")
    }
}

impl<'s> VelloImagingBackend<'s> {
    /// Create a new backend that renders into the given scene.
    pub fn new(scene: &'s mut Scene) -> Self {
        Self {
            scene,
            paths: Vec::new(),
            images: Vec::new(),
            paints: Vec::new(),
            pictures: Vec::new(),
            current_transform: Affine::IDENTITY,
            current_stroke: None,
            current_brush: Brush::Solid(Color::WHITE),
            current_clip: None,
            current_blend: BlendMode::default(),
            current_opacity: 1.0,
            current_paint_transform: Affine::IDENTITY,
            recording_ops: Vec::new(),
            recording_active: false,
        }
    }

    fn path_to_bez(&self, id: PathId) -> Option<BezPath> {
        let idx = id.0 as usize;
        let desc = self.paths.get(idx)?.as_ref()?;
        let mut p = BezPath::new();
        for cmd in desc.commands.iter() {
            match *cmd {
                PathCmd::MoveTo { x, y } => p.move_to((x as f64, y as f64)),
                PathCmd::LineTo { x, y } => p.line_to((x as f64, y as f64)),
                PathCmd::QuadTo { x1, y1, x, y } => {
                    p.quad_to((x1 as f64, y1 as f64), (x as f64, y as f64));
                }
                PathCmd::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => p.curve_to(
                    (x1 as f64, y1 as f64),
                    (x2 as f64, y2 as f64),
                    (x as f64, y as f64),
                ),
                PathCmd::Close => p.close_path(),
            }
        }
        Some(p)
    }
}

impl ResourceBackend for VelloImagingBackend<'_> {
    fn create_path(&mut self, desc: PathDesc) -> PathId {
        let id = u32::try_from(self.paths.len())
            .expect("VelloImagingBackend: too many paths for u32 PathId");
        self.paths.push(Some(desc));
        PathId(id)
    }

    fn destroy_path(&mut self, id: PathId) {
        let idx = id.0 as usize;
        if let Some(slot) = self.paths.get_mut(idx) {
            *slot = None;
        }
    }

    fn create_image(&mut self, desc: ImageDesc, pixels: &[u8]) -> ImageId {
        let id = u32::try_from(self.images.len())
            .expect("VelloImagingBackend: too many images for u32 ImageId");
        self.images.push(Some((desc, pixels.to_vec())));
        ImageId(id)
    }

    fn destroy_image(&mut self, id: ImageId) {
        let idx = id.0 as usize;
        if let Some(slot) = self.images.get_mut(idx) {
            *slot = None;
        }
    }

    fn create_paint(&mut self, desc: PaintDesc) -> PaintId {
        let id = u32::try_from(self.paints.len())
            .expect("VelloImagingBackend: too many paints for u32 PaintId");
        self.paints.push(Some(desc));
        PaintId(id)
    }

    fn destroy_paint(&mut self, id: PaintId) {
        let idx = id.0 as usize;
        if let Some(slot) = self.paints.get_mut(idx) {
            *slot = None;
        }
    }

    fn create_picture(&mut self, desc: PictureDesc) -> PictureId {
        let id = u32::try_from(self.pictures.len())
            .expect("VelloImagingBackend: too many pictures for u32 PictureId");
        self.pictures.push(Some(desc));
        PictureId(id)
    }

    fn destroy_picture(&mut self, id: PictureId) {
        let idx = id.0 as usize;
        if let Some(slot) = self.pictures.get_mut(idx) {
            *slot = None;
        }
    }
}

impl ImagingBackend for VelloImagingBackend<'_> {
    fn state(&mut self, op: StateOp) {
        if self.recording_active {
            self.recording_ops.push(ImagingOp::State(op.clone()));
        }
        match op {
            StateOp::SetTransform(xf) => {
                self.current_transform = xf;
            }
            StateOp::SetPaintTransform(xf) => {
                self.current_paint_transform = xf;
            }
            StateOp::SetStroke(style) => {
                self.current_stroke = Some(style);
            }
            StateOp::SetClip(shape) => {
                // Map imaging clip shapes into Vello clip layers. For now we
                // support a single active clip region at a time: setting a new
                // clip replaces any previous clip, and `ClipShape::Infinite`
                // clears the clip.
                //
                // Clip geometry is interpreted in the same local coordinate
                // system as other shapes, subject to the current transform.
                if self.current_clip.is_some() {
                    self.scene.pop_layer();
                    self.current_clip = None;
                }

                match shape {
                    ClipShape::Infinite => {
                        // No clip: draw into the scene directly.
                    }
                    ClipShape::Rect { x0, y0, x1, y1 } => {
                        let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                        self.scene.push_clip_layer(self.current_transform, &rect);
                        self.current_clip = Some(shape);
                    }
                    ClipShape::RoundedRect {
                        x0,
                        y0,
                        x1,
                        y1,
                        radius_x,
                        radius_y,
                    } => {
                        let rr = RoundedRect::new(
                            x0 as f64,
                            y0 as f64,
                            x1 as f64,
                            y1 as f64,
                            f64::from(radius_x.min(radius_y)),
                        );
                        self.scene.push_clip_layer(self.current_transform, &rr);
                        self.current_clip = Some(shape);
                    }
                    ClipShape::Path(id) => {
                        if let Some(path) = self.path_to_bez(id) {
                            self.scene.push_clip_layer(self.current_transform, &path);
                            self.current_clip = Some(ClipShape::Path(id));
                        }
                    }
                }
            }
            StateOp::SetPaint(id) => {
                if let Some(Some(desc)) = self.paints.get(id.0 as usize) {
                    self.current_brush = desc.brush.clone();
                }
            }
            StateOp::SetBlendMode(mode) => {
                self.current_blend = mode;
            }
            StateOp::SetOpacity(value) => {
                self.current_opacity = value;
            }
            StateOp::BeginGroup { blend, opacity } => {
                // Begin a compositing group. We currently approximate this
                // using a Vello layer with a large clip region so that group
                // blending is applied without unintentionally trimming
                // geometry.
                let big = Rect::new(-10_000.0, -10_000.0, 10_000.0, 10_000.0);
                self.scene
                    .push_layer(blend, opacity.clamp(0.0, 1.0), Affine::IDENTITY, &big);
            }
            StateOp::EndGroup => {
                self.scene.pop_layer();
            }
        }
    }

    fn draw(&mut self, op: DrawOp) {
        if self.recording_active {
            self.recording_ops.push(ImagingOp::Draw(op.clone()));
        }
        // Adjust a brush to account for an additional opacity factor.
        fn brush_with_opacity(brush: &Brush, alpha: f32) -> Brush {
            let a = alpha.clamp(0.0, 1.0);
            if (a - 1.0).abs() < f32::EPSILON {
                brush.clone()
            } else {
                brush.clone().multiply_alpha(a)
            }
        }

        match op {
            DrawOp::FillPath(id) => {
                if let Some(shape) = self.path_to_bez(id) {
                    let brush = brush_with_opacity(&self.current_brush, self.current_opacity);
                    // If blend mode and opacity are both default, draw directly.
                    if self.current_blend == BlendMode::default()
                        && (self.current_opacity - 1.0).abs() < f32::EPSILON
                    {
                        self.scene.fill(
                            Fill::NonZero,
                            self.current_transform,
                            &brush,
                            Some(self.current_paint_transform),
                            &shape,
                        );
                    } else {
                        // Otherwise, wrap the draw in a layer so the blend
                        // mode is honored. Use a large rect as the clip so
                        // we don't unintentionally trim geometry.
                        let big = Rect::new(-10_000.0, -10_000.0, 10_000.0, 10_000.0);
                        self.scene
                            .push_layer(self.current_blend, 1.0, Affine::IDENTITY, &big);
                        self.scene.fill(
                            Fill::NonZero,
                            self.current_transform,
                            &brush,
                            Some(self.current_paint_transform),
                            &shape,
                        );
                        self.scene.pop_layer();
                    }
                }
            }
            DrawOp::FillRect { x0, y0, x1, y1 } => {
                let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                let brush = brush_with_opacity(&self.current_brush, self.current_opacity);
                if self.current_blend == BlendMode::default()
                    && (self.current_opacity - 1.0).abs() < f32::EPSILON
                {
                    self.scene.fill(
                        Fill::NonZero,
                        self.current_transform,
                        &brush,
                        Some(self.current_paint_transform),
                        &rect,
                    );
                } else {
                    let big = Rect::new(-10_000.0, -10_000.0, 10_000.0, 10_000.0);
                    self.scene
                        .push_layer(self.current_blend, 1.0, Affine::IDENTITY, &big);
                    self.scene.fill(
                        Fill::NonZero,
                        self.current_transform,
                        &brush,
                        Some(self.current_paint_transform),
                        &rect,
                    );
                    self.scene.pop_layer();
                }
            }
            DrawOp::StrokePath(id) => {
                if let (Some(shape), Some(stroke)) =
                    (self.path_to_bez(id), self.current_stroke.clone())
                {
                    let brush = brush_with_opacity(&self.current_brush, self.current_opacity);
                    if self.current_blend == BlendMode::default()
                        && (self.current_opacity - 1.0).abs() < f32::EPSILON
                    {
                        self.scene.stroke(
                            &stroke,
                            self.current_transform,
                            &brush,
                            Some(self.current_paint_transform),
                            &shape,
                        );
                    } else {
                        let big = Rect::new(-10_000.0, -10_000.0, 10_000.0, 10_000.0);
                        self.scene
                            .push_layer(self.current_blend, 1.0, Affine::IDENTITY, &big);
                        self.scene.stroke(
                            &stroke,
                            self.current_transform,
                            &brush,
                            Some(self.current_paint_transform),
                            &shape,
                        );
                        self.scene.pop_layer();
                    }
                }
            }
            DrawOp::StrokeRect { x0, y0, x1, y1 } => {
                if let Some(stroke) = self.current_stroke.clone() {
                    let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                    let brush = brush_with_opacity(&self.current_brush, self.current_opacity);
                    if self.current_blend == BlendMode::default()
                        && (self.current_opacity - 1.0).abs() < f32::EPSILON
                    {
                        self.scene.stroke(
                            &stroke,
                            self.current_transform,
                            &brush,
                            Some(self.current_paint_transform),
                            &rect,
                        );
                    } else {
                        let big = Rect::new(-10_000.0, -10_000.0, 10_000.0, 10_000.0);
                        self.scene
                            .push_layer(self.current_blend, 1.0, Affine::IDENTITY, &big);
                        self.scene.stroke(
                            &stroke,
                            self.current_transform,
                            &brush,
                            Some(self.current_paint_transform),
                            &rect,
                        );
                        self.scene.pop_layer();
                    }
                }
            }
            DrawOp::DrawImage { image, transform } => {
                let idx = image.0 as usize;
                if let Some(Some((desc, pixels))) = self.images.get(idx) {
                    // Assume RGBA8 straight-alpha pixels for now; callers are
                    // responsible for packing accordingly.
                    let data: Blob<u8> = Blob::from(pixels.clone());
                    let image = ImageData {
                        data,
                        format: ImageFormat::Rgba8,
                        alpha_type: ImageAlphaType::Alpha,
                        width: desc.width,
                        height: desc.height,
                    };
                    let brush = ImageBrush::new(image);
                    // For now, opacity and blend mode are not applied to
                    // images; callers are expected to bake alpha into the
                    // pixel data.
                    self.scene.draw_image(&brush, transform);
                }
            }
            DrawOp::DrawPicture { picture, transform } => {
                let idx = picture.0 as usize;
                if let Some(Some(desc)) = self.pictures.get_mut(idx) {
                    // Preferred path: use a cached picture-local Vello Scene
                    // built at record-time, then apply the outer transform
                    // when appending.
                    if let Some(accel_any) = desc.recording.acceleration.as_ref()
                        && desc.recording.can_reuse(transform)
                        && let Some(picture_scene) = accel_any.downcast_ref::<Scene>()
                    {
                        self.scene.append(picture_scene, Some(transform));
                        return;
                    }

                    // Fallback: no usable acceleration (e.g., recording was
                    // created by another backend). Replay the IR directly into
                    // this backend, applying the outer transform to any
                    // SetTransform ops.
                    let saved_transform = self.current_transform;
                    let saved_stroke = self.current_stroke.clone();
                    let saved_brush = self.current_brush.clone();
                    let saved_clip = self.current_clip.clone();
                    let saved_blend = self.current_blend;
                    let saved_opacity = self.current_opacity;
                    let saved_paint_transform = self.current_paint_transform;

                    let ops: Vec<_> = desc.recording.ops.to_vec();
                    for op in ops {
                        match op {
                            ImagingOp::State(StateOp::SetTransform(xf)) => {
                                self.state(StateOp::SetTransform(transform * xf));
                            }
                            ImagingOp::State(s) => self.state(s),
                            ImagingOp::Draw(d) => self.draw(d),
                        }
                    }

                    self.current_transform = saved_transform;
                    self.current_stroke = saved_stroke;
                    self.current_brush = saved_brush;
                    self.current_clip = saved_clip;
                    self.current_blend = saved_blend;
                    self.current_opacity = saved_opacity;
                    self.current_paint_transform = saved_paint_transform;
                }
            }
        }
    }

    fn begin_record(&mut self) {
        self.recording_ops.clear();
        self.recording_active = true;
    }

    fn end_record(&mut self) -> understory_imaging::RecordedOps {
        self.recording_active = false;
        let slice: &[ImagingOp] = &self.recording_ops;
        let ops_arc: Arc<[ImagingOp]> = Arc::from(slice);

        // Build a picture-local Vello Scene by replaying the captured ops into
        // a fresh backend instance that draws into a separate Scene.
        let mut picture_scene = Scene::new();
        {
            let mut sub_backend = VelloImagingBackend {
                scene: &mut picture_scene,
                paths: self.paths.clone(),
                images: self.images.clone(),
                paints: self.paints.clone(),
                pictures: Vec::new(),
                current_transform: Affine::IDENTITY,
                current_stroke: None,
                current_brush: Brush::Solid(Color::WHITE),
                current_clip: None,
                current_blend: BlendMode::default(),
                current_opacity: 1.0,
                current_paint_transform: Affine::IDENTITY,
                recording_ops: Vec::new(),
                recording_active: false,
            };

            let ops_vec: Vec<_> = self.recording_ops.clone();
            for op in ops_vec {
                match op {
                    ImagingOp::State(s) => sub_backend.state(s),
                    ImagingOp::Draw(d) => sub_backend.draw(d),
                }
            }
        }

        let acceleration: Option<Box<dyn Any>> = Some(Box::new(picture_scene));

        understory_imaging::RecordedOps {
            ops: ops_arc,
            acceleration,
            // Picture-local Vello Scenes are valid under any affine transform;
            // the outer transform is applied at DrawPicture time.
            valid_under: TransformClass::Affine,
            original_ctm: Some(Affine::IDENTITY),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn basic_fill_path_renders_something() {
        let mut scene = Scene::new();
        let mut backend = VelloImagingBackend::new(&mut scene);

        let path = backend.create_path(PathDesc {
            commands: vec![
                PathCmd::MoveTo { x: 0.0, y: 0.0 },
                PathCmd::LineTo { x: 10.0, y: 0.0 },
                PathCmd::LineTo { x: 10.0, y: 10.0 },
                PathCmd::LineTo { x: 0.0, y: 10.0 },
                PathCmd::Close,
            ]
            .into_boxed_slice(),
        });

        backend.state(StateOp::SetTransform(Affine::IDENTITY));
        backend.draw(DrawOp::FillPath(path));

        let encoding = scene.encoding();
        assert!(
            !encoding.draw_tags.is_empty(),
            "expected some draw tags after FillPath"
        );
    }

    #[test]
    fn draw_picture_replays_ops() {
        let mut scene = Scene::new();
        let mut backend = VelloImagingBackend::new(&mut scene);

        // Create a simple path.
        let path = backend.create_path(PathDesc {
            commands: vec![
                PathCmd::MoveTo { x: 0.0, y: 0.0 },
                PathCmd::LineTo { x: 20.0, y: 0.0 },
                PathCmd::LineTo { x: 20.0, y: 20.0 },
                PathCmd::LineTo { x: 0.0, y: 20.0 },
                PathCmd::Close,
            ]
            .into_boxed_slice(),
        });

        // Build a tiny picture that fills this path at the origin.
        let ops = vec![
            ImagingOp::State(StateOp::SetTransform(Affine::IDENTITY)),
            ImagingOp::Draw(DrawOp::FillPath(path)),
        ]
        .into_boxed_slice();
        let picture = backend.create_picture(PictureDesc {
            recording: understory_imaging::RecordedOps {
                ops: ops.into(),
                acceleration: None,
                valid_under: TransformClass::Exact,
                original_ctm: None,
            },
        });

        // Draw the picture with a translation.
        let xf = Affine::translate((10.0, 10.0));
        backend.draw(DrawOp::DrawPicture {
            picture,
            transform: xf,
        });

        let encoding = scene.encoding();
        assert!(
            !encoding.draw_tags.is_empty(),
            "expected some draw tags after DrawPicture"
        );
    }
}
