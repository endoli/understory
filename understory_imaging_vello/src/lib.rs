// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_imaging_vello --heading-base-level=0

//! Vello backend for `understory_imaging`.
//!
//! This crate implements [`ImagingBackend`] and [`ResourceBackend`] on top of a Vello
//! [`vello::Scene`].
//!
//! It is primarily used by examples and higher-level crates that choose Vello as their rendering
//! engine.
//!
//! ## Notes
//!
//! - This backend translates the imaging IR into Vello scene commands; your application is still
//!   responsible for rendering the resulting [`vello::Scene`] using Vello’s renderer.
//! - Layer scoping is expressed using `StateOp::PushLayer`/`StateOp::PopLayer` (for clips and
//!   compositing), matching the layer-only model in `understory_imaging`.

#![no_std]

extern crate alloc;

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::any::Any;
use core::fmt;

use kurbo::{Affine, BezPath, Rect};
use understory_imaging::{
    ClipOp, ClipShape, DrawOp, FillRule, ImageDesc, ImageId, ImagingBackend, ImagingOp, LayerOp,
    PaintDesc, PaintId, PathCmd, PathDesc, PathId, PictureDesc, PictureId, RecordedOps, RectF,
    ResourceBackend, StateOp, StrokeStyle, TransformClass, stroke_outline_for_clip_shape,
};
use vello::Scene;
use vello::peniko::{Blob, Brush, Color, Fill, ImageBrush, ImageData};

const CLIP_TOLERANCE: f64 = 0.1;

