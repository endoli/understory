// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_imaging_vello_cpu --heading-base-level=0

//! Vello CPU–backed implementation of the imaging backend.
//!
//! This crate implements [`ImagingBackend`] on top of
//! the sparse-strips [`vello_cpu::RenderContext`], so that the same imaging
//! IR can be rendered either via Vello Classic/GPU or via the CPU renderer.

#![deny(unsafe_code)]
#![no_std]

extern crate alloc;

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::any::Any;
use core::fmt;
use kurbo::{Affine, Cap, Join};
use peniko::{Brush, Fill, ImageData};
use understory_imaging::{
    ClipOp, DrawOp, FillRule, FilterDesc, ImageDesc, ImageId, ImagingBackend, ImagingOp, LayerOp,
    PaintDesc, PaintId, PathCmd, PathDesc, PathId, PictureDesc, PictureId, RecordedOps, RectF,
    ResourceBackend, StateOp, TransformClass, clip_shape_to_bez_path,
    stroke_outline_for_clip_shape,
};
use vello_common::filter_effects::{EdgeMode, Filter, FilterPrimitive};
use vello_common::recording::{Recordable, Recording as CpuRecording};
use vello_cpu::kurbo::{
    Affine as CpuAffine, BezPath, Cap as CpuCap, Join as CpuJoin, Rect, Stroke,
};
use vello_cpu::{Image as CpuImage, ImageSource, RenderContext};

const CLIP_TOLERANCE: f64 = 0.1;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum StackEntry {
    Noop,
    /// Number of `vello_cpu` layers pushed to represent a single `StateOp::PushLayer`.
    Pushed(u8),
}

fn with_temp_fill_rule<C>(
    ctx: &mut C,
    saved: FillRule,
    desired: FillRule,
    mut set_rule: impl FnMut(&mut C, FillRule),
    f: impl FnOnce(&mut C),
) {
    if desired != saved {
        set_rule(ctx, desired);
    }
    f(ctx);
    if desired != saved {
        set_rule(ctx, saved);
    }
}

struct CpuRecordingAccel {
    /// Cached recordings keyed by the exact base CTM they were prepared for.
    ///
    /// `vello_cpu` cached strips are effectively transform-dependent, so we keep
    /// separate recordings per transform (exact match).
    recordings: Vec<(Affine, CpuRecording)>,
}

/// CPU-backed implementation of the imaging backend using `vello_cpu`.
pub struct VelloCpuImagingBackend<'ctx> {
    /// Underlying Vello CPU render context to draw into.
    pub ctx: &'ctx mut RenderContext,
    paths: Vec<Option<BezPath>>,
    images: Vec<Option<(ImageDesc, Vec<u8>)>>,
    paints: Vec<Option<PaintDesc>>,
    pictures: Vec<Option<PictureDesc>>,

    stack: Vec<StackEntry>,
    current_paint: Option<PaintId>,
    current_fill_rule: FillRule,
    current_transform: Affine,
    /// Buffered imaging ops captured between `begin_record`/`end_record`.
    recording_ops: Vec<ImagingOp>,
    /// Whether recording is currently active.
    recording_active: bool,
}

