// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_imaging_web_canvas --heading-base-level=0

//! Web Canvas (2D) backend for the Understory imaging IR.
//!
//! This crate provides an [`ImagingBackend`] implementation backed by
//! `web_sys::CanvasRenderingContext2d` when targeting `wasm32`.
//!
//! # Usage
//!
//! Prefer `WebCanvasImagingBackend::new_html_canvas` when you have an `HtmlCanvasElement`.
//! It enables correct isolated layer compositing for `LayerOp { opacity, blend }` by allocating
//! temporary scratch canvases on-demand. If you only have a `CanvasRenderingContext2d`, use
//! `WebCanvasImagingBackend::new` (best-effort for group compositing).
//!
//! ```no_run
//! #[cfg(target_arch = "wasm32")]
//! fn make_backend(
//!     canvas: web_sys::HtmlCanvasElement,
//! ) -> Result<understory_imaging_web_canvas::WebCanvasImagingBackend, wasm_bindgen::JsValue> {
//!     understory_imaging_web_canvas::WebCanvasImagingBackend::new_html_canvas(canvas)
//! }
//! ```
//!
//! Notes:
//! - This is an early, best-effort renderer intended for debugging and prototyping.
//! - Supported: paths and axis-aligned rects; solid-color fills/strokes; stroke styles including
//!   dashes/caps/joins; clip layers (including stroke clips and fill-rule aware path clips).
//! - Supported: linear and radial gradients (best-effort; Canvas 2D does not support all Peniko
//!   gradient features, such as extend modes beyond pad).
//! - Placeholders: images and sweep gradients.
//! - Correct group opacity/blend (isolated layers) is only available when constructed from an
//!   `HtmlCanvasElement`; the context-only constructor is best-effort for group compositing.
//!   Canvas 2D does not have a built-in “isolated group” primitive, so matching the IR semantics
//!   requires rendering into a temporary offscreen buffer and compositing once at `PopLayer`.
//! - Performance: isolated layers allocate a scratch `<canvas>` on-demand. This is intended for
//!   prototyping and debugging, not production rendering.

#![no_std]

extern crate alloc;

use understory_imaging::{
    DrawOp, ImageDesc, ImageId, ImagingBackend, PaintDesc, PaintId, PathDesc, PathId, PictureDesc,
    PictureId, RecordedOps, ResourceBackend, StateOp,
};

#[cfg(target_arch = "wasm32")]
use alloc::{
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
#[cfg(target_arch = "wasm32")]
use core::fmt;
#[cfg(target_arch = "wasm32")]
use kurbo::{Affine, Cap, Join};
#[cfg(target_arch = "wasm32")]
use peniko::Brush;
#[cfg(target_arch = "wasm32")]
use understory_imaging::{
    BlendMode, ClipOp, FillRule, FilterDesc, ImagingOp, LayerOp, PathCmd, StrokeStyle,
    TransformClass, clip_shape_to_bez_path, stroke_outline_for_clip_shape,
};

#[cfg(target_arch = "wasm32")]
use js_sys::Array;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use web_sys::{CanvasGradient, CanvasRenderingContext2d, CanvasWindingRule, HtmlCanvasElement};

#[cfg(target_arch = "wasm32")]
fn affine_to_canvas(xf: Affine) -> [f64; 6] {
    xf.as_coeffs()
}

#[cfg(target_arch = "wasm32")]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Canvas 2D filter parameters operate in f32-like space; truncation is acceptable."
)]
fn f64_to_f32(v: f64) -> f32 {
    v as f32
}

#[cfg(target_arch = "wasm32")]
fn fill_rule_to_canvas(rule: FillRule) -> CanvasWindingRule {
    match rule {
        FillRule::NonZero => CanvasWindingRule::Nonzero,
        FillRule::EvenOdd => CanvasWindingRule::Evenodd,
    }
}

#[cfg(target_arch = "wasm32")]
fn color_to_css(color: peniko::Color) -> String {
    // `Rgba8` formats as a CSS `rgb(...)`/`rgba(...)` string.
    color.to_rgba8().to_string()
}

