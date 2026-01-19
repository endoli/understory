// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_imaging_svg --heading-base-level=0

//! SVG export backend for the Understory imaging IR.
//!
//! This crate provides a small implementation of
//! [`ImagingBackend`] and [`ResourceBackend`] that records imaging ops and
//! can export them as an SVG document.
//!
//! This is intended for debugging/inspection, not pixel-perfect rendering:
//! - Not all brush types are supported yet (solid colors are; other brushes use a fallback).
//! - Layer compositing semantics are approximated using SVG `<g>` with `opacity`/`mix-blend-mode`.
//! - Image drawing is represented as placeholders (no embedded pixels yet).

#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Write as _;
#[cfg(not(feature = "std"))]
use kurbo::common::FloatFuncs as _; // for `sqrt`
use kurbo::{BezPath, Cap, Join, Shape};
use peniko::{Brush, Color, Compose, Mix};
use understory_imaging::{
    Affine, BlendMode, ClipOp, ClipShape, DrawOp, FillRule, FilterDesc, ImageDesc, ImageId,
    ImagingBackend, ImagingOp, PaintDesc, PaintId, PathCmd, PathDesc, PathId, PictureDesc,
    PictureId, RecordedOps, ResourceBackend, RoundedRectF, StateOp, StrokeStyle, TransformClass,
    stroke_outline_for_clip_shape,
};

const CLIP_TOLERANCE: f64 = 0.1;

#[derive(Clone, Debug)]
struct SvgState {
    transform: Affine,
    paint: Option<PaintId>,
    stroke: Option<StrokeStyle>,
    fill_rule: FillRule,
}

impl Default for SvgState {
    fn default() -> Self {
        Self {
            transform: Affine::IDENTITY,
            paint: None,
            stroke: None,
            fill_rule: FillRule::NonZero,
        }
    }
}

/// A recording SVG backend.
#[derive(Default, Debug)]
pub struct SvgBackend {
    paths: Vec<Option<PathDesc>>,
    images: Vec<Option<(ImageDesc, Vec<u8>)>>,
    paints: Vec<Option<PaintDesc>>,
    pictures: Vec<Option<PictureDesc>>,

    ops: Vec<ImagingOp>,
    recording_start: Option<usize>,
}

impl SvgBackend {
    /// Clears the recorded ops while retaining resources.
    pub fn clear_ops(&mut self) {
        self.ops.clear();
        self.recording_start = None;
    }

    /// Returns the recorded imaging ops.
    pub fn ops(&self) -> &[ImagingOp] {
        &self.ops
    }

    /// Export the currently recorded ops as an SVG document.
    ///
    /// `width`/`height` are used both as the SVG `width`/`height` attributes and to set
    /// `viewBox="0 0 width height"`.
    pub fn to_svg(&self, width: u32, height: u32) -> String {
        render_svg_document(self, width, height, &self.ops)
    }
}

impl ResourceBackend for SvgBackend {
    fn create_path(&mut self, desc: PathDesc) -> PathId {
        let id =
            u32::try_from(self.paths.len()).expect("SvgBackend: too many paths for u32 PathId");
        self.paths.push(Some(desc));
        PathId(id)
    }

    fn destroy_path(&mut self, id: PathId) {
        if let Some(slot) = self.paths.get_mut(id.0 as usize) {
            *slot = None;
        }
    }

    fn create_image(&mut self, desc: ImageDesc, pixels: &[u8]) -> ImageId {
        let id =
            u32::try_from(self.images.len()).expect("SvgBackend: too many images for u32 ImageId");
        self.images.push(Some((desc, pixels.to_vec())));
        ImageId(id)
    }

    fn destroy_image(&mut self, id: ImageId) {
        if let Some(slot) = self.images.get_mut(id.0 as usize) {
            *slot = None;
        }
    }

    fn create_paint(&mut self, desc: PaintDesc) -> PaintId {
        let id =
            u32::try_from(self.paints.len()).expect("SvgBackend: too many paints for u32 PaintId");
        self.paints.push(Some(desc));
        PaintId(id)
    }

    fn destroy_paint(&mut self, id: PaintId) {
        if let Some(slot) = self.paints.get_mut(id.0 as usize) {
            *slot = None;
        }
    }