impl fmt::Debug for VelloCpuImagingBackend<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            stack: Vec::new(),
            current_paint: None,
            current_fill_rule: FillRule::NonZero,
            current_transform: Affine::IDENTITY,
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

    fn filter_to_vello(filter: FilterDesc) -> Option<Filter> {
        // `vello_cpu` scales filter parameters using the layer's transform internally.
        // Understory `FilterDesc` parameters are specified in user space, so we pass them through.
        match filter {
            FilterDesc::Flood { color } => {
                Some(Filter::from_primitive(FilterPrimitive::Flood { color }))
            }
            FilterDesc::Blur {
                std_deviation_x,
                std_deviation_y,
            } => {
                // vello_common currently supports only uniform blur.
                let sigma = std_deviation_x.max(std_deviation_y);
                Some(Filter::from_primitive(FilterPrimitive::GaussianBlur {
                    std_deviation: sigma,
                    edge_mode: EdgeMode::None,
                }))
            }
            FilterDesc::DropShadow {
                dx,
                dy,
                std_deviation_x,
                std_deviation_y,
                color,
            } => {
                // vello_common currently supports only uniform blur.
                let sigma = std_deviation_x.max(std_deviation_y);

                Some(Filter::from_primitive(FilterPrimitive::DropShadow {
                    dx,
                    dy,
                    std_deviation: sigma,
                    color,
                    edge_mode: EdgeMode::None,
                }))
            }
            FilterDesc::Offset { .. } => None,
        }
    }

    fn translate_ops_to_recording(
        ctx: &mut RenderContext,
        recording: &mut CpuRecording,
        ops: &[ImagingOp],
        base_transform: Affine,
        paths_snapshot: &[Option<BezPath>],
        images_snapshot: &[Option<(ImageDesc, Vec<u8>)>],
        paints_snapshot: &[Option<PaintDesc>],
    ) {
        ctx.record(recording, |rec| {
            // Local imaging state while translating the IR into vello_cpu commands.
            let mut rec_stack: Vec<StackEntry> = Vec::new();
            let mut rec_paint_id: Option<PaintId> = None;
            let mut rec_transform = base_transform;
            let mut rec_paint_transform = Affine::IDENTITY;
            let mut rec_fill_rule = FillRule::NonZero;

            let set_paint_from_id = |rec: &mut vello_common::recording::Recorder<'_>,
                                     paints: &[Option<PaintDesc>],
                                     paint_id: PaintId| {
                let idx = paint_id.0 as usize;
                if let Some(Some(PaintDesc { brush })) = paints.get(idx) {
                    match brush.clone() {
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

            // Start the recording at the requested base transform (outer CTM applied).
            rec.set_transform(Self::affine_to_cpu(base_transform));

            for op in ops {
                match op {
                    ImagingOp::State(StateOp::SetTransform(xf)) => {
                        rec_transform = base_transform * *xf;
                        rec.set_transform(Self::affine_to_cpu(rec_transform));
                    }
                    ImagingOp::State(StateOp::SetFillRule(rule)) => {
                        rec_fill_rule = *rule;
                        let fill = match rule {
                            FillRule::NonZero => Fill::NonZero,
                            FillRule::EvenOdd => Fill::EvenOdd,
                        };
                        rec.set_fill_rule(fill);
                    }
                    ImagingOp::State(StateOp::SetPaint(id)) => {
                        rec_paint_id = Some(*id);
                        set_paint_from_id(rec, paints_snapshot, *id);
                    }
                    ImagingOp::State(StateOp::SetPaintTransform(xf)) => {
                        rec_paint_transform = *xf;
                        rec.set_paint_transform(Self::affine_to_cpu(rec_paint_transform));
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
                    ImagingOp::State(StateOp::PushLayer(layer)) => {
                        let mut desired_fill_rule = rec_fill_rule;
                        let clip_path: Option<BezPath> = match &layer.clip {
                            Some(ClipOp::Fill { shape, fill_rule }) => {
                                desired_fill_rule = *fill_rule;
                                clip_shape_to_bez_path(shape, CLIP_TOLERANCE, |id: PathId| {
                                    paths_snapshot
                                        .get(id.0 as usize)
                                        .and_then(|slot| slot.as_ref().cloned())
                                })
                            }
                            Some(ClipOp::Stroke { shape, style }) => stroke_outline_for_clip_shape(
                                shape,
                                style,
                                CLIP_TOLERANCE,
                                |id: PathId| -> Option<BezPath> {
                                    paths_snapshot
                                        .get(id.0 as usize)
                                        .and_then(|slot| slot.as_ref().cloned())
                                },
                            ),
                            None => None,
                        };

                        let filter = layer.filter.clone().and_then(Self::filter_to_vello);

                        if clip_path.is_some()
                            || layer.blend.is_some()
                            || layer.opacity.is_some()
                            || filter.is_some()
                        {
                            let saved = rec_fill_rule;
                            with_temp_fill_rule(
                                rec,
                                saved,
                                desired_fill_rule,
                                |rec, rule| {
                                    let fill = match rule {
                                        FillRule::NonZero => Fill::NonZero,
                                        FillRule::EvenOdd => Fill::EvenOdd,
                                    };
                                    rec.set_fill_rule(fill);
                                },
                                |rec| {
                                    rec.push_layer(
                                        clip_path.as_ref(),
                                        layer.blend,
                                        layer.opacity,
                                        None,
                                        filter,
                                    );
                                },
                            );
                            rec_stack.push(StackEntry::Pushed(1));
                        } else {
                            rec_stack.push(StackEntry::Noop);
                        }
                    }
                    ImagingOp::State(StateOp::PopLayer) => match rec_stack.pop() {
                        Some(StackEntry::Noop) => {}
                        Some(StackEntry::Pushed(n)) => {
                            for _ in 0..n {
                                rec.pop_layer();
                            }
                        }
                        None => panic!("PopLayer with empty stack"),
                    },
                    ImagingOp::Draw(DrawOp::FillPath(id)) => {
                        let idx = id.0 as usize;
                        if let Some(Some(path)) = paths_snapshot.get(idx) {
                            rec.fill_path(path);
                        }
                    }
                    ImagingOp::Draw(DrawOp::StrokePath(id)) => {
                        let idx = id.0 as usize;
                        if let Some(Some(path)) = paths_snapshot.get(idx) {
                            rec.stroke_path(path);
                        }
                    }
                    ImagingOp::Draw(DrawOp::FillRect { x0, y0, x1, y1 }) => {
                        let rect = Rect::new(*x0 as f64, *y0 as f64, *x1 as f64, *y1 as f64);
                        rec.fill_rect(&rect);
                    }
                    ImagingOp::Draw(DrawOp::StrokeRect { x0, y0, x1, y1 }) => {
                        let rect = Rect::new(*x0 as f64, *y0 as f64, *x1 as f64, *y1 as f64);
                        rec.stroke_rect(&rect);
                    }
                    ImagingOp::Draw(DrawOp::DrawImage {
                        image,
                        transform,
                        sampler,
                    }) => {
                        let idx = image.0 as usize;
                        if let Some(Some((desc, pixels))) = images_snapshot.get(idx) {
                            let data = peniko::Blob::from(pixels.clone());
                            let image_data = ImageData {
                                data,
                                format: desc.format,
                                alpha_type: desc.alpha_type,
                                width: desc.width,
                                height: desc.height,
                            };

                            let source = ImageSource::from_peniko_image_data(&image_data);
                            let image_paint = CpuImage {
                                image: source,
                                sampler: *sampler,
                            };

                            // Save current recording state.
                            let saved_transform = rec_transform;
                            let saved_paint_transform = rec_paint_transform;
                            let saved_paint_id = rec_paint_id;

                            // Apply per-image local transform on top of the current CTM.
                            rec.set_transform(Self::affine_to_cpu(rec_transform * *transform));
                            rec.set_paint(image_paint);

                            // Draw the image at its natural size at the origin.
                            let rect =
                                Rect::new(0.0, 0.0, f64::from(desc.width), f64::from(desc.height));
                            rec.fill_rect(&rect);

                            // Restore prior state.
                            rec_transform = saved_transform;
                            rec_paint_transform = saved_paint_transform;
                            rec_paint_id = saved_paint_id;
                            rec.set_transform(Self::affine_to_cpu(rec_transform));
                            rec.set_paint_transform(Self::affine_to_cpu(rec_paint_transform));
                            if let Some(paint_id) = rec_paint_id {
                                set_paint_from_id(rec, paints_snapshot, paint_id);
                            }
                        }
                    }
                    ImagingOp::Draw(DrawOp::DrawImageRect {
                        image,
                        src,
                        dst,
                        sampler,
                    }) => {
                        let idx = image.0 as usize;
                        if let Some(Some((desc, pixels))) = images_snapshot.get(idx) {
                            let src = src.unwrap_or(RectF {
                                x0: 0.0,
                                y0: 0.0,
                                x1: desc.width as f32,
                                y1: desc.height as f32,
                            });
                            let dst_w = dst.x1 - dst.x0;
                            let dst_h = dst.y1 - dst.y0;
                            let src_w = src.x1 - src.x0;
                            let src_h = src.y1 - src.y0;
                            if dst_w.abs() < f32::EPSILON
                                || dst_h.abs() < f32::EPSILON
                                || src_w.abs() < f32::EPSILON
                                || src_h.abs() < f32::EPSILON
                            {
                                continue;
                            }

                            let local = Affine::translate((dst.x0 as f64, dst.y0 as f64))
                                * Affine::scale_non_uniform(
                                    (dst_w / src_w) as f64,
                                    (dst_h / src_h) as f64,
                                )
                                * Affine::translate((-src.x0 as f64, -src.y0 as f64));

                            let data = peniko::Blob::from(pixels.clone());
                            let image_data = ImageData {
                                data,
                                format: desc.format,
                                alpha_type: desc.alpha_type,
                                width: desc.width,
                                height: desc.height,
                            };
                            let source = ImageSource::from_peniko_image_data(&image_data);
                            let image_paint = CpuImage {
                                image: source,
                                sampler: *sampler,
                            };

                            // Save current recording state.
                            let saved_transform = rec_transform;
                            let saved_paint_transform = rec_paint_transform;
                            let saved_paint_id = rec_paint_id;

                            // Clip to dst in the current (non-image) transform.
                            let mut dst_path = BezPath::new();
                            dst_path.move_to((dst.x0 as f64, dst.y0 as f64));
                            dst_path.line_to((dst.x1 as f64, dst.y0 as f64));
                            dst_path.line_to((dst.x1 as f64, dst.y1 as f64));
                            dst_path.line_to((dst.x0 as f64, dst.y1 as f64));
                            dst_path.close_path();
                            rec.push_clip_layer(&dst_path);

                            // Apply per-image mapping transform and draw the image.
                            rec.set_transform(Self::affine_to_cpu(rec_transform * local));
                            rec.set_paint(image_paint);

                            let rect =
                                Rect::new(0.0, 0.0, f64::from(desc.width), f64::from(desc.height));
                            rec.fill_rect(&rect);

                            rec.pop_layer(); // pop clip

                            // Restore prior state.
                            rec_transform = saved_transform;
                            rec_paint_transform = saved_paint_transform;
                            rec_paint_id = saved_paint_id;
                            rec.set_transform(Self::affine_to_cpu(rec_transform));
                            rec.set_paint_transform(Self::affine_to_cpu(rec_paint_transform));
                            if let Some(paint_id) = rec_paint_id {
                                set_paint_from_id(rec, paints_snapshot, paint_id);
                            }
                        }
                    }
                    ImagingOp::Draw(DrawOp::DrawPicture { .. }) => {
                        // Nested pictures are not currently recorded into vello_cpu recordings.
                    }
                }
            }
        });
    }

    fn apply_current_paint(&mut self) {
        let Some(id) = self.current_paint else {
            return;
        };
        let idx = id.0 as usize;
        if let Some(Some(PaintDesc { brush })) = self.paints.get(idx) {
            match brush.clone() {
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
                self.current_transform = xf;
                self.ctx.set_transform(Self::affine_to_cpu(xf));
            }
            StateOp::SetPaintTransform(xf) => {
                self.ctx.set_paint_transform(Self::affine_to_cpu(xf));
            }
            StateOp::PushLayer(layer) => {
                let LayerOp {
                    clip,
                    filter,
                    blend,
                    opacity,
                } = layer;

                let mut desired_fill_rule = self.current_fill_rule;
                let clip_path: Option<BezPath> = match clip {
                    Some(ClipOp::Fill { shape, fill_rule }) => {
                        desired_fill_rule = fill_rule;
                        clip_shape_to_bez_path(&shape, CLIP_TOLERANCE, |id| self.path_to_bez(id))
                    }
                    Some(ClipOp::Stroke { shape, style }) => {
                        stroke_outline_for_clip_shape(&shape, &style, CLIP_TOLERANCE, |id| {
                            self.path_to_bez(id)
                        })
                    }
                    None => None,
                };

                let filter = filter.and_then(Self::filter_to_vello);

                if clip_path.is_some() || blend.is_some() || opacity.is_some() || filter.is_some() {
                    let saved = self.current_fill_rule;
                    with_temp_fill_rule(
                        self.ctx,
                        saved,
                        desired_fill_rule,
                        |ctx, rule| {
                            let fill = match rule {
                                FillRule::NonZero => Fill::NonZero,
                                FillRule::EvenOdd => Fill::EvenOdd,
                            };
                            ctx.set_fill_rule(fill);
                        },
                        |ctx| ctx.push_layer(clip_path.as_ref(), blend, opacity, None, filter),
                    );
                    self.stack.push(StackEntry::Pushed(1));
                } else {
                    self.stack.push(StackEntry::Noop);
                }
            }
            StateOp::PopLayer => match self.stack.pop() {
                Some(StackEntry::Noop) => {}
                Some(StackEntry::Pushed(n)) => {
                    for _ in 0..n {
                        self.ctx.pop_layer();
                    }
                }
                None => panic!("PopLayer with empty stack"),
            },
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
            StateOp::SetFillRule(rule) => {
                self.current_fill_rule = rule;
                // vello_cpu uses peniko::Fill for fill rules.
                let fill = match rule {
                    FillRule::NonZero => Fill::NonZero,
                    FillRule::EvenOdd => Fill::EvenOdd,
                };
                self.ctx.set_fill_rule(fill);
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
                    self.ctx.fill_path(&path);
                }
            }
            DrawOp::StrokePath(id) => {
                if let Some(path) = self.path_to_bez(id) {
                    self.ctx.stroke_path(&path);
                }
            }
            DrawOp::FillRect { x0, y0, x1, y1 } => {
                let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                self.ctx.fill_rect(&rect);
            }
            DrawOp::StrokeRect { x0, y0, x1, y1 } => {
                let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                self.ctx.stroke_rect(&rect);
            }
            DrawOp::DrawImage {
                image,
                transform,
                sampler,
            } => {
                let idx = image.0 as usize;
                if let Some(Some((desc, pixels))) = self.images.get(idx) {
                    let data = peniko::Blob::from(pixels.clone());
                    let image_data = ImageData {
                        data,
                        format: desc.format,
                        alpha_type: desc.alpha_type,
                        width: desc.width,
                        height: desc.height,
                    };

                    let source = ImageSource::from_peniko_image_data(&image_data);
                    let image_paint = CpuImage {
                        image: source,
                        sampler,
                    };

                    let saved_transform_affine = self.current_transform;
                    let saved_transform = *self.ctx.transform();
                    let saved_paint = self.ctx.paint().clone();

                    self.ctx.set_paint(image_paint);
                    let combined = saved_transform * Self::affine_to_cpu(transform);
                    self.current_transform = saved_transform_affine * transform;
                    self.ctx.set_transform(combined);

                    let rect = Rect::new(0.0, 0.0, f64::from(desc.width), f64::from(desc.height));

                    self.ctx.fill_rect(&rect);

                    self.ctx.set_transform(saved_transform);
                    self.current_transform = saved_transform_affine;
                    self.ctx.set_paint(saved_paint);
                }
            }
            DrawOp::DrawImageRect {
                image,
                src,
                dst,
                sampler,
            } => {
                let idx = image.0 as usize;
                if let Some(Some((desc, pixels))) = self.images.get(idx) {
                    let src = src.unwrap_or(RectF {
                        x0: 0.0,
                        y0: 0.0,
                        x1: desc.width as f32,
                        y1: desc.height as f32,
                    });
                    let dst_w = dst.x1 - dst.x0;
                    let dst_h = dst.y1 - dst.y0;
                    let src_w = src.x1 - src.x0;
                    let src_h = src.y1 - src.y0;
                    if dst_w.abs() < f32::EPSILON
                        || dst_h.abs() < f32::EPSILON
                        || src_w.abs() < f32::EPSILON
                        || src_h.abs() < f32::EPSILON
                    {
                        return;
                    }

                    let local = Affine::translate((dst.x0 as f64, dst.y0 as f64))
                        * Affine::scale_non_uniform((dst_w / src_w) as f64, (dst_h / src_h) as f64)
                        * Affine::translate((-src.x0 as f64, -src.y0 as f64));

                    let data = peniko::Blob::from(pixels.clone());
                    let image_data = ImageData {
                        data,
                        format: desc.format,
                        alpha_type: desc.alpha_type,
                        width: desc.width,
                        height: desc.height,
                    };

                    let source = ImageSource::from_peniko_image_data(&image_data);
                    let image_paint = CpuImage {
                        image: source,
                        sampler,
                    };

                    let saved_transform_affine = self.current_transform;
                    let saved_transform = *self.ctx.transform();
                    let saved_paint = self.ctx.paint().clone();

                    // Clip to destination in the current (non-image) transform.
                    let mut dst_path = BezPath::new();
                    dst_path.move_to((dst.x0 as f64, dst.y0 as f64));
                    dst_path.line_to((dst.x1 as f64, dst.y0 as f64));
                    dst_path.line_to((dst.x1 as f64, dst.y1 as f64));
                    dst_path.line_to((dst.x0 as f64, dst.y1 as f64));
                    dst_path.close_path();
                    self.ctx.push_clip_layer(&dst_path);

                    self.ctx.set_paint(image_paint);
                    let combined = saved_transform * Self::affine_to_cpu(local);
                    self.current_transform = saved_transform_affine * local;
                    self.ctx.set_transform(combined);

                    let rect = Rect::new(0.0, 0.0, f64::from(desc.width), f64::from(desc.height));

                    self.ctx.fill_rect(&rect);

                    self.ctx.set_transform(saved_transform);
                    self.current_transform = saved_transform_affine;
                    self.ctx.set_paint(saved_paint);
                    self.ctx.pop_layer(); // pop clip
                }
            }
            DrawOp::DrawPicture { picture, transform } => {
                let idx = picture.0 as usize;
                if let Some(Some(desc)) = self.pictures.get_mut(idx) {
                    let saved_transform_affine = self.current_transform;
                    let saved_transform = *self.ctx.transform();
                    let saved_paint = self.ctx.paint().clone();
                    let base = Affine::new(saved_transform.as_coeffs()) * transform;

                    // Cached strips in vello_cpu recordings are transform-dependent, so keep
                    // a per-transform cache of recordings for this picture.
                    if desc.recording.acceleration.is_none() {
                        desc.recording.acceleration = Some(Box::new(CpuRecordingAccel {
                            recordings: Vec::new(),
                        }));
                    }

                    let mut used_accel = false;
                    if let Some(accel_any) = desc.recording.acceleration.as_mut()
                        && let Some(accel) = accel_any.downcast_mut::<CpuRecordingAccel>()
                    {
                        if let Some((_, cached)) =
                            accel.recordings.iter().find(|(xf, _)| *xf == base)
                        {
                            self.ctx.execute_recording(cached);
                            used_accel = true;
                        } else {
                            let paths_snapshot = self.paths.clone();
                            let images_snapshot = self.images.clone();
                            let paints_snapshot = self.paints.clone();
                            let ops: Vec<_> = desc.recording.ops.to_vec();
                            let mut recording = CpuRecording::new();
                            Self::translate_ops_to_recording(
                                self.ctx,
                                &mut recording,
                                &ops,
                                base,
                                &paths_snapshot,
                                &images_snapshot,
                                &paints_snapshot,
                            );
                            self.ctx.prepare_recording(&mut recording);
                            self.ctx.execute_recording(&recording);
                            accel.recordings.push((base, recording));
                            used_accel = true;
                        }
                    }

                    if !used_accel {
                        // Fallback: replay the picture's IR directly into this
                        // backend, applying the outer transform to any
                        // SetTransform ops. This path is primarily for
                        // recordings created by other backends that don't
                        // provide vello_cpu-specific acceleration.
                        let saved_stack_len = self.stack.len();
                        let saved_paint_id = self.current_paint;

                        let ops: Vec<_> = desc.recording.ops.to_vec();
                        let saved_recording = self.recording_active;
                        self.recording_active = false;
                        for op in ops {
                            match op {
                                ImagingOp::State(StateOp::SetTransform(xf)) => {
                                    self.state(StateOp::SetTransform(transform * xf));
                                }
                                ImagingOp::State(s) => self.state(s),
                                ImagingOp::Draw(d) => self.draw(d),
                            }
                        }
                        self.recording_active = saved_recording;

                        while self.stack.len() > saved_stack_len {
                            match self.stack.pop() {
                                Some(StackEntry::Noop) => {}
                                Some(StackEntry::Pushed(n)) => {
                                    for _ in 0..n {
                                        self.ctx.pop_layer();
                                    }
                                }
                                None => break,
                            }
                        }
                        self.current_paint = saved_paint_id;
                    }

                    self.ctx.set_transform(saved_transform);
                    self.current_transform = saved_transform_affine;
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
        let acceleration: Option<Box<dyn Any>> = Some(Box::new(CpuRecordingAccel {
            recordings: Vec::new(),
        }));

        RecordedOps {
            ops: ops_arc,
            acceleration,
            // Cached strips are effectively transform-dependent in vello_cpu, so
            // we don't claim any particular reuse guarantees here.
            valid_under: TransformClass::Exact,
            original_ctm: None,
        }
    }
}