#[cfg(target_arch = "wasm32")]
fn apply_stroke_style(ctx: &CanvasRenderingContext2d, style: &StrokeStyle) {
    ctx.set_line_width(style.width);
    ctx.set_miter_limit(style.miter_limit);

    let cap = match style.start_cap {
        Cap::Butt => "butt",
        Cap::Round => "round",
        Cap::Square => "square",
    };
    ctx.set_line_cap(cap);

    let join = match style.join {
        Join::Bevel => "bevel",
        Join::Miter => "miter",
        Join::Round => "round",
    };
    ctx.set_line_join(join);

    if !style.dash_pattern.is_empty() {
        let dash = Array::new();
        for v in style.dash_pattern.iter().copied() {
            dash.push(&JsValue::from_f64(v));
        }
        let _ = ctx.set_line_dash(&dash);
        ctx.set_line_dash_offset(style.dash_offset);
    } else {
        let dash = Array::new();
        let _ = ctx.set_line_dash(&dash);
        ctx.set_line_dash_offset(0.0);
    }
}

#[cfg(target_arch = "wasm32")]
fn dynamic_color_to_css(color: peniko::color::DynamicColor) -> String {
    let color = color.to_alpha_color::<peniko::color::Srgb>();
    color_to_css(color)
}

#[cfg(target_arch = "wasm32")]
fn begin_path_from_desc(ctx: &CanvasRenderingContext2d, desc: &PathDesc) {
    ctx.begin_path();
    for cmd in desc.commands.iter() {
        match *cmd {
            PathCmd::MoveTo { x, y } => ctx.move_to(f64::from(x), f64::from(y)),
            PathCmd::LineTo { x, y } => ctx.line_to(f64::from(x), f64::from(y)),
            PathCmd::QuadTo { x1, y1, x, y } => {
                ctx.quadratic_curve_to(f64::from(x1), f64::from(y1), f64::from(x), f64::from(y));
            }
            PathCmd::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => ctx.bezier_curve_to(
                f64::from(x1),
                f64::from(y1),
                f64::from(x2),
                f64::from(y2),
                f64::from(x),
                f64::from(y),
            ),
            PathCmd::Close => ctx.close_path(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn begin_path_from_bez(ctx: &CanvasRenderingContext2d, path: &kurbo::BezPath) {
    ctx.begin_path();
    for el in path.elements() {
        use kurbo::PathEl;
        match *el {
            PathEl::MoveTo(p) => ctx.move_to(p.x, p.y),
            PathEl::LineTo(p) => ctx.line_to(p.x, p.y),
            PathEl::QuadTo(p1, p) => ctx.quadratic_curve_to(p1.x, p1.y, p.x, p.y),
            PathEl::CurveTo(p1, p2, p) => ctx.bezier_curve_to(p1.x, p1.y, p2.x, p2.y, p.x, p.y),
            PathEl::ClosePath => ctx.close_path(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn path_desc_to_bez(desc: &PathDesc) -> kurbo::BezPath {
    let mut p = kurbo::BezPath::new();
    for cmd in desc.commands.iter() {
        match *cmd {
            PathCmd::MoveTo { x, y } => p.move_to((f64::from(x), f64::from(y))),
            PathCmd::LineTo { x, y } => p.line_to((f64::from(x), f64::from(y))),
            PathCmd::QuadTo { x1, y1, x, y } => {
                p.quad_to((f64::from(x1), f64::from(y1)), (f64::from(x), f64::from(y)));
            }
            PathCmd::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => p.curve_to(
                (f64::from(x1), f64::from(y1)),
                (f64::from(x2), f64::from(y2)),
                (f64::from(x), f64::from(y)),
            ),
            PathCmd::Close => p.close_path(),
        }
    }
    p
}

#[cfg(target_arch = "wasm32")]
fn map_blend_mode(mode: &BlendMode) -> Option<&'static str> {
    // Best-effort mapping to canvas `globalCompositeOperation`.
    // Note: canvas supports a subset of Porter-Duff + blend modes.
    use peniko::{Compose, Mix};
    let (mix, compose) = (mode.mix, mode.compose);
    match (mix, compose) {
        (Mix::Normal, Compose::SrcOver) => Some("source-over"),
        (Mix::Normal, Compose::SrcIn) => Some("source-in"),
        (Mix::Normal, Compose::SrcOut) => Some("source-out"),
        (Mix::Normal, Compose::SrcAtop) => Some("source-atop"),
        (Mix::Normal, Compose::DestOver) => Some("destination-over"),
        (Mix::Normal, Compose::DestIn) => Some("destination-in"),
        (Mix::Normal, Compose::DestOut) => Some("destination-out"),
        (Mix::Normal, Compose::DestAtop) => Some("destination-atop"),
        (Mix::Normal, Compose::Xor) => Some("xor"),
        (Mix::Normal, Compose::Plus) => Some("lighter"),
        (Mix::Multiply, Compose::SrcOver) => Some("multiply"),
        (Mix::Screen, Compose::SrcOver) => Some("screen"),
        (Mix::Overlay, Compose::SrcOver) => Some("overlay"),
        (Mix::Darken, Compose::SrcOver) => Some("darken"),
        (Mix::Lighten, Compose::SrcOver) => Some("lighten"),
        (Mix::ColorDodge, Compose::SrcOver) => Some("color-dodge"),
        (Mix::ColorBurn, Compose::SrcOver) => Some("color-burn"),
        (Mix::HardLight, Compose::SrcOver) => Some("hard-light"),
        (Mix::SoftLight, Compose::SrcOver) => Some("soft-light"),
        (Mix::Difference, Compose::SrcOver) => Some("difference"),
        (Mix::Exclusion, Compose::SrcOver) => Some("exclusion"),
        _ => None,
    }
}

#[cfg(target_arch = "wasm32")]
const CLIP_TOLERANCE: f64 = 0.1;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug)]
enum LayerEntry {
    Noop,
    Direct,
    Isolated {
        parent_ctx: CanvasRenderingContext2d,
        scratch_canvas: HtmlCanvasElement,
        opacity: f32,
        blend: Option<BlendMode>,
        filter: Option<FilterDesc>,
        /// Transform active when the layer was created (for scaling filter params).
        layer_transform: Affine,
    },
}

/// Web Canvas backend (only available on `wasm32`).
#[cfg(target_arch = "wasm32")]
pub struct WebCanvasImagingBackend {
    ctx: CanvasRenderingContext2d,
    root_canvas: Option<HtmlCanvasElement>,

    paths: Vec<Option<PathDesc>>,
    images: Vec<Option<(ImageDesc, Vec<u8>)>>,
    paints: Vec<Option<PaintDesc>>,
    pictures: Vec<Option<PictureDesc>>,

    ops: Vec<ImagingOp>,
    recording_start: Option<usize>,
    replaying_picture: bool,
    layer_stack: Vec<LayerEntry>,

    current_transform: Affine,
    current_paint_transform: Affine,
    current_brush: Brush,
    current_stroke: Option<StrokeStyle>,
    current_fill_rule: FillRule,
}

#[cfg(target_arch = "wasm32")]
impl fmt::Debug for WebCanvasImagingBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WebCanvasImagingBackend { .. }")
    }
}

#[cfg(target_arch = "wasm32")]
impl WebCanvasImagingBackend {
    /// Create a backend that draws into the given canvas 2D context.
    ///
    /// This constructor cannot allocate scratch canvases, so `LayerOp { opacity, blend }`
    /// uses a best-effort approximation (per-draw `globalAlpha`/`globalCompositeOperation`).
    pub fn new(ctx: CanvasRenderingContext2d) -> Self {
        Self {
            ctx,
            root_canvas: None,
            paths: Vec::new(),
            images: Vec::new(),
            paints: Vec::new(),
            pictures: Vec::new(),
            ops: Vec::new(),
            recording_start: None,
            replaying_picture: false,
            layer_stack: Vec::new(),
            current_transform: Affine::IDENTITY,
            current_paint_transform: Affine::IDENTITY,
            current_brush: Brush::Solid(peniko::Color::BLACK),
            current_stroke: None,
            current_fill_rule: FillRule::NonZero,
        }
    }

    /// Create a backend for a DOM canvas element.
    ///
    /// This enables correct isolated layer compositing for `LayerOp { opacity, blend }` by
    /// allocating temporary scratch canvases on-demand.
    pub fn new_html_canvas(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let ctx = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("missing 2d context"))?
            .dyn_into::<CanvasRenderingContext2d>()?;
        Ok(Self {
            ctx,
            root_canvas: Some(canvas),
            paths: Vec::new(),
            images: Vec::new(),
            paints: Vec::new(),
            pictures: Vec::new(),
            ops: Vec::new(),
            recording_start: None,
            replaying_picture: false,
            layer_stack: Vec::new(),
            current_transform: Affine::IDENTITY,
            current_paint_transform: Affine::IDENTITY,
            current_brush: Brush::Solid(peniko::Color::BLACK),
            current_stroke: None,
            current_fill_rule: FillRule::NonZero,
        })
    }

    fn apply_transform(&self) {
        let [a, b, c, d, e, f] = affine_to_canvas(self.current_transform);
        // set_transform(a, b, c, d, e, f)
        let _ = self.ctx.set_transform(a, b, c, d, e, f);
    }

    fn apply_paint_as_fill(&self) {
        match &self.current_brush {
            Brush::Solid(color) => {
                let css = color_to_css(*color);
                self.ctx.set_fill_style_str(&css);
            }
            Brush::Gradient(gradient) => {
                if let Some(grad) = self.create_canvas_gradient(gradient) {
                    self.ctx.set_fill_style_canvas_gradient(grad.as_ref());
                } else {
                    self.ctx.set_fill_style_str("#ff00ff");
                }
            }
            Brush::Image(_image) => {
                self.ctx.set_fill_style_str("#ff00ff");
            }
        }
    }

    fn apply_paint_as_stroke(&self) {
        match &self.current_brush {
            Brush::Solid(color) => {
                let css = color_to_css(*color);
                self.ctx.set_stroke_style_str(&css);
            }
            Brush::Gradient(gradient) => {
                if let Some(grad) = self.create_canvas_gradient(gradient) {
                    self.ctx.set_stroke_style_canvas_gradient(grad.as_ref());
                } else {
                    self.ctx.set_stroke_style_str("#ff00ff");
                }
            }
            Brush::Image(_image) => {
                self.ctx.set_stroke_style_str("#ff00ff");
            }
        }
    }

    fn create_canvas_gradient(&self, gradient: &peniko::Gradient) -> Option<CanvasGradient> {
        let saved = self.current_transform;
        let combined = self.current_transform * self.current_paint_transform;
        let [a, b, c, d, e, f] = affine_to_canvas(combined);
        let _ = self.ctx.set_transform(a, b, c, d, e, f);

        let grad = match gradient.kind {
            peniko::GradientKind::Linear(pos) => {
                let x0 = pos.start.x;
                let y0 = pos.start.y;
                let x1 = pos.end.x;
                let y1 = pos.end.y;
                self.ctx.create_linear_gradient(x0, y0, x1, y1)
            }
            peniko::GradientKind::Radial(pos) => {
                let x0 = pos.start_center.x;
                let y0 = pos.start_center.y;
                let r0 = f64::from(pos.start_radius);
                let x1 = pos.end_center.x;
                let y1 = pos.end_center.y;
                let r1 = f64::from(pos.end_radius);
                self.ctx
                    .create_radial_gradient(x0, y0, r0, x1, y1, r1)
                    .ok()?
            }
            peniko::GradientKind::Sweep(_pos) => {
                // Canvas 2D conic gradients are not currently wired up in this backend.
                let [a, b, c, d, e, f] = affine_to_canvas(saved);
                let _ = self.ctx.set_transform(a, b, c, d, e, f);
                return None;
            }
        };

        let stops = gradient.stops.as_slice();
        if stops.is_empty() {
            let _ = grad.add_color_stop(0.0, "rgba(0, 0, 0, 0)");
            let _ = grad.add_color_stop(1.0, "rgba(0, 0, 0, 0)");
        } else {
            for stop in stops {
                let offset = stop.offset.clamp(0.0, 1.0);
                if !offset.is_finite() {
                    continue;
                }
                let css = dynamic_color_to_css(stop.color);
                let _ = grad.add_color_stop(offset, &css);
            }
        }

        // Canvas gradients are best-effort: Peniko supports repeat/reflect extend and multiple
        // interpolation modes that Canvas 2D does not expose.
        let [a, b, c, d, e, f] = affine_to_canvas(saved);
        let _ = self.ctx.set_transform(a, b, c, d, e, f);
        Some(grad)
    }

    fn bez_path_for_id(&self, id: PathId) -> Option<kurbo::BezPath> {
        let idx = id.0 as usize;
        let desc = self.paths.get(idx)?.as_ref()?;
        Some(path_desc_to_bez(desc))
    }

    fn scratch_size(&self) -> Option<(u32, u32)> {
        let canvas = self.root_canvas.as_ref()?;
        Some((canvas.width(), canvas.height()))
    }

    fn push_layer(&mut self, layer: LayerOp) {
        if layer.is_noop() {
            self.layer_stack.push(LayerEntry::Noop);
            return;
        }

        let needs_isolation = layer.has_compositing_effects();

        if needs_isolation && self.root_canvas.is_some() && self.push_isolated_layer(&layer) {
            return;
        }

        self.push_direct_layer(layer);
    }

    fn push_direct_layer(&mut self, layer: LayerOp) {
        // Fallback / clip-only: rely on save/restore and per-draw canvas state.
        self.ctx.save();

        self.apply_layer_composite(layer.blend, layer.opacity);

        if let Some(filter) = layer.filter.as_ref() {
            // Best-effort: without an isolated scratch canvas, Canvas 2D applies filters
            // per draw operation rather than to the group as a whole.
            let (filter_str, _) = self.filter_to_canvas_composite(filter, self.current_transform);
            if let Some(filter_str) = filter_str {
                self.ctx.set_filter(&filter_str);
            }
        }

        if let Some(clip) = layer.clip
            && !self.apply_layer_clip(&self.ctx, &clip)
        {
            // Keep the save() balanced even if the clip couldn't be constructed.
            self.ctx.restore();
            self.layer_stack.push(LayerEntry::Direct);
            return;
        }

        self.layer_stack.push(LayerEntry::Direct);
    }

    fn apply_layer_composite(&self, blend: Option<BlendMode>, opacity: Option<f32>) {
        if let Some(blend) = blend.and_then(|m| map_blend_mode(&m)) {
            let _ = self.ctx.set_global_composite_operation(blend);
        }
        if let Some(opacity) = opacity {
            let parent_alpha = self.ctx.global_alpha();
            self.ctx
                .set_global_alpha(parent_alpha * f64::from(opacity.clamp(0.0, 1.0)));
        }
    }

    fn apply_layer_clip(&self, ctx: &CanvasRenderingContext2d, clip: &ClipOp) -> bool {
        match clip {
            ClipOp::Fill { shape, fill_rule } => {
                let Some(path) =
                    clip_shape_to_bez_path(shape, CLIP_TOLERANCE, |id| self.bez_path_for_id(id))
                else {
                    return false;
                };
                begin_path_from_bez(ctx, &path);
                let rule = fill_rule_to_canvas(*fill_rule);
                ctx.clip_with_canvas_winding_rule(rule);
                true
            }
            ClipOp::Stroke { shape, style } => {
                let Some(outline) =
                    stroke_outline_for_clip_shape(shape, style, CLIP_TOLERANCE, |id| {
                        self.bez_path_for_id(id)
                    })
                else {
                    return false;
                };
                begin_path_from_bez(ctx, &outline);
                ctx.clip();
                true
            }
        }
    }

    fn push_isolated_layer(&mut self, layer: &LayerOp) -> bool {
        let Some((width, height)) = self.scratch_size() else {
            return false;
        };
        let Some(root) = self.root_canvas.as_ref() else {
            return false;
        };

        let doc = web_sys::window()
            .and_then(|w| w.document())
            .or_else(|| root.owner_document())
            .expect("window/document available for HtmlCanvasElement");

        let el = doc
            .create_element("canvas")
            .expect("create <canvas> element");
        let scratch_canvas = el
            .dyn_into::<HtmlCanvasElement>()
            .expect("canvas element is HtmlCanvasElement");
        scratch_canvas.set_width(width);
        scratch_canvas.set_height(height);

        let scratch_ctx = scratch_canvas
            .get_context("2d")
            .expect("get_context('2d')")
            .expect("2d context")
            .dyn_into::<CanvasRenderingContext2d>()
            .expect("CanvasRenderingContext2d");

        // Clear to transparent.
        let _ = scratch_ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        scratch_ctx.clear_rect(0.0, 0.0, f64::from(width), f64::from(height));

        // Prepare child ctx with current state.
        scratch_ctx.save();
        scratch_ctx.set_global_alpha(1.0);
        let _ = scratch_ctx.set_global_composite_operation("source-over");
        let [a, b, c, d, e, f] = affine_to_canvas(self.current_transform);
        let _ = scratch_ctx.set_transform(a, b, c, d, e, f);
        if let Some(style) = self.current_stroke.as_ref() {
            apply_stroke_style(&scratch_ctx, style);
        }

        if let Some(clip) = layer.clip.as_ref()
            && !self.apply_layer_clip(&scratch_ctx, clip)
        {
            scratch_ctx.restore();
            return false;
        }

        let parent_ctx = self.ctx.clone();
        self.ctx = scratch_ctx;
        self.layer_stack.push(LayerEntry::Isolated {
            parent_ctx,
            scratch_canvas,
            opacity: layer.opacity.unwrap_or(1.0).clamp(0.0, 1.0),
            blend: layer.blend,
            filter: layer.filter.clone(),
            layer_transform: self.current_transform,
        });
        true
    }

    fn filter_to_canvas_composite(
        &self,
        filter: &FilterDesc,
        transform: Affine,
    ) -> (Option<String>, (f64, f64)) {
        // Understory filter parameters are specified in user space. Canvas 2D applies
        // `ctx.filter` in device space, so we scale parameters using the layer transform.
        let [a, b, c, d, _e, _f] = transform.as_coeffs();
        let a = f64_to_f32(a);
        let b = f64_to_f32(b);
        let c = f64_to_f32(c);
        let d = f64_to_f32(d);

        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();

        match *filter {
            FilterDesc::Flood { .. } => {
                // Flood is handled specially for isolated layers so it can respect the layer clip.
                (None, (0.0, 0.0))
            }
            FilterDesc::Blur {
                std_deviation_x,
                std_deviation_y,
            } => {
                // Canvas 2D only supports a uniform blur; pick the max as a best-effort.
                let sigma = (std_deviation_x * scale_x)
                    .max(std_deviation_y * scale_y)
                    .max(0.0);
                (Some(format!("blur({sigma}px)")), (0.0, 0.0))
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
                let sigma = (std_deviation_x * scale_x)
                    .max(std_deviation_y * scale_y)
                    .max(0.0);
                let css = color_to_css(color);
                let s = format!(
                    "drop-shadow({}px {}px {}px {css})",
                    offset_x, offset_y, sigma
                );
                (Some(s), (0.0, 0.0))
            }
            FilterDesc::Offset { dx, dy } => {
                // Canvas 2D doesn't have an offset filter, but because we already render to an
                // isolated scratch canvas we can implement it by translating the composite step.
                let offset_x = dx * a + dy * c;
                let offset_y = dx * b + dy * d;
                (None, (f64::from(offset_x), f64::from(offset_y)))
            }
        }
    }

    fn pop_isolated_layer(&mut self, entry: LayerEntry) {
        let LayerEntry::Isolated {
            parent_ctx,
            scratch_canvas,
            opacity,
            blend,
            filter,
            layer_transform,
        } = entry
        else {
            return;
        };

        if let Some(FilterDesc::Flood { color }) = filter.as_ref() {
            // Apply flood while the scratch ctx is still active (and still has the layer clip).
            let css = color_to_css(*color);
            let w = f64::from(scratch_canvas.width());
            let h = f64::from(scratch_canvas.height());
            let _ = self.ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
            self.ctx.clear_rect(0.0, 0.0, w, h);
            self.ctx.set_fill_style_str(&css);
            self.ctx.fill_rect(0.0, 0.0, w, h);
        }

        // Close the scratch ctx save() we opened at push.
        self.ctx.restore();

        // Switch back to parent and composite the scratch result once.
        self.ctx = parent_ctx;

        self.ctx.save();
        let _ = self.ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

        if let Some(blend) = blend.and_then(|m| map_blend_mode(&m)) {
            let _ = self.ctx.set_global_composite_operation(blend);
        }

        let parent_alpha = self.ctx.global_alpha();
        self.ctx.set_global_alpha(parent_alpha * f64::from(opacity));

        let (canvas_filter, (dx, dy)) = match filter.as_ref() {
            Some(FilterDesc::Flood { .. }) => (None, (0.0, 0.0)),
            Some(f) => self.filter_to_canvas_composite(f, layer_transform),
            None => (None, (0.0, 0.0)),
        };

        // Canvas 2D filter is stateful; set it only for the composite draw.
        {
            if let Some(filter) = canvas_filter.as_deref() {
                self.ctx.set_filter(filter);
            }
        }
        let _ = self
            .ctx
            .draw_image_with_html_canvas_element(&scratch_canvas, dx, dy);
        {
            if canvas_filter.is_some() {
                self.ctx.set_filter("none");
            }
        }
        self.ctx.restore();

        // Ensure the parent context reflects the current state after drawing into a scratch
        // context (e.g. SetTransform/SetStroke inside the layer).
        self.apply_transform();
        if let Some(style) = self.current_stroke.as_ref() {
            apply_stroke_style(&self.ctx, style);
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl ResourceBackend for WebCanvasImagingBackend {
    fn create_path(&mut self, desc: PathDesc) -> PathId {
        let id = u32::try_from(self.paths.len())
            .expect("WebCanvasImagingBackend: too many paths for u32 PathId");
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
            .expect("WebCanvasImagingBackend: too many images for u32 ImageId");
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
            .expect("WebCanvasImagingBackend: too many paints for u32 PaintId");
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
            .expect("WebCanvasImagingBackend: too many pictures for u32 PictureId");
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

#[cfg(target_arch = "wasm32")]
impl ImagingBackend for WebCanvasImagingBackend {
    fn state(&mut self, op: StateOp) {
        if !self.replaying_picture {
            self.ops.push(ImagingOp::State(op.clone()));
        }
        match op {
            StateOp::SetTransform(xf) => {
                self.current_transform = xf;
                self.apply_transform();
            }
            StateOp::SetPaintTransform(xf) => {
                self.current_paint_transform = xf;
            }
            StateOp::PushLayer(layer) => self.push_layer(layer),
            StateOp::PopLayer => {
                match self.layer_stack.pop() {
                    Some(LayerEntry::Noop) => {}
                    Some(LayerEntry::Direct) => {
                        self.ctx.restore();
                        self.apply_transform();
                        if let Some(style) = self.current_stroke.as_ref() {
                            apply_stroke_style(&self.ctx, style);
                        }
                    }
                    Some(entry @ LayerEntry::Isolated { .. }) => {
                        self.pop_isolated_layer(entry);
                    }
                    None => {
                        // Keep behavior consistent with other backends.
                        panic!("PopLayer with empty stack");
                    }
                }
            }
            StateOp::SetPaint(id) => {
                if let Some(Some(desc)) = self.paints.get(id.0 as usize) {
                    self.current_brush = desc.brush.clone();
                }
            }
            StateOp::SetStroke(style) => {
                // Apply to the canvas now so subsequent strokes use this.
                apply_stroke_style(&self.ctx, &style);
                self.current_stroke = Some(style);
            }
            StateOp::SetFillRule(rule) => {
                self.current_fill_rule = rule;
            }
        }
    }

    fn draw(&mut self, op: DrawOp) {
        if !self.replaying_picture {
            self.ops.push(ImagingOp::Draw(op.clone()));
        }

        match op {
            DrawOp::FillPath(id) => {
                let idx = id.0 as usize;
                let Some(Some(desc)) = self.paths.get(idx) else {
                    return;
                };
                self.apply_paint_as_fill();
                begin_path_from_desc(&self.ctx, desc);
                match self.current_fill_rule {
                    FillRule::NonZero => self.ctx.fill(),
                    FillRule::EvenOdd => {
                        self.ctx
                            .fill_with_canvas_winding_rule(CanvasWindingRule::Evenodd);
                    }
                }
            }
            DrawOp::StrokePath(id) => {
                let idx = id.0 as usize;
                let Some(Some(desc)) = self.paths.get(idx) else {
                    return;
                };
                self.apply_paint_as_stroke();
                begin_path_from_desc(&self.ctx, desc);
                self.ctx.stroke();
            }
            DrawOp::FillRect { x0, y0, x1, y1 } => {
                self.apply_paint_as_fill();
                let w = x1 - x0;
                let h = y1 - y0;
                self.ctx
                    .fill_rect(f64::from(x0), f64::from(y0), f64::from(w), f64::from(h));
            }
            DrawOp::StrokeRect { x0, y0, x1, y1 } => {
                self.apply_paint_as_stroke();
                let w = x1 - x0;
                let h = y1 - y0;
                self.ctx
                    .stroke_rect(f64::from(x0), f64::from(y0), f64::from(w), f64::from(h));
            }
            DrawOp::DrawImage {
                image, transform, ..
            } => {
                let idx = image.0 as usize;
                if self.images.get(idx).and_then(|x| x.as_ref()).is_none() {
                    return;
                }
                // Placeholder: draw a semi-transparent rect showing the image bounds.
                self.ctx.save();
                let saved = self.current_transform;
                self.current_transform = saved * transform;
                self.apply_transform();
                self.ctx.set_fill_style_str("#ff00ff");
                self.ctx.set_global_alpha(0.25);
                self.ctx.fill_rect(0.0, 0.0, 32.0, 32.0);
                self.ctx.restore();
                self.current_transform = saved;
                self.apply_transform();
            }
            DrawOp::DrawImageRect { image, dst, .. } => {
                let idx = image.0 as usize;
                if self.images.get(idx).and_then(|x| x.as_ref()).is_none() {
                    return;
                }
                self.ctx.set_fill_style_str("#ff00ff");
                self.ctx.set_global_alpha(0.25);
                let w = dst.x1 - dst.x0;
                let h = dst.y1 - dst.y0;
                self.ctx.fill_rect(
                    f64::from(dst.x0),
                    f64::from(dst.y0),
                    f64::from(w),
                    f64::from(h),
                );
            }
            DrawOp::DrawPicture { picture, transform } => {
                let idx = picture.0 as usize;
                let Some(Some(desc)) = self.pictures.get(idx) else {
                    return;
                };

                // Replay with isolated state, applying the outer transform to SetTransform.
                let saved_transform = self.current_transform;
                let saved_fill_rule = self.current_fill_rule;
                let saved_stroke = self.current_stroke.clone();
                let saved_brush = self.current_brush.clone();
                let saved_paint_transform = self.current_paint_transform;

                let ops: Vec<_> = desc.recording.ops.to_vec();
                let saved_replaying = self.replaying_picture;
                self.replaying_picture = true;

                for op in ops {
                    match op {
                        ImagingOp::State(StateOp::SetTransform(xf)) => {
                            self.state(StateOp::SetTransform(transform * xf));
                        }
                        ImagingOp::State(s) => self.state(s),
                        ImagingOp::Draw(d) => self.draw(d),
                    }
                }

                self.replaying_picture = saved_replaying;
                self.current_transform = saved_transform;
                self.current_fill_rule = saved_fill_rule;
                self.current_stroke = saved_stroke;
                self.current_brush = saved_brush;
                self.current_paint_transform = saved_paint_transform;
                self.apply_transform();
            }
        }
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

/// Stub type for non-wasm targets so the crate can be included in the workspace.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default)]
pub struct WebCanvasImagingBackend;

#[cfg(not(target_arch = "wasm32"))]
impl ResourceBackend for WebCanvasImagingBackend {
    fn create_path(&mut self, _desc: PathDesc) -> PathId {
        unimplemented!("WebCanvasImagingBackend is only available on wasm32")
    }
    fn destroy_path(&mut self, _id: PathId) {}
    fn create_image(&mut self, _desc: ImageDesc, _pixels: &[u8]) -> ImageId {
        unimplemented!("WebCanvasImagingBackend is only available on wasm32")
    }
    fn destroy_image(&mut self, _id: ImageId) {}
    fn create_paint(&mut self, _desc: PaintDesc) -> PaintId {
        unimplemented!("WebCanvasImagingBackend is only available on wasm32")
    }
    fn destroy_paint(&mut self, _id: PaintId) {}
    fn create_picture(&mut self, _desc: PictureDesc) -> PictureId {
        unimplemented!("WebCanvasImagingBackend is only available on wasm32")
    }
    fn destroy_picture(&mut self, _id: PictureId) {}
}

#[cfg(not(target_arch = "wasm32"))]
impl ImagingBackend for WebCanvasImagingBackend {
    fn state(&mut self, _op: StateOp) {
        unimplemented!("WebCanvasImagingBackend is only available on wasm32")
    }
    fn draw(&mut self, _op: DrawOp) {
        unimplemented!("WebCanvasImagingBackend is only available on wasm32")
    }
    fn begin_record(&mut self) {
        unimplemented!("WebCanvasImagingBackend is only available on wasm32")
    }
    fn end_record(&mut self) -> RecordedOps {
        unimplemented!("WebCanvasImagingBackend is only available on wasm32")
    }
}