    fn create_picture(&mut self, desc: PictureDesc) -> PictureId {
        let id = u32::try_from(self.pictures.len())
            .expect("SvgBackend: too many pictures for u32 PictureId");
        self.pictures.push(Some(desc));
        PictureId(id)
    }

    fn destroy_picture(&mut self, id: PictureId) {
        if let Some(slot) = self.pictures.get_mut(id.0 as usize) {
            *slot = None;
        }
    }
}

impl ImagingBackend for SvgBackend {
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

fn render_svg_document(backend: &SvgBackend, width: u32, height: u32, ops: &[ImagingOp]) -> String {
    let mut defs = String::new();
    let mut body = String::new();

    let mut layer_stack: Vec<bool> = Vec::new();
    let mut clip_counter: u64 = 0;
    let mut filter_counter: u64 = 0;

    let mut state = SvgState::default();

    for op in ops {
        match op.clone() {
            ImagingOp::State(state_op) => match state_op {
                StateOp::SetTransform(xf) => state.transform = xf,
                StateOp::SetPaintTransform(_xf) => {}
                StateOp::SetPaint(id) => state.paint = Some(id),
                StateOp::SetStroke(style) => state.stroke = Some(style),
                StateOp::SetFillRule(rule) => state.fill_rule = rule,
                StateOp::PushLayer(layer) => {
                    let mut opened = false;

                    if layer.clip.is_some() || layer.has_compositing_effects() {
                        let mut attrs = String::new();

                        if let Some(clip) = layer.clip {
                            clip_counter += 1;
                            let clip_id = format!("clip{clip_counter}");
                            match clip {
                                ClipOp::Fill { shape, fill_rule } => {
                                    write_clip_def(
                                        backend,
                                        &mut defs,
                                        &clip_id,
                                        &shape,
                                        state.transform,
                                        fill_rule,
                                    );
                                }
                                ClipOp::Stroke { shape, style } => {
                                    write_stroke_clip_def(
                                        backend,
                                        &mut defs,
                                        &clip_id,
                                        &shape,
                                        &style,
                                        state.transform,
                                    );
                                }
                            }
                            let _ = write!(attrs, " clip-path=\"url(#{clip_id})\"");
                        }

                        if let Some(filter) = layer.filter {
                            filter_counter += 1;
                            let filter_id = format!("filter{filter_counter}");
                            write_filter_def(&mut defs, &filter_id, &filter, state.transform);
                            let _ = write!(attrs, " filter=\"url(#{filter_id})\"");
                        }

                        if let Some(opacity) = layer.opacity {
                            let opacity = opacity.clamp(0.0, 1.0);
                            if opacity < 1.0 {
                                let _ = write!(attrs, " opacity=\"{}\"", fmt_f32(opacity));
                            }
                        }

                        if let Some(blend) = layer.blend
                            && let Some(css) = blend_mode_css(&blend)
                        {
                            let _ = write!(attrs, " style=\"mix-blend-mode:{css}\"");
                        }

                        let _ = write!(body, "<g{attrs}>");
                        opened = true;
                    }

                    layer_stack.push(opened);
                }
                StateOp::PopLayer => {
                    let Some(opened) = layer_stack.pop() else {
                        panic!("PopLayer underflow in SVG backend");
                    };
                    if opened {
                        body.push_str("</g>");
                    }
                }
            },
            ImagingOp::Draw(draw_op) => {
                write_draw_op(backend, &mut defs, &mut body, &draw_op, &state);
            }
        }
    }

    while let Some(opened) = layer_stack.pop() {
        if opened {
            body.push_str("</g>");
        }
    }

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">"
    );
    if !defs.is_empty() {
        svg.push_str("<defs>");
        svg.push_str(&defs);
        svg.push_str("</defs>");
    }
    svg.push_str(&body);
    svg.push_str("</svg>");
    svg
}