/// Simple Vello-backed imaging backend that draws into a Vello [`Scene`].
pub struct VelloImagingBackend<'s> {
    /// Underlying Vello scene to draw into.
    pub scene: &'s mut Scene,
    paths: Vec<Option<PathDesc>>,
    images: Vec<Option<ImageData>>,
    paints: Vec<Option<PaintDesc>>,
    pictures: Vec<Option<PictureDesc>>,

    current_transform: Affine,
    current_stroke: Option<StrokeStyle>,
    current_brush: Brush,
    current_fill_rule: FillRule,
    current_paint_transform: Affine,
    stack: Vec<StackEntry>,

    /// Buffered imaging ops captured between `begin_record`/`end_record`.
    recording_ops: Vec<ImagingOp>,
    /// Whether recording is currently active.
    recording_active: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum StackEntry {
    Noop,
    Pushed,
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
            current_fill_rule: FillRule::NonZero,
            current_paint_transform: Affine::IDENTITY,
            stack: Vec::new(),
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
        // Vello's scene encoding may borrow image data; keep it alive for the
        // lifetime of the resource rather than constructing a temporary image
        // in `DrawImage`.
        let data: Blob<u8> = Blob::from(pixels.to_vec());
        self.images.push(Some(ImageData {
            data,
            format: desc.format,
            alpha_type: desc.alpha_type,
            width: desc.width,
            height: desc.height,
        }));
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
            StateOp::SetFillRule(rule) => {
                self.current_fill_rule = rule;
            }
            StateOp::PushLayer(LayerOp {
                clip,
                filter: _,
                blend,
                opacity,
            }) => {
                let needs_compositing = blend.is_some() || opacity.is_some();
                if clip.is_none() && !needs_compositing {
                    self.stack.push(StackEntry::Noop);
                    return;
                }

                let blend = blend.unwrap_or_default();
                let opacity = opacity.unwrap_or(1.0).clamp(0.0, 1.0);

                if needs_compositing {
                    // Use a Vello layer for blending/opacity. If no clip is provided, use a large
                    // clip region so we don't unintentionally trim geometry.
                    let big = Rect::new(-10_000.0, -10_000.0, 10_000.0, 10_000.0);
                    match clip {
                        None => self
                            .scene
                            .push_layer(blend, opacity, Affine::IDENTITY, &big),
                        Some(ClipOp::Fill {
                            shape,
                            fill_rule: _,
                        }) => match shape {
                            ClipShape::Rect(rect) => {
                                self.scene.push_layer(
                                    blend,
                                    opacity,
                                    self.current_transform,
                                    &rect.to_kurbo(),
                                );
                            }
                            ClipShape::RoundedRect(rr) => {
                                self.scene.push_layer(
                                    blend,
                                    opacity,
                                    self.current_transform,
                                    &rr.to_kurbo(),
                                );
                            }
                            ClipShape::Path(id) => {
                                let Some(path) = self.path_to_bez(id) else {
                                    self.stack.push(StackEntry::Noop);
                                    return;
                                };
                                self.scene.push_layer(
                                    blend,
                                    opacity,
                                    self.current_transform,
                                    &path,
                                );
                            }
                        },
                        Some(ClipOp::Stroke { shape, style }) => {
                            let Some(outline) = stroke_outline_for_clip_shape(
                                &shape,
                                &style,
                                CLIP_TOLERANCE,
                                |id| self.path_to_bez(id),
                            ) else {
                                self.stack.push(StackEntry::Noop);
                                return;
                            };
                            self.scene
                                .push_layer(blend, opacity, self.current_transform, &outline);
                        }
                    }
                    self.stack.push(StackEntry::Pushed);
                    return;
                }

                // Clip-only layer: use a clip layer (non-isolating).
                let Some(clip) = clip else {
                    self.stack.push(StackEntry::Noop);
                    return;
                };
                match clip {
                    ClipOp::Fill {
                        shape,
                        fill_rule: _,
                    } => match shape {
                        ClipShape::Rect(rect) => {
                            self.scene
                                .push_clip_layer(self.current_transform, &rect.to_kurbo());
                        }
                        ClipShape::RoundedRect(rr) => {
                            self.scene
                                .push_clip_layer(self.current_transform, &rr.to_kurbo());
                        }
                        ClipShape::Path(id) => {
                            let Some(path) = self.path_to_bez(id) else {
                                self.stack.push(StackEntry::Noop);
                                return;
                            };
                            self.scene.push_clip_layer(self.current_transform, &path);
                        }
                    },
                    ClipOp::Stroke { shape, style } => {
                        let Some(outline) =
                            stroke_outline_for_clip_shape(&shape, &style, CLIP_TOLERANCE, |id| {
                                self.path_to_bez(id)
                            })
                        else {
                            self.stack.push(StackEntry::Noop);
                            return;
                        };
                        self.scene.push_clip_layer(self.current_transform, &outline);
                    }
                }
                self.stack.push(StackEntry::Pushed);
            }
            StateOp::PopLayer => match self.stack.pop() {
                Some(StackEntry::Noop) => {}
                Some(StackEntry::Pushed) => self.scene.pop_layer(),
                None => panic!("PopLayer with empty stack"),
            },
            StateOp::SetPaint(id) => {
                if let Some(Some(desc)) = self.paints.get(id.0 as usize) {
                    self.current_brush = desc.brush.clone();
                }
            }
        }
    }

    fn draw(&mut self, op: DrawOp) {
        if self.recording_active {
            self.recording_ops.push(ImagingOp::Draw(op.clone()));
        }

        match op {
            DrawOp::FillPath(id) => {
                if let Some(shape) = self.path_to_bez(id) {
                    let fill = match self.current_fill_rule {
                        FillRule::NonZero => Fill::NonZero,
                        FillRule::EvenOdd => Fill::EvenOdd,
                    };
                    self.scene.fill(
                        fill,
                        self.current_transform,
                        &self.current_brush,
                        Some(self.current_paint_transform),
                        &shape,
                    );
                }
            }
            DrawOp::FillRect { x0, y0, x1, y1 } => {
                let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                let fill = match self.current_fill_rule {
                    FillRule::NonZero => Fill::NonZero,
                    FillRule::EvenOdd => Fill::EvenOdd,
                };
                self.scene.fill(
                    fill,
                    self.current_transform,
                    &self.current_brush,
                    Some(self.current_paint_transform),
                    &rect,
                );
            }
            DrawOp::StrokePath(id) => {
                if let (Some(shape), Some(stroke)) =
                    (self.path_to_bez(id), self.current_stroke.clone())
                {
                    self.scene.stroke(
                        &stroke,
                        self.current_transform,
                        &self.current_brush,
                        Some(self.current_paint_transform),
                        &shape,
                    );
                }
            }
            DrawOp::StrokeRect { x0, y0, x1, y1 } => {
                if let Some(stroke) = self.current_stroke.clone() {
                    let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
                    self.scene.stroke(
                        &stroke,
                        self.current_transform,
                        &self.current_brush,
                        Some(self.current_paint_transform),
                        &rect,
                    );
                }
            }
            DrawOp::DrawImage {
                image,
                transform,
                sampler,
            } => {
                let idx = image.0 as usize;
                if let Some(Some(image)) = self.images.get(idx) {
                    let brush = ImageBrush { image, sampler };
                    self.scene
                        .draw_image(brush, self.current_transform * transform);
                }
            }
            DrawOp::DrawImageRect {
                image,
                src,
                dst,
                sampler,
            } => {
                let idx = image.0 as usize;
                if let Some(Some(image)) = self.images.get(idx) {
                    let src = src.unwrap_or(RectF {
                        x0: 0.0,
                        y0: 0.0,
                        x1: image.width as f32,
                        y1: image.height as f32,
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

                    // Clip to the destination rect so only the mapped region is visible.
                    let clip =
                        Rect::new(dst.x0 as f64, dst.y0 as f64, dst.x1 as f64, dst.y1 as f64);
                    self.scene.push_clip_layer(self.current_transform, &clip);
                    let brush = ImageBrush { image, sampler };
                    self.scene.draw_image(brush, self.current_transform * local);

                    self.scene.pop_layer(); // pop clip
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
                    let saved_stack_len = self.stack.len();
                    let saved_paint_transform = self.current_paint_transform;

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
                        if let Some(entry) = self.stack.pop()
                            && entry == StackEntry::Pushed
                        {
                            self.scene.pop_layer();
                        }
                    }
                    self.current_transform = saved_transform;
                    self.current_stroke = saved_stroke;
                    self.current_brush = saved_brush;
                    self.current_paint_transform = saved_paint_transform;
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
                current_fill_rule: FillRule::NonZero,
                current_paint_transform: Affine::IDENTITY,
                stack: Vec::new(),
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

        RecordedOps {
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
            recording: RecordedOps {
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
