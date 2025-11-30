// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Vello CPU–backed implementation of the imaging backend.
//!
//! This crate implements [`understory_imaging::ImagingBackend`] on top of
//! the sparse-strips [`vello_cpu::RenderContext`], so that the same imaging
//! IR can be rendered either via Vello Classic/GPU or via the CPU renderer.

#![deny(unsafe_code)]

extern crate alloc;

use alloc::{sync::Arc, vec::Vec};
use kurbo::{Affine, Cap, Join};
use peniko::{BlendMode, Brush, ImageAlphaType, ImageData, ImageFormat, ImageSampler};
use understory_imaging::{
    ClipShape, DrawOp, ImageDesc, ImageId, ImagingBackend, ImagingOp, PaintDesc, PaintId, PathCmd,
    PathDesc, PathId, PictureDesc, PictureId, RecordedOps, ResourceBackend, StateOp,
    TransformClass,
};
use vello_common::recording::{Recordable, Recording as CpuRecording};
use vello_cpu::kurbo::{
    Affine as CpuAffine, BezPath, Cap as CpuCap, Join as CpuJoin, Rect, Stroke,
};
use vello_cpu::{Image as CpuImage, ImageSource, RenderContext};

struct CpuRecordingAccel {
    recording: CpuRecording,
}

/// CPU-backed implementation of the imaging backend using `vello_cpu`.
pub struct VelloCpuImagingBackend<'ctx> {
    /// Underlying Vello CPU render context to draw into.
    pub ctx: &'ctx mut RenderContext,
    paths: Vec<Option<BezPath>>,
    images: Vec<Option<(ImageDesc, Vec<u8>)>>,
    paints: Vec<Option<PaintDesc>>,
    pictures: Vec<Option<PictureDesc>>,

    current_clip: Option<ClipShape>,
    current_opacity: f32,
    current_paint: Option<PaintId>,
    current_blend: BlendMode,
    /// Buffered imaging ops captured between `begin_record`/`end_record`.
    recording_ops: Vec<ImagingOp>,
    /// Whether recording is currently active.
    recording_active: bool,
}

impl core::fmt::Debug for VelloCpuImagingBackend<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("VelloCpuImagingBackend { .. }")
    }
}

impl<'ctx> VelloCpuImagingBackend<'ctx> {
    /// Create a new backend that renders into the given CPU render context.
    pub fn new(ctx: &'ctx mut RenderContext) -> Self {
        Self {
            ctx,
            paths: Vec::new(),
            images: Vec::new(),
            paints: Vec::new(),
            pictures: Vec::new(),
            current_clip: None,
            current_opacity: 1.0,
            current_paint: None,
            current_blend: BlendMode::default(),
            recording_ops: Vec::new(),
            recording_active: false,
        }
    }

    fn path_to_bez(&self, id: PathId) -> Option<BezPath> {
        let idx = id.0 as usize;
        let desc = self.paths.get(idx)?.as_ref()?;
        Some(desc.clone())
    }

    fn affine_to_cpu(xf: Affine) -> CpuAffine {
        CpuAffine::new(xf.as_coeffs())
    }

    fn apply_current_paint(&mut self) {
        let Some(id) = self.current_paint else {
            return;
        };
        let idx = id.0 as usize;
        if let Some(Some(PaintDesc { brush })) = self.paints.get(idx) {
            let brush = brush.clone().multiply_alpha(self.current_opacity);
            match brush {
                Brush::Solid(color) => {
                    self.ctx.set_paint(color);
                }
                Brush::Gradient(gradient) => {
                    self.ctx.set_paint(gradient);
                }
                Brush::Image(image_brush) => {
                    // Map peniko image brushes into vello_cpu image paints.
                    let source = ImageSource::from_peniko_image_data(&image_brush.image);
                    let image = CpuImage {
                        image: source,
                        sampler: image_brush.sampler,
                    };
                    self.ctx.set_paint(image);
                }
            }
        }
    }
}