fn write_clip_def(
    backend: &SvgBackend,
    defs: &mut String,
    id: &str,
    clip: &ClipShape,
    transform: Affine,
    fill_rule: FillRule,
) {
    let _ = write!(
        defs,
        "<clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\">"
    );
    match clip {
        ClipShape::Rect(rect) => {
            write_rect(defs, rect.x0, rect.y0, rect.x1, rect.y1, Some(transform));
        }
        ClipShape::RoundedRect(rr) => {
            write_rounded_rect_clip_def(defs, rr, transform);
        }
        ClipShape::Path(path_id) => {
            if let Some(Some(path)) = backend.paths.get(path_id.0 as usize) {
                let d = path_to_svg_d(path);
                let attrs = svg_transform_attr(transform);
                let _ = write!(
                    defs,
                    "<path d=\"{d}\"{attrs} clip-rule=\"{}\"/>",
                    fill_rule_svg(fill_rule)
                );
            }
        }
    }
    defs.push_str("</clipPath>");
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "SVG output is best-effort; filter parameter math uses f32 values"
)]
fn f64_to_f32(v: f64) -> f32 {
    v as f32
}

fn write_filter_def(defs: &mut String, id: &str, filter: &FilterDesc, transform: Affine) {
    // Use a generous region to avoid clipping blur/shadow output.
    let _ = write!(
        defs,
        "<filter id=\"{id}\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\">"
    );
    // Match Vello CPU’s semantics: filter parameters are specified in user space
    // and scaled using the current transform when the filter is applied.
    let [a, b, c, d, _e, _f] = transform.as_coeffs();
    let a = f64_to_f32(a);
    let b = f64_to_f32(b);
    let c = f64_to_f32(c);
    let d = f64_to_f32(d);

    let scale_x = (a * a + b * b).sqrt();
    let scale_y = (c * c + d * d).sqrt();
    match filter {
        FilterDesc::Flood { color } => {
            let (rgb, a) = color_to_svg(*color);
            let _ = write!(
                defs,
                "<feFlood flood-color=\"{rgb}\" flood-opacity=\"{}\"/>",
                fmt_f32(a)
            );
        }
        FilterDesc::Blur {
            std_deviation_x,
            std_deviation_y,
        } => {
            let _ = write!(
                defs,
                "<feGaussianBlur stdDeviation=\"{} {}\"/>",
                fmt_f32(*std_deviation_x * scale_x),
                fmt_f32(*std_deviation_y * scale_y)
            );
        }
        FilterDesc::DropShadow {
            dx,
            dy,
            std_deviation_x,
            std_deviation_y,
            color,
        } => {
            // Transform the offset vector by the current linear transform.
            let offset_x = dx * a + dy * c;
            let offset_y = dx * b + dy * d;
            let (rgb, a) = color_to_svg(*color);
            let _ = write!(
                defs,
                "<feDropShadow dx=\"{}\" dy=\"{}\" stdDeviation=\"{} {}\" flood-color=\"{rgb}\" flood-opacity=\"{}\"/>",
                fmt_f32(offset_x),
                fmt_f32(offset_y),
                fmt_f32(*std_deviation_x * scale_x),
                fmt_f32(*std_deviation_y * scale_y),
                fmt_f32(a),
            );
        }
        FilterDesc::Offset { dx, dy } => {
            // Transform the offset vector by the current linear transform.
            let offset_x = dx * a + dy * c;
            let offset_y = dx * b + dy * d;
            let _ = write!(
                defs,
                "<feOffset dx=\"{}\" dy=\"{}\"/>",
                fmt_f32(offset_x),
                fmt_f32(offset_y)
            );
        }
    }
    defs.push_str("</filter>");
}

fn write_rounded_rect_clip_def(defs: &mut String, rr: &RoundedRectF, transform: Affine) {
    if let Some(radius) = rr.radii.as_single_radius() {
        let rect = rr.rect;
        let w = rect.x1 - rect.x0;
        let h = rect.y1 - rect.y0;
        let attrs = svg_transform_attr(transform);
        let _ = write!(
            defs,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\"{attrs}/>",
            fmt_f32(rect.x0),
            fmt_f32(rect.y0),
            fmt_f32(w),
            fmt_f32(h),
            fmt_f32(radius),
            fmt_f32(radius),
        );
    } else {
        let outline = rr.to_kurbo().to_path(CLIP_TOLERANCE);
        let d = bez_path_to_svg_d(&outline);
        let attrs = svg_transform_attr(transform);
        let _ = write!(defs, "<path d=\"{d}\"{attrs} clip-rule=\"nonzero\"/>");
    }
}

fn svg_transform_attr(transform: Affine) -> String {
    let mut attrs = String::new();
    if transform != Affine::IDENTITY {
        let _ = write!(attrs, " transform=\"{}\"", affine_to_svg_matrix(transform));
    }
    attrs
}

fn fill_rule_svg(rule: FillRule) -> &'static str {
    match rule {
        FillRule::NonZero => "nonzero",
        FillRule::EvenOdd => "evenodd",
    }
}

fn path_desc_to_bezpath(desc: &PathDesc) -> BezPath {
    use peniko::kurbo::{PathEl, Point};

    let mut out = BezPath::new();
    for cmd in desc.commands.iter() {
        match *cmd {
            PathCmd::MoveTo { x, y } => {
                out.push(PathEl::MoveTo(Point::new(f64::from(x), f64::from(y))));
            }
            PathCmd::LineTo { x, y } => {
                out.push(PathEl::LineTo(Point::new(f64::from(x), f64::from(y))));
            }
            PathCmd::QuadTo { x1, y1, x, y } => {
                out.push(PathEl::QuadTo(
                    Point::new(f64::from(x1), f64::from(y1)),
                    Point::new(f64::from(x), f64::from(y)),
                ));
            }
            PathCmd::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                out.push(PathEl::CurveTo(
                    Point::new(f64::from(x1), f64::from(y1)),
                    Point::new(f64::from(x2), f64::from(y2)),
                    Point::new(f64::from(x), f64::from(y)),
                ));
            }
            PathCmd::Close => out.push(PathEl::ClosePath),
        }
    }
    out
}

fn bez_path_to_svg_d(path: &BezPath) -> String {
    use peniko::kurbo::PathEl;

    let mut d = String::new();
    for el in path.iter() {
        match el {
            PathEl::MoveTo(p) => {
                let _ = write!(d, "M{} {}", fmt_f64_to_f32(p.x), fmt_f64_to_f32(p.y));
            }
            PathEl::LineTo(p) => {
                let _ = write!(d, "L{} {}", fmt_f64_to_f32(p.x), fmt_f64_to_f32(p.y));
            }
            PathEl::QuadTo(p1, p2) => {
                let _ = write!(
                    d,
                    "Q{} {} {} {}",
                    fmt_f64_to_f32(p1.x),
                    fmt_f64_to_f32(p1.y),
                    fmt_f64_to_f32(p2.x),
                    fmt_f64_to_f32(p2.y)
                );
            }
            PathEl::CurveTo(p1, p2, p3) => {
                let _ = write!(
                    d,
                    "C{} {} {} {} {} {}",
                    fmt_f64_to_f32(p1.x),
                    fmt_f64_to_f32(p1.y),
                    fmt_f64_to_f32(p2.x),
                    fmt_f64_to_f32(p2.y),
                    fmt_f64_to_f32(p3.x),
                    fmt_f64_to_f32(p3.y)
                );
            }
            PathEl::ClosePath => d.push('Z'),
        }
    }
    d
}

fn write_stroke_clip_def(
    backend: &SvgBackend,
    defs: &mut String,
    id: &str,
    shape: &ClipShape,
    style: &StrokeStyle,
    transform: Affine,
) {
    let Some(outline) = stroke_outline_for_clip_shape(shape, style, CLIP_TOLERANCE, |id| {
        let Some(Some(path)) = backend.paths.get(id.0 as usize) else {
            return None;
        };
        Some(path_desc_to_bezpath(path))
    }) else {
        return;
    };

    let d = bez_path_to_svg_d(&outline);
    let attrs = svg_transform_attr(transform);
    let _ = write!(
        defs,
        "<clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\">"
    );
    let _ = write!(defs, "<path d=\"{d}\"{attrs} clip-rule=\"nonzero\"/>");
    defs.push_str("</clipPath>");
}

fn write_rect(out: &mut String, x0: f32, y0: f32, x1: f32, y1: f32, transform: Option<Affine>) {
    let w = x1 - x0;
    let h = y1 - y0;
    let mut attrs = String::new();
    if let Some(xf) = transform
        && xf != Affine::IDENTITY
    {
        let _ = write!(attrs, " transform=\"{}\"", affine_to_svg_matrix(xf));
    }
    let _ = write!(
        out,
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{attrs}/>",
        fmt_f32(x0),
        fmt_f32(y0),
        fmt_f32(w),
        fmt_f32(h),
    );
}