impl ResourceBackend for VelloCpuImagingBackend<'_> {
    fn create_path(&mut self, desc: PathDesc) -> PathId {
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
        let id = u32::try_from(self.paths.len())
            .expect("VelloCpuImagingBackend: too many paths for u32 PathId");
        self.paths.push(Some(p));
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
            .expect("VelloCpuImagingBackend: too many images for u32 ImageId");
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
            .expect("VelloCpuImagingBackend: too many paints for u32 PaintId");
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
            .expect("VelloCpuImagingBackend: too many pictures for u32 PictureId");
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

impl ImagingBackend for VelloCpuImagingBackend<'_> {
    fn state(&mut self, op: StateOp) {
        if self.recording_active {
            self.recording_ops.push(ImagingOp::State(op.clone()));
        }
        match op {
            StateOp::SetTransform(xf) => {
                self.ctx.set_transform(Self::affine_to_cpu(xf));
            }
            StateOp::SetPaintTransform(xf) => {
                self.ctx.set_paint_transform(Self::affine_to_cpu(xf));
            }
            StateOp::SetClip(shape) => {
                if self.current_clip.is_some() {
                    self.ctx.pop_layer();
                    self.current_clip = None;
                }

                match shape {
                    ClipShape::Infinite => {
                        // No clip: draw directly.
                    }
                    ClipShape::Rect { x0, y0, x1, y1 } => {
                        let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                        let mut path = BezPath::new();
                        path.move_to((rect.x0, rect.y0));
                        path.line_to((rect.x1, rect.y0));
                        path.line_to((rect.x1, rect.y1));
                        path.line_to((rect.x0, rect.y1));
                        path.close_path();
                        self.ctx.push_clip_layer(&path);
                        self.current_clip = Some(shape);
                    }
                    ClipShape::RoundedRect { x0, y0, x1, y1, .. } => {
                        let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                        let mut path = BezPath::new();
                        path.move_to((rect.x0, rect.y0));
                        path.line_to((rect.x1, rect.y0));
                        path.line_to((rect.x1, rect.y1));
                        path.line_to((rect.x0, rect.y1));
                        path.close_path();
                        self.ctx.push_clip_layer(&path);
                        self.current_clip = Some(shape);
                    }
                    ClipShape::Path(id) => {
                        if let Some(path) = self.path_to_bez(id) {
                            self.ctx.push_clip_layer(&path);
                            self.current_clip = Some(ClipShape::Path(id));
                        }
                    }
                }
            }
            StateOp::SetPaint(id) => {
                self.current_paint = Some(id);
                self.apply_current_paint();
            }
            StateOp::SetStroke(style) => {
                let mut stroke = Stroke::new(style.width);
                stroke.miter_limit = style.miter_limit;
                stroke.join = match style.join {
                    Join::Bevel => CpuJoin::Bevel,
                    Join::Miter => CpuJoin::Miter,
                    Join::Round => CpuJoin::Round,
                };
                stroke.start_cap = match style.start_cap {
                    Cap::Butt => CpuCap::Butt,
                    Cap::Round => CpuCap::Round,
                    Cap::Square => CpuCap::Square,
                };
                stroke.end_cap = match style.end_cap {
                    Cap::Butt => CpuCap::Butt,
                    Cap::Round => CpuCap::Round,
                    Cap::Square => CpuCap::Square,
                };
                self.ctx.set_stroke(stroke);
            }
            StateOp::SetBlendMode(mode) => {
                self.current_blend = mode;
            }
            StateOp::SetOpacity(value) => {
                self.current_opacity = value;
                self.apply_current_paint();
            }
            StateOp::BeginGroup { blend, opacity } => {
                self.ctx.push_layer(None, Some(blend), Some(opacity), None);
            }
            StateOp::EndGroup => {
                self.ctx.pop_layer();
            }
        }
    }

    fn draw(&mut self, op: DrawOp) {
        if self.recording_active {
            self.recording_ops.push(ImagingOp::Draw(op.clone()));
        }
        match op {
            DrawOp::FillPath(id) => {
                if let Some(path) = self.path_to_bez(id) {
                    if self.current_blend == BlendMode::default() {
                        self.ctx.fill_path(&path);
                    } else {
                        self.ctx.push_blend_layer(self.current_blend);
                        self.ctx.fill_path(&path);
                        self.ctx.pop_layer();
                    }
                }
            }
            DrawOp::StrokePath(id) => {
                if let Some(path) = self.path_to_bez(id) {
                    if self.current_blend == BlendMode::default() {
                        self.ctx.stroke_path(&path);
                    } else {
                        self.ctx.push_blend_layer(self.current_blend);
                        self.ctx.stroke_path(&path);
                        self.ctx.pop_layer();
                    }
                }
            }
            DrawOp::FillRect { x0, y0, x1, y1 } => {
                let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                if self.current_blend == BlendMode::default() {
                    self.ctx.fill_rect(&rect);
                } else {
                    self.ctx.push_blend_layer(self.current_blend);
                    self.ctx.fill_rect(&rect);
                    self.ctx.pop_layer();
                }
            }
            DrawOp::StrokeRect { x0, y0, x1, y1 } => {
                let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                if self.current_blend == BlendMode::default() {
                    self.ctx.stroke_rect(&rect);
                } else {
                    self.ctx.push_blend_layer(self.current_blend);
                    self.ctx.stroke_rect(&rect);
                    self.ctx.pop_layer();
                }
            }
            DrawOp::DrawImage { image, transform } => {
                let idx = image.0 as usize;
                if let Some(Some((desc, pixels))) = self.images.get(idx) {
                    // Assume RGBA8 straight-alpha pixels for now; callers are
                    // responsible for packing accordingly.
                    let data = peniko::Blob::from(pixels.clone());
                    let image_data = ImageData {
                        data,
                        format: ImageFormat::Rgba8,
                        alpha_type: ImageAlphaType::Alpha,
                        width: desc.width,
                        height: desc.height,
                    };

                    let source = ImageSource::from_peniko_image_data(&image_data);
                    let sampler = ImageSampler::default();
                    let image_paint = CpuImage {
                        image: source,
                        sampler,
                    };

                    let saved_transform = *self.ctx.transform();
                    let saved_paint = self.ctx.paint().clone();

                    self.ctx.set_paint(image_paint);
                    let combined = saved_transform * Self::affine_to_cpu(transform);
                    self.ctx.set_transform(combined);

                    let rect = Rect::new(0.0, 0.0, f64::from(desc.width), f64::from(desc.height));

                    if self.current_blend == BlendMode::default() {
                        self.ctx.fill_rect(&rect);
                    } else {
                        self.ctx.push_blend_layer(self.current_blend);
                        self.ctx.fill_rect(&rect);
                        self.ctx.pop_layer();
                    }

                    self.ctx.set_transform(saved_transform);
                    self.ctx.set_paint(saved_paint);
                }
            }
            DrawOp::DrawPicture { picture, transform } => {
                let idx = picture.0 as usize;
                if let Some(Some(desc)) = self.pictures.get_mut(idx) {
                    let saved_transform = *self.ctx.transform();
                    let saved_paint = self.ctx.paint().clone();
                    // Try to use a cached vello_cpu Recording if one has been
                    // prepared for this picture. Recordings are picture-local,
                    // so we apply the outer transform via the context CTM.
                    let mut used_accel = false;
                    if let Some(accel_any) = desc.recording.acceleration.as_ref()
                        && let Some(accel) = accel_any.downcast_ref::<CpuRecordingAccel>()
                        && desc.recording.can_reuse(Affine::IDENTITY)
                    {
                        let combined = saved_transform * Self::affine_to_cpu(transform);
                        self.ctx.set_transform(combined);
                        self.ctx.execute_recording(&accel.recording);
                        used_accel = true;
                    }

                    if !used_accel {
                        // Fallback: replay the picture's IR directly into this
                        // backend, applying the outer transform to any
                        // SetTransform ops. This path is primarily for
                        // recordings created by other backends that don't
                        // provide vello_cpu-specific acceleration.
                        let saved_clip = self.current_clip.clone();
                        let saved_opacity = self.current_opacity;
                        let saved_paint_id = self.current_paint;
                        let saved_blend = self.current_blend;

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

                        self.current_clip = saved_clip;
                        self.current_opacity = saved_opacity;
                        self.current_paint = saved_paint_id;
                        self.current_blend = saved_blend;
                    }

                    self.ctx.set_transform(saved_transform);
                    self.ctx.set_paint(saved_paint);
                }
            }
        }
    }

    fn begin_record(&mut self) {
        self.recording_ops.clear();
        self.recording_active = true;
    }

    fn end_record(&mut self) -> RecordedOps {
        self.recording_active = false;
        let slice: &[ImagingOp] = &self.recording_ops;
        let ops_arc: Arc<[ImagingOp]> = Arc::from(slice);

        // Build a picture-local vello_cpu Recording from the captured ops.
        let mut recording = CpuRecording::new();
        let paths_snapshot = self.paths.clone();
        let paints_snapshot = self.paints.clone();
        let ops_vec: Vec<_> = self.recording_ops.clone();

        let saved_transform = *self.ctx.transform();
        let saved_paint = self.ctx.paint().clone();

        self.ctx.record(&mut recording, |rec| {
            // Local imaging state while translating the IR into vello_cpu
            // recording commands.
            let mut rec_blend_mode = BlendMode::default();
            let mut rec_opacity = 1.0_f32;
            let mut rec_clip: Option<ClipShape> = None;
            let mut rec_paint_id: Option<PaintId> = None;

            let set_paint_from_id = |rec: &mut vello_common::recording::Recorder<'_>,
                                     paints: &[Option<PaintDesc>],
                                     paint_id: PaintId,
                                     opacity: f32| {
                let idx = paint_id.0 as usize;
                if let Some(Some(PaintDesc { brush })) = paints.get(idx) {
                    let brush = brush.clone().multiply_alpha(opacity);
                    match brush {
                        Brush::Solid(color) => rec.set_paint(color),
                        Brush::Gradient(gradient) => rec.set_paint(gradient),
                        Brush::Image(image_brush) => {
                            let source = ImageSource::from_peniko_image_data(&image_brush.image);
                            let image = CpuImage {
                                image: source,
                                sampler: image_brush.sampler,
                            };
                            rec.set_paint(image);
                        }
                    }
                }
            };

            // Picture-local recordings start from identity; any SetTransform
            // ops inside the recording are interpreted in that local space.
            rec.set_transform(CpuAffine::IDENTITY);

            for op in &ops_vec {
                match op {
                    ImagingOp::State(StateOp::SetTransform(xf)) => {
                        rec.set_transform(Self::affine_to_cpu(*xf));
                    }
                    ImagingOp::State(StateOp::SetPaint(id)) => {
                        rec_paint_id = Some(*id);
                        set_paint_from_id(rec, &paints_snapshot, *id, rec_opacity);
                    }
                    ImagingOp::State(StateOp::SetPaintTransform(xf)) => {
                        rec.set_paint_transform(Self::affine_to_cpu(*xf));
                    }
                    ImagingOp::State(StateOp::SetStroke(style)) => {
                        let mut stroke = Stroke::new(style.width);
                        stroke.miter_limit = style.miter_limit;
                        stroke.join = match style.join {
                            Join::Bevel => CpuJoin::Bevel,
                            Join::Miter => CpuJoin::Miter,
                            Join::Round => CpuJoin::Round,
                        };
                        stroke.start_cap = match style.start_cap {
                            Cap::Butt => CpuCap::Butt,
                            Cap::Round => CpuCap::Round,
                            Cap::Square => CpuCap::Square,
                        };
                        stroke.end_cap = match style.end_cap {
                            Cap::Butt => CpuCap::Butt,
                            Cap::Round => CpuCap::Round,
                            Cap::Square => CpuCap::Square,
                        };
                        rec.set_stroke(stroke);
                    }
                    ImagingOp::State(StateOp::SetBlendMode(mode)) => {
                        rec_blend_mode = *mode;
                    }
                    ImagingOp::State(StateOp::SetOpacity(value)) => {
                        rec_opacity = *value;
                        if let Some(paint_id) = rec_paint_id {
                            set_paint_from_id(rec, &paints_snapshot, paint_id, rec_opacity);
                        }
                    }
                    ImagingOp::State(StateOp::BeginGroup { blend, opacity }) => {
                        rec.push_layer(None, Some(*blend), Some(*opacity), None);
                    }
                    ImagingOp::State(StateOp::EndGroup) => {
                        rec.pop_layer();
                    }
                    ImagingOp::State(StateOp::SetClip(shape)) => {
                        if rec_clip.is_some() {
                            rec.pop_layer();
                            rec_clip = None;
                        }
                        match shape {
                            ClipShape::Infinite => {
                                // No clip.
                            }
                            ClipShape::Rect { x0, y0, x1, y1 } => {
                                let rect =
                                    Rect::new(*x0 as f64, *y0 as f64, *x1 as f64, *y1 as f64);
                                let mut path = BezPath::new();
                                path.move_to((rect.x0, rect.y0));
                                path.line_to((rect.x1, rect.y0));
                                path.line_to((rect.x1, rect.y1));
                                path.line_to((rect.x0, rect.y1));
                                path.close_path();
                                rec.push_clip_layer(&path);
                                rec_clip = Some(shape.clone());
                            }
                            ClipShape::RoundedRect { x0, y0, x1, y1, .. } => {
                                let rect =
                                    Rect::new(*x0 as f64, *y0 as f64, *x1 as f64, *y1 as f64);
                                let mut path = BezPath::new();
                                path.move_to((rect.x0, rect.y0));
                                path.line_to((rect.x1, rect.y0));
                                path.line_to((rect.x1, rect.y1));
                                path.line_to((rect.x0, rect.y1));
                                path.close_path();
                                rec.push_clip_layer(&path);
                                rec_clip = Some(shape.clone());
                            }
                            ClipShape::Path(id) => {
                                let idx = id.0 as usize;
                                if let Some(Some(path)) = paths_snapshot.get(idx) {
                                    rec.push_clip_layer(path);
                                    rec_clip = Some(shape.clone());
                                }
                            }
                        }
                    }
                    ImagingOp::Draw(DrawOp::FillPath(id)) => {
                        let idx = id.0 as usize;
                        if let Some(Some(path)) = paths_snapshot.get(idx) {
                            if rec_blend_mode == BlendMode::default() {
                                rec.fill_path(path);
                            } else {
                                rec.push_layer(None, Some(rec_blend_mode), None, None);
                                rec.fill_path(path);
                                rec.pop_layer();
                            }
                        }
                    }
                    ImagingOp::Draw(DrawOp::StrokePath(id)) => {
                        let idx = id.0 as usize;
                        if let Some(Some(path)) = paths_snapshot.get(idx) {
                            if rec_blend_mode == BlendMode::default() {
                                rec.stroke_path(path);
                            } else {
                                rec.push_layer(None, Some(rec_blend_mode), None, None);
                                rec.stroke_path(path);
                                rec.pop_layer();
                            }
                        }
                    }
                    ImagingOp::Draw(DrawOp::FillRect { x0, y0, x1, y1 }) => {
                        let rect = Rect::new(*x0 as f64, *y0 as f64, *x1 as f64, *y1 as f64);
                        if rec_blend_mode == BlendMode::default() {
                            rec.fill_rect(&rect);
                        } else {
                            rec.push_layer(None, Some(rec_blend_mode), None, None);
                            rec.fill_rect(&rect);
                            rec.pop_layer();
                        }
                    }
                    ImagingOp::Draw(DrawOp::StrokeRect { x0, y0, x1, y1 }) => {
                        let rect = Rect::new(*x0 as f64, *y0 as f64, *x1 as f64, *y1 as f64);
                        if rec_blend_mode == BlendMode::default() {
                            rec.stroke_rect(&rect);
                        } else {
                            rec.push_layer(None, Some(rec_blend_mode), None, None);
                            rec.stroke_rect(&rect);
                            rec.pop_layer();
                        }
                    }
                    ImagingOp::Draw(DrawOp::DrawImage { .. })
                    | ImagingOp::Draw(DrawOp::DrawPicture { .. }) => {
                        // Nested images and pictures are not currently recorded
                        // into vello_cpu recordings.
                    }
                }
            }
        });

        self.ctx.prepare_recording(&mut recording);
        self.ctx.set_transform(saved_transform);
        self.ctx.set_paint(saved_paint);

        let acceleration: Option<Box<dyn core::any::Any>> =
            Some(Box::new(CpuRecordingAccel { recording }));

        RecordedOps {
            ops: ops_arc,
            acceleration,
            // Picture-local recordings are valid under any affine transform;
            // the outer CTM is applied at DrawPicture time.
            valid_under: TransformClass::Affine,
            original_ctm: Some(Affine::IDENTITY),
        }
    }
}