fn write_draw_op(
    backend: &SvgBackend,
    defs: &mut String,
    out: &mut String,
    op: &DrawOp,
    state: &SvgState,
) {
    match op {
        DrawOp::FillRect { x0, y0, x1, y1 } => {
            let style = style_for_paint(backend, state, PaintKind::Fill);
            let mut attrs = String::new();
            if state.transform != Affine::IDENTITY {
                let _ = write!(
                    attrs,
                    " transform=\"{}\"",
                    affine_to_svg_matrix(state.transform)
                );
            }
            let _ = write!(
                out,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{attrs}{style}/>",
                fmt_f32(*x0),
                fmt_f32(*y0),
                fmt_f32(x1 - x0),
                fmt_f32(y1 - y0),
            );
        }
        DrawOp::StrokeRect { x0, y0, x1, y1 } => {
            let style = style_for_paint(backend, state, PaintKind::Stroke);
            let mut attrs = String::new();
            if state.transform != Affine::IDENTITY {
                let _ = write!(
                    attrs,
                    " transform=\"{}\"",
                    affine_to_svg_matrix(state.transform)
                );
            }
            let _ = write!(
                out,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{attrs}{style}/>",
                fmt_f32(*x0),
                fmt_f32(*y0),
                fmt_f32(x1 - x0),
                fmt_f32(y1 - y0),
            );
        }
        DrawOp::FillPath(path_id) => {
            if let Some(Some(path)) = backend.paths.get(path_id.0 as usize) {
                let d = path_to_svg_d(path);
                let style = style_for_paint(backend, state, PaintKind::Fill);
                let mut attrs = String::new();
                if state.transform != Affine::IDENTITY {
                    let _ = write!(
                        attrs,
                        " transform=\"{}\"",
                        affine_to_svg_matrix(state.transform)
                    );
                }
                let _ = write!(
                    out,
                    "<path d=\"{d}\"{attrs} fill-rule=\"{}\"{style}/>",
                    fill_rule_svg(state.fill_rule)
                );
            }
        }
        DrawOp::StrokePath(path_id) => {
            if let Some(Some(path)) = backend.paths.get(path_id.0 as usize) {
                let d = path_to_svg_d(path);
                let style = style_for_paint(backend, state, PaintKind::Stroke);
                let mut attrs = String::new();
                if state.transform != Affine::IDENTITY {
                    let _ = write!(
                        attrs,
                        " transform=\"{}\"",
                        affine_to_svg_matrix(state.transform)
                    );
                }
                let _ = write!(out, "<path d=\"{d}\"{attrs}{style}/>");
            }
        }
        DrawOp::DrawImage {
            image, transform, ..
        } => {
            // Placeholder: draw a rect sized to the image and label it.
            let idx = image.0 as usize;
            if let Some(Some((desc, _pixels))) = backend.images.get(idx) {
                let w = desc.width as f32;
                let h = desc.height as f32;
                let xf = state.transform * (*transform);
                let _ = write!(out, "<g transform=\"{}\">", affine_to_svg_matrix(xf));
                out.push_str("<rect x=\"0\" y=\"0\" ");
                let _ = write!(
                    out,
                    "width=\"{}\" height=\"{}\" fill=\"#ff00ff\" fill-opacity=\"0.25\" stroke=\"#ff00ff\" stroke-width=\"1\"/>",
                    fmt_f32(w),
                    fmt_f32(h)
                );
                let _ = write!(
                    out,
                    "<text x=\"4\" y=\"14\" font-size=\"12\" fill=\"#ff00ff\">image#{}</text>",
                    image.0
                );
                out.push_str("</g>");
            }
        }
        DrawOp::DrawImageRect { image, dst, .. } => {
            let idx = image.0 as usize;
            if let Some(Some((_desc, _pixels))) = backend.images.get(idx) {
                let xf = state.transform;
                let _ = write!(out, "<g transform=\"{}\">", affine_to_svg_matrix(xf));
                let _ = write!(
                    out,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ff00ff\" fill-opacity=\"0.25\" stroke=\"#ff00ff\" stroke-width=\"1\"/>",
                    fmt_f32(dst.x0),
                    fmt_f32(dst.y0),
                    fmt_f32(dst.x1 - dst.x0),
                    fmt_f32(dst.y1 - dst.y0),
                );
                let _ = write!(
                    out,
                    "<text x=\"{}\" y=\"{}\" font-size=\"12\" fill=\"#ff00ff\">image#{} (rect)</text>",
                    fmt_f32(dst.x0 + 4.0),
                    fmt_f32(dst.y0 + 14.0),
                    image.0
                );
                out.push_str("</g>");
            }
        }
        DrawOp::DrawPicture { picture, transform } => {
            if let Some(Some(desc)) = backend.pictures.get(picture.0 as usize) {
                let nested_ops: Vec<_> = desc.recording.ops.iter().cloned().collect();
                // Render pictures with an isolated state (matching backend replay behavior),
                // applying the outer transform using an SVG group wrapper.
                let outer = state.transform * (*transform);
                let _ = write!(out, "<g transform=\"{}\">", affine_to_svg_matrix(outer));
                let nested = render_svg_fragment(backend, defs, &nested_ops);
                out.push_str(&nested);
                out.push_str("</g>");
            }
        }
    }
}

fn render_svg_fragment(backend: &SvgBackend, defs: &mut String, ops: &[ImagingOp]) -> String {
    let mut body = String::new();
    let mut layer_stack: Vec<bool> = Vec::new();
    let mut clip_counter: u64 = 10_000; // avoid collisions with outer doc ids.
    let mut filter_counter: u64 = 10_000; // avoid collisions with outer doc ids.
    let mut state = SvgState::default();

    for op in ops {
        match op.clone() {
            ImagingOp::State(state_op) => match state_op {
                StateOp::SetTransform(xf) => state.transform = xf,
                StateOp::SetPaintTransform(_xf) => {}
                StateOp::SetPaint(id) => state.paint = Some(id),
                StateOp::SetStroke(style) => state.stroke = Some(style),
                StateOp::SetFillRule(rule) => state.fill_rule = rule,
                StateOp::PushLayer(layer) => {
                    let mut opened = false;

                    if layer.clip.is_some() || layer.has_compositing_effects() {
                        let mut attrs = String::new();

                        if let Some(clip) = layer.clip {
                            clip_counter += 1;
                            let clip_id = format!("clip{clip_counter}");
                            match clip {
                                ClipOp::Fill { shape, fill_rule } => {
                                    write_clip_def(
                                        backend,
                                        defs,
                                        &clip_id,
                                        &shape,
                                        state.transform,
                                        fill_rule,
                                    );
                                }
                                ClipOp::Stroke { shape, style } => {
                                    write_stroke_clip_def(
                                        backend,
                                        defs,
                                        &clip_id,
                                        &shape,
                                        &style,
                                        state.transform,
                                    );
                                }
                            }
                            let _ = write!(attrs, " clip-path=\"url(#{clip_id})\"");
                        }

                        if let Some(filter) = layer.filter {
                            filter_counter += 1;
                            let filter_id = format!("filter{filter_counter}");
                            write_filter_def(defs, &filter_id, &filter, state.transform);
                            let _ = write!(attrs, " filter=\"url(#{filter_id})\"");
                        }

                        if let Some(opacity) = layer.opacity {
                            let opacity = opacity.clamp(0.0, 1.0);
                            if opacity < 1.0 {
                                let _ = write!(attrs, " opacity=\"{}\"", fmt_f32(opacity));
                            }
                        }

                        if let Some(blend) = layer.blend
                            && let Some(css) = blend_mode_css(&blend)
                        {
                            let _ = write!(attrs, " style=\"mix-blend-mode:{css}\"");
                        }

                        let _ = write!(body, "<g{attrs}>");
                        opened = true;
                    }

                    layer_stack.push(opened);
                }
                StateOp::PopLayer => {
                    let Some(opened) = layer_stack.pop() else {
                        panic!("PopLayer underflow in SVG backend");
                    };
                    if opened {
                        body.push_str("</g>");
                    }
                }
            },
            ImagingOp::Draw(draw_op) => {
                write_draw_op(backend, defs, &mut body, &draw_op, &state);
            }
        }
    }

    while let Some(opened) = layer_stack.pop() {
        if opened {
            body.push_str("</g>");
        }
    }
    body
}

#[derive(Copy, Clone)]
enum PaintKind {
    Fill,
    Stroke,
}

fn style_for_paint(backend: &SvgBackend, state: &SvgState, kind: PaintKind) -> String {
    let mut out = String::new();

    // Default fill/stroke values.
    match kind {
        PaintKind::Fill => out.push_str(" fill=\"#000000\" stroke=\"none\""),
        PaintKind::Stroke => out.push_str(" fill=\"none\" stroke=\"#000000\""),
    }

    if let Some(paint_id) = state.paint
        && let Some(Some(desc)) = backend.paints.get(paint_id.0 as usize)
    {
        match &desc.brush {
            Brush::Solid(color) => {
                let (rgb, a) = color_to_svg(*color);
                match kind {
                    PaintKind::Fill => {
                        let _ = write!(out, " fill=\"{rgb}\"");
                        if a < 1.0 {
                            let _ = write!(out, " fill-opacity=\"{}\"", fmt_f32(a));
                        }
                    }
                    PaintKind::Stroke => {
                        let _ = write!(out, " stroke=\"{rgb}\"");
                        if a < 1.0 {
                            let _ = write!(out, " stroke-opacity=\"{}\"", fmt_f32(a));
                        }
                    }
                }
            }
            _ => {
                // Fallback: keep defaults.
            }
        }
    }

    if let PaintKind::Stroke = kind
        && let Some(stroke) = state.stroke.as_ref()
    {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "SVG uses f32-like scalar formatting"
        )]
        let _ = write!(out, " stroke-width=\"{}\"", fmt_f32(stroke.width as f32));
        // SVG has a single linecap, while kurbo can specify start/end caps.
        // Use the start cap when they differ.
        let _ = write!(
            out,
            " stroke-linecap=\"{}\"",
            stroke_cap_svg(stroke.start_cap)
        );
        let _ = write!(out, " stroke-linejoin=\"{}\"", stroke_join_svg(stroke.join));
        if stroke.miter_limit.is_finite() && stroke.join == Join::Miter {
            let _ = write!(
                out,
                " stroke-miterlimit=\"{}\"",
                fmt_f32({
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "SVG uses f32-like scalar formatting"
                    )]
                    {
                        stroke.miter_limit as f32
                    }
                })
            );
        }
        if !stroke.dash_pattern.is_empty() {
            out.push_str(" stroke-dasharray=\"");
            for (i, v) in stroke.dash_pattern.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&fmt_f32({
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "SVG uses f32-like scalar formatting"
                    )]
                    {
                        *v as f32
                    }
                }));
            }
            out.push('"');
        }
        if stroke.dash_offset != 0.0 {
            let _ = write!(
                out,
                " stroke-dashoffset=\"{}\"",
                fmt_f32({
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "SVG uses f32-like scalar formatting"
                    )]
                    {
                        stroke.dash_offset as f32
                    }
                })
            );
        }
    }

    out
}

fn stroke_cap_svg(cap: Cap) -> &'static str {
    match cap {
        Cap::Butt => "butt",
        Cap::Round => "round",
        Cap::Square => "square",
    }
}

fn stroke_join_svg(join: Join) -> &'static str {
    match join {
        Join::Miter => "miter",
        Join::Round => "round",
        Join::Bevel => "bevel",
    }
}

fn blend_mode_css(mode: &BlendMode) -> Option<&'static str> {
    match (mode.mix, mode.compose) {
        (_, Compose::SrcOver) => match mode.mix {
            Mix::Normal => None,
            Mix::Multiply => Some("multiply"),
            Mix::Screen => Some("screen"),
            Mix::Overlay => Some("overlay"),
            Mix::Darken => Some("darken"),
            Mix::Lighten => Some("lighten"),
            Mix::ColorDodge => Some("color-dodge"),
            Mix::ColorBurn => Some("color-burn"),
            Mix::HardLight => Some("hard-light"),
            Mix::SoftLight => Some("soft-light"),
            Mix::Difference => Some("difference"),
            Mix::Exclusion => Some("exclusion"),
            Mix::Hue => Some("hue"),
            Mix::Saturation => Some("saturation"),
            Mix::Color => Some("color"),
            Mix::Luminosity => Some("luminosity"),
            _ => None,
        },
        // Composition modes don't map cleanly onto SVG. Keep them as "normal".
        _ => None,
    }
}

fn color_to_svg(color: Color) -> (String, f32) {
    let rgba = color.to_rgba8();
    let a = (rgba.a as f32) / 255.0;
    (format!("#{:02x}{:02x}{:02x}", rgba.r, rgba.g, rgba.b), a)
}

fn affine_to_svg_matrix(xf: Affine) -> String {
    // kurbo::Affine stores [a, b, c, d, e, f] corresponding to:
    // [ a c e ]
    // [ b d f ]
    // [ 0 0 1 ]
    let c = xf.as_coeffs();
    format!(
        "matrix({} {} {} {} {} {})",
        fmt_f64_to_f32(c[0]),
        fmt_f64_to_f32(c[1]),
        fmt_f64_to_f32(c[2]),
        fmt_f64_to_f32(c[3]),
        fmt_f64_to_f32(c[4]),
        fmt_f64_to_f32(c[5]),
    )
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "SVG uses f32-like scalar formatting"
)]
fn fmt_f64_to_f32(v: f64) -> String {
    fmt_f32(v as f32)
}

fn path_to_svg_d(path: &PathDesc) -> String {
    let mut d = String::new();
    for cmd in path.commands.iter() {
        match *cmd {
            PathCmd::MoveTo { x, y } => {
                let _ = write!(d, "M{} {}", fmt_f32(x), fmt_f32(y));
            }
            PathCmd::LineTo { x, y } => {
                let _ = write!(d, "L{} {}", fmt_f32(x), fmt_f32(y));
            }
            PathCmd::QuadTo { x1, y1, x, y } => {
                let _ = write!(
                    d,
                    "Q{} {} {} {}",
                    fmt_f32(x1),
                    fmt_f32(y1),
                    fmt_f32(x),
                    fmt_f32(y)
                );
            }
            PathCmd::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let _ = write!(
                    d,
                    "C{} {} {} {} {} {}",
                    fmt_f32(x1),
                    fmt_f32(y1),
                    fmt_f32(x2),
                    fmt_f32(y2),
                    fmt_f32(x),
                    fmt_f32(y)
                );
            }
            PathCmd::Close => {
                d.push('Z');
            }
        }
    }
    d
}

fn fmt_f32(v: f32) -> String {
    // Keep output readable and stable enough for debugging.
    if v.is_finite() {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "best-effort pretty formatting"
        )]
        let i = v as i32;
        let diff = (i as f32) - v;
        if diff > -1e-6 && diff < 1e-6 {
            return format!("{i}");
        }
    } else {
        return format!("{v}");
    }

    let mut s = format!("{:.3}", v);
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use peniko::Color;
    use understory_imaging::ImagingBackendExt;
    use understory_imaging::{PaintDesc, StateOp};

    #[test]
    fn exports_basic_svg() {
        let mut backend = SvgBackend::default();
        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(255, 0, 0, 255)),
        });
        backend.state(StateOp::SetPaint(paint));
        backend.draw(DrawOp::FillRect {
            x0: 10.0,
            y0: 20.0,
            x1: 30.0,
            y1: 40.0,
        });
        let svg = backend.to_svg(100, 80);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("fill=\"#ff0000\""));
    }

    #[test]
    fn exports_offset_filter() {
        let mut backend = SvgBackend::default();
        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 0, 0, 255)),
        });

        backend.with_filter_layer(FilterDesc::offset(5.0, -3.0), |backend| {
            backend.state(StateOp::SetPaint(paint));
            backend.draw(DrawOp::FillRect {
                x0: 10.0,
                y0: 20.0,
                x1: 30.0,
                y1: 40.0,
            });
        });

        let svg = backend.to_svg(100, 80);
        assert!(svg.contains("<feOffset dx=\"5\" dy=\"-3\"/>"));
    }
}
