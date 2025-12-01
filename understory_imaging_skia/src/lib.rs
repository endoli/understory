// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Skia backend implementation of the `understory_imaging` IR.
//!
//! This crate provides a thin adapter that maps the backend-agnostic
//! imaging operations defined in `understory_imaging` onto a Skia canvas,
//! using the `skia-safe` wrapper crate.

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;
use kurbo::Affine;
use peniko::Brush;
use peniko::InterpolationAlphaSpace;
use peniko::color::{ColorSpaceTag, HueDirection};
use skia_safe as sk;
use understory_imaging::{
    DrawOp, ImageDesc, ImageId, ImagingBackend, ImagingOp, PaintDesc, PaintId, PathDesc, PathId,
    PictureDesc, PictureId, RecordedOps, ResourceBackend, StateOp, TransformClass,
};

/// Skia-backed implementation of the imaging backend.
///
/// This type owns resource tables that mirror the handle-based resources in
/// `understory_imaging` and forwards draw/state operations into a Skia canvas.
pub struct SkiaImagingBackend<'a> {
    canvas: &'a sk::Canvas,

    paths: Vec<Option<sk::Path>>,
    images: Vec<Option<sk::Image>>,
    paints: Vec<Option<PaintDesc>>,
    pictures: Vec<Option<PictureDesc>>,

    current_transform: Affine,
    current_paint_transform: Affine,
    current_brush: Brush,
    current_stroke: Option<kurbo::Stroke>,
    current_blend: understory_imaging::BlendMode,
    current_opacity: f32,
    clip_depth: u32,

    /// Buffered imaging ops captured between `begin_record`/`end_record`.
    recording_ops: Vec<ImagingOp>,
    /// Whether recording is currently active.
    recording_active: bool,
}

impl fmt::Debug for SkiaImagingBackend<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SkiaImagingBackend { .. }")
    }
}

impl<'a> SkiaImagingBackend<'a> {
    /// Create a new backend that renders into the given canvas.
    pub fn new(canvas: &'a sk::Canvas) -> Self {
        Self {
            canvas,
            paths: Vec::new(),
            images: Vec::new(),
            paints: Vec::new(),
            pictures: Vec::new(),
            current_transform: Affine::IDENTITY,
            current_paint_transform: Affine::IDENTITY,
            current_brush: Brush::Solid(peniko::Color::BLACK),
            current_stroke: None,
            current_blend: understory_imaging::BlendMode::default(),
            current_opacity: 1.0,
            clip_depth: 0,
            recording_ops: Vec::new(),
            recording_active: false,
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "alpha values are explicitly clamped to [0, 255] before casting"
)]
fn alpha_to_u8(a: f32) -> u8 {
    (a * 255.0).round().clamp(0.0, 255.0) as u8
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "Skia APIs consume f32; truncation from f64 geometry is acceptable"
)]
fn f64_to_f32(v: f64) -> f32 {
    v as f32
}

fn affine_to_matrix(xf: Affine) -> sk::Matrix {
    let a = xf.as_coeffs();
    // kurbo::Affine stores [a, b, c, d, e, f] as:
    // [sx, ky, kx, sy, tx, ty] in row-major form:
    // [a c e]
    // [b d f]
    // [0 0 1]
    //
    // Skia uses:
    // [sx kx tx]
    // [ky sy ty]
    // [p0 p1 p2]
    sk::Matrix::new_all(
        f64_to_f32(a[0]),
        f64_to_f32(a[2]),
        f64_to_f32(a[4]),
        f64_to_f32(a[1]),
        f64_to_f32(a[3]),
        f64_to_f32(a[5]),
        0.0,
        0.0,
        1.0,
    )
}

fn brush_to_paint(brush: &Brush, opacity: f32, paint_xf: Affine) -> sk::Paint {
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    let alpha_scale = opacity.clamp(0.0, 1.0);

    match brush {
        Brush::Solid(color) => {
            let rgba = color.to_rgba8();
            let (r, g, b, a) = (rgba.r, rgba.g, rgba.b, rgba.a);
            let a_scaled = alpha_to_u8((a as f32 / 255.0) * alpha_scale);
            paint.set_color(skia_safe::Color::from_argb(a_scaled, r, g, b));
        }
        Brush::Gradient(grad) => {
            // Map peniko gradients to Skia gradient shaders, honoring
            // interpolation color space and hue direction.
            let stops = grad.stops.as_ref();
            if stops.is_empty() {
                paint.set_color(skia_safe::Color::TRANSPARENT);
                return paint;
            }

            let mut colors: Vec<sk::Color4f> = Vec::with_capacity(stops.len());
            let mut pos: Vec<f32> = Vec::with_capacity(stops.len());

            for s in stops {
                // Use the dynamic color components directly and apply additional
                // opacity as an alpha multiplier.
                let comps = s.color.components;
                let a = comps[3] * alpha_scale;
                colors.push(skia_safe::Color4f::new(comps[0], comps[1], comps[2], a));
                pos.push(s.offset.clamp(0.0, 1.0));
            }

            let tile_mode = tile_mode_from_extend(grad.extend);

            let local = affine_to_matrix(paint_xf);

            let interpolation = skia_safe::gradient_shader::Interpolation {
                color_space: gradient_shader_cs_from_cs_tag(grad.interpolation_cs),
                in_premul: match grad.interpolation_alpha_space {
                    InterpolationAlphaSpace::Premultiplied => {
                        skia_safe::gradient_shader::interpolation::InPremul::Yes
                    }
                    InterpolationAlphaSpace::Unpremultiplied => {
                        skia_safe::gradient_shader::interpolation::InPremul::No
                    }
                },
                hue_method: gradient_shader_hue_method_from_hue_direction(grad.hue_direction),
            };

            match grad.kind {
                peniko::GradientKind::Linear(line) => {
                    let p0 = sk::Point::new(f64_to_f32(line.start.x), f64_to_f32(line.start.y));
                    let p1 = sk::Point::new(f64_to_f32(line.end.x), f64_to_f32(line.end.y));
                    if let Some(shader) = sk::Shader::linear_gradient_with_interpolation(
                        (p0, p1),
                        (&colors[..], None),
                        &pos[..],
                        tile_mode,
                        interpolation,
                        &Some(local),
                    ) {
                        paint.set_shader(shader);
                    }
                }
                peniko::GradientKind::Radial(rad) => {
                    let start_center = sk::Point::new(
                        f64_to_f32(rad.start_center.x),
                        f64_to_f32(rad.start_center.y),
                    );
                    let start_radius = rad.start_radius;
                    let end_center =
                        sk::Point::new(f64_to_f32(rad.end_center.x), f64_to_f32(rad.end_center.y));
                    let end_radius = rad.end_radius;

                    let shader = if start_center == end_center && start_radius == end_radius {
                        sk::Shader::radial_gradient_with_interpolation(
                            (start_center, start_radius),
                            (&colors[..], None),
                            &pos[..],
                            tile_mode,
                            interpolation,
                            &Some(local),
                        )
                    } else {
                        sk::Shader::two_point_conical_gradient_with_interpolation(
                            (start_center, start_radius),
                            (end_center, end_radius),
                            (&colors[..], None),
                            &pos[..],
                            tile_mode,
                            interpolation,
                            &Some(local),
                        )
                    };

                    if let Some(shader) = shader {
                        paint.set_shader(shader);
                    }
                }
                peniko::GradientKind::Sweep(sweep) => {
                    let center =
                        sk::Point::new(f64_to_f32(sweep.center.x), f64_to_f32(sweep.center.y));
                    let angles = (sweep.start_angle.to_degrees(), sweep.end_angle.to_degrees());
                    if let Some(shader) = sk::Shader::sweep_gradient_with_interpolation(
                        center,
                        (&colors[..], None),
                        &pos[..],
                        tile_mode,
                        angles,
                        interpolation,
                        &Some(local),
                    ) {
                        paint.set_shader(shader);
                    }
                }
            }

            // If shader creation failed for any reason, fall back to the last stop.
            if paint.shader().is_none() {
                if let Some(last_stop) = stops.last() {
                    let color = last_stop
                        .color
                        .to_alpha_color::<peniko::color::Srgb>()
                        .multiply_alpha(alpha_scale);
                    let rgba = color.to_rgba8();
                    paint.set_color(skia_safe::Color::from_argb(rgba.a, rgba.r, rgba.g, rgba.b));
                }
            }
        }
        // Image brushes are not yet mapped; fall back to solid black with opacity.
        Brush::Image(_) => {
            paint.set_color(skia_safe::Color::from_argb(
                alpha_to_u8(alpha_scale),
                0,
                0,
                0,
            ));
        }
    }

    paint
}

fn tile_mode_from_extend(extend: peniko::Extend) -> sk::TileMode {
    match extend {
        peniko::Extend::Pad => sk::TileMode::Clamp,
        peniko::Extend::Repeat => sk::TileMode::Repeat,
        peniko::Extend::Reflect => sk::TileMode::Mirror,
    }
}

fn gradient_shader_cs_from_cs_tag(
    color_space: ColorSpaceTag,
) -> skia_safe::gradient_shader::interpolation::ColorSpace {
    use skia_safe::gradient_shader::interpolation::ColorSpace as SkGradientShaderColorSpace;

    match color_space {
        ColorSpaceTag::Srgb => SkGradientShaderColorSpace::SRGB,
        ColorSpaceTag::LinearSrgb => SkGradientShaderColorSpace::SRGBLinear,
        ColorSpaceTag::Lab => SkGradientShaderColorSpace::Lab,
        ColorSpaceTag::Lch => SkGradientShaderColorSpace::LCH,
        ColorSpaceTag::Hsl => SkGradientShaderColorSpace::HSL,
        ColorSpaceTag::Hwb => SkGradientShaderColorSpace::HWB,
        ColorSpaceTag::Oklab => SkGradientShaderColorSpace::OKLab,
        ColorSpaceTag::Oklch => SkGradientShaderColorSpace::OKLCH,
        ColorSpaceTag::DisplayP3 => SkGradientShaderColorSpace::DisplayP3,
        ColorSpaceTag::A98Rgb => SkGradientShaderColorSpace::A98RGB,
        ColorSpaceTag::ProphotoRgb => SkGradientShaderColorSpace::ProphotoRGB,
        ColorSpaceTag::Rec2020 => SkGradientShaderColorSpace::Rec2020,
        _ => SkGradientShaderColorSpace::SRGB,
    }
}

fn gradient_shader_hue_method_from_hue_direction(
    direction: HueDirection,
) -> skia_safe::gradient_shader::interpolation::HueMethod {
    use skia_safe::gradient_shader::interpolation::HueMethod as SkGradientShaderHueMethod;

    match direction {
        HueDirection::Shorter => SkGradientShaderHueMethod::Shorter,
        HueDirection::Longer => SkGradientShaderHueMethod::Longer,
        HueDirection::Increasing => SkGradientShaderHueMethod::Increasing,
        HueDirection::Decreasing => SkGradientShaderHueMethod::Decreasing,
        _ => SkGradientShaderHueMethod::Shorter,
    }
}

fn map_blend_mode(mode: &understory_imaging::BlendMode) -> sk::BlendMode {
    use peniko::{Compose, Mix};

    match (mode.mix, mode.compose) {
        // Composition takes precedence when it is not the default SrcOver.
        (_, Compose::Clear) => sk::BlendMode::Clear,
        (_, Compose::Copy) => sk::BlendMode::Src,
        (_, Compose::Dest) => sk::BlendMode::Dst,
        (_, Compose::SrcOver) => match mode.mix {
            Mix::Normal => sk::BlendMode::SrcOver,
            Mix::Multiply => sk::BlendMode::Multiply,
            Mix::Screen => sk::BlendMode::Screen,
            Mix::Overlay => sk::BlendMode::Overlay,
            Mix::Darken => sk::BlendMode::Darken,
            Mix::Lighten => sk::BlendMode::Lighten,
            Mix::ColorDodge => sk::BlendMode::ColorDodge,
            Mix::ColorBurn => sk::BlendMode::ColorBurn,
            Mix::HardLight => sk::BlendMode::HardLight,
            Mix::SoftLight => sk::BlendMode::SoftLight,
            Mix::Difference => sk::BlendMode::Difference,
            Mix::Exclusion => sk::BlendMode::Exclusion,
            Mix::Hue => sk::BlendMode::Hue,
            Mix::Saturation => sk::BlendMode::Saturation,
            Mix::Color => sk::BlendMode::Color,
            Mix::Luminosity => sk::BlendMode::Luminosity,
            #[allow(
                deprecated,
                reason = "Mix::Clip is mapped to SrcOver for now; kept for completeness"
            )]
            Mix::Clip => sk::BlendMode::SrcOver,
        },
        (_, Compose::DestOver) => sk::BlendMode::DstOver,
        (_, Compose::SrcIn) => sk::BlendMode::SrcIn,
        (_, Compose::DestIn) => sk::BlendMode::DstIn,
        (_, Compose::SrcOut) => sk::BlendMode::SrcOut,
        (_, Compose::DestOut) => sk::BlendMode::DstOut,
        (_, Compose::SrcAtop) => sk::BlendMode::SrcATop,
        (_, Compose::DestAtop) => sk::BlendMode::DstATop,
        (_, Compose::Xor) => sk::BlendMode::Xor,
        (_, Compose::Plus) => sk::BlendMode::Plus,
        // Approximate PlusLighter with Plus.
        (_, Compose::PlusLighter) => sk::BlendMode::Plus,
    }
}

impl ResourceBackend for SkiaImagingBackend<'_> {
    fn create_path(&mut self, desc: PathDesc) -> PathId {
        let mut path = sk::Path::new();
        for cmd in desc.commands.iter() {
            match *cmd {
                understory_imaging::PathCmd::MoveTo { x, y } => {
                    path.move_to((x, y));
                }
                understory_imaging::PathCmd::LineTo { x, y } => {
                    path.line_to((x, y));
                }
                understory_imaging::PathCmd::QuadTo { x1, y1, x, y } => {
                    path.quad_to((x1, y1), (x, y));
                }
                understory_imaging::PathCmd::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    path.cubic_to((x1, y1), (x2, y2), (x, y));
                }
                understory_imaging::PathCmd::Close => {
                    path.close();
                }
            }
        }
        let id = u32::try_from(self.paths.len())
            .expect("SkiaImagingBackend: too many paths for u32 PathId");
        self.paths.push(Some(path));
        PathId(id)
    }

    fn destroy_path(&mut self, id: PathId) {
        let idx = id.0 as usize;
        if let Some(slot) = self.paths.get_mut(idx) {
            *slot = None;
        }
    }

    fn create_image(&mut self, desc: ImageDesc, pixels: &[u8]) -> ImageId {
        // Assume RGBA8 premultiplied-alpha pixels.
        let info = sk::ImageInfo::new(
            (desc.width as i32, desc.height as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        let data = sk::Data::new_copy(pixels);
        let row_bytes = desc.width as usize * 4;
        let image = sk::images::raster_from_data(&info, data, row_bytes);
        let id = u32::try_from(self.images.len())
            .expect("SkiaImagingBackend: too many images for u32 ImageId");
        self.images.push(image);
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
            .expect("SkiaImagingBackend: too many paints for u32 PaintId");
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
            .expect("SkiaImagingBackend: too many pictures for u32 PictureId");
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

impl ImagingBackend for SkiaImagingBackend<'_> {
    fn state(&mut self, op: StateOp) {
        if self.recording_active {
            self.recording_ops.push(ImagingOp::State(op.clone()));
        }
        match op {
            StateOp::SetTransform(xf) => {
                self.current_transform = xf;
                let m = affine_to_matrix(xf);
                self.canvas.reset_matrix();
                self.canvas.concat(&m);
            }
            StateOp::SetPaintTransform(xf) => {
                self.current_paint_transform = xf;
            }
            StateOp::SetClip(shape) => match shape {
                understory_imaging::ClipShape::Infinite => {
                    // Restore any active clips back to the prior state.
                    while self.clip_depth > 0 {
                        self.canvas.restore();
                        self.clip_depth -= 1;
                    }
                }
                understory_imaging::ClipShape::Rect { x0, y0, x1, y1 } => {
                    let rect = sk::Rect::new(x0, y0, x1, y1);
                    self.canvas.save();
                    self.canvas.clip_rect(rect, None, true);
                    self.clip_depth = self.clip_depth.saturating_add(1);
                }
                understory_imaging::ClipShape::RoundedRect {
                    x0,
                    y0,
                    x1,
                    y1,
                    radius_x,
                    radius_y,
                } => {
                    let rect = sk::Rect::new(x0, y0, x1, y1);
                    let mut path = sk::Path::new();
                    path.add_round_rect(rect, (radius_x, radius_y), None);
                    self.canvas.save();
                    self.canvas.clip_path(&path, None, true);
                    self.clip_depth = self.clip_depth.saturating_add(1);
                }
                understory_imaging::ClipShape::Path(id) => {
                    if let Some(Some(path)) = self.paths.get(id.0 as usize) {
                        self.canvas.save();
                        self.canvas.clip_path(path, None, true);
                        self.clip_depth = self.clip_depth.saturating_add(1);
                    }
                }
            },
            StateOp::SetPaint(id) => {
                if let Some(Some(desc)) = self.paints.get(id.0 as usize) {
                    self.current_brush = desc.brush.clone();
                }
            }
            StateOp::SetStroke(style) => {
                self.current_stroke = Some(style);
            }
            StateOp::SetBlendMode(mode) => {
                self.current_blend = mode;
            }
            StateOp::SetOpacity(value) => {
                self.current_opacity = value;
            }
            StateOp::BeginGroup { blend, opacity } => {
                // Begin a compositing group using a saveLayer so that the
                // group's blend mode and opacity are applied when restoring.
                let sk_blend = map_blend_mode(&blend);
                let mut paint = sk::Paint::default();
                paint.set_blend_mode(sk_blend);
                paint.set_alpha_f(opacity.clamp(0.0, 1.0));

                let bounds = sk::Rect::new(-10_000.0, -10_000.0, 10_000.0, 10_000.0);
                let mut rec = sk::canvas::SaveLayerRec::default();
                rec = rec.bounds(&bounds);
                rec = rec.paint(&paint);
                self.canvas.save_layer(&rec);
            }
            StateOp::EndGroup => {
                self.canvas.restore();
            }
        }
    }

    fn draw(&mut self, op: DrawOp) {
        if self.recording_active {
            self.recording_ops.push(ImagingOp::Draw(op.clone()));
        }
        match op {
            DrawOp::FillPath(id) => {
                if let Some(Some(path)) = self.paths.get(id.0 as usize) {
                    let mut paint = brush_to_paint(
                        &self.current_brush,
                        self.current_opacity,
                        self.current_paint_transform,
                    );
                    paint.set_style(skia_safe::PaintStyle::Fill);
                    // Apply the current blend mode when not inside a group.
                    paint.set_blend_mode(map_blend_mode(&self.current_blend));

                    self.canvas.draw_path(path, &paint);
                }
            }
            DrawOp::FillRect { x0, y0, x1, y1 } => {
                let rect = sk::Rect::new(x0, y0, x1, y1);
                let mut paint = brush_to_paint(
                    &self.current_brush,
                    self.current_opacity,
                    self.current_paint_transform,
                );
                paint.set_style(skia_safe::PaintStyle::Fill);
                paint.set_blend_mode(map_blend_mode(&self.current_blend));
                self.canvas.draw_rect(rect, &paint);
            }
            DrawOp::StrokePath(id) => {
                if let (Some(Some(path)), Some(stroke)) =
                    (self.paths.get(id.0 as usize), self.current_stroke.clone())
                {
                    let mut paint = brush_to_paint(
                        &self.current_brush,
                        self.current_opacity,
                        self.current_paint_transform,
                    );
                    paint.set_style(skia_safe::PaintStyle::Stroke);
                    paint.set_stroke_width(f64_to_f32(stroke.width));
                    paint.set_blend_mode(map_blend_mode(&self.current_blend));

                    self.canvas.draw_path(path, &paint);
                }
            }
            DrawOp::StrokeRect { x0, y0, x1, y1 } => {
                if let Some(stroke) = self.current_stroke.clone() {
                    let rect = sk::Rect::new(x0, y0, x1, y1);
                    let mut paint = brush_to_paint(
                        &self.current_brush,
                        self.current_opacity,
                        self.current_paint_transform,
                    );
                    paint.set_style(skia_safe::PaintStyle::Stroke);
                    paint.set_stroke_width(f64_to_f32(stroke.width));
                    paint.set_blend_mode(map_blend_mode(&self.current_blend));
                    self.canvas.draw_rect(rect, &paint);
                }
            }
            DrawOp::DrawImage { image, transform } => {
                let idx = image.0 as usize;
                if let Some(Some(img)) = self.images.get(idx) {
                    self.canvas.save();
                    // Apply only the image-local transform; current_transform
                    // has already been applied via `SetTransform`.
                    let m = affine_to_matrix(transform);
                    self.canvas.concat(&m);
                    // Draw at the origin; the transform places it. Apply
                    // current opacity and blend mode via a temporary paint.
                    let mut paint = sk::Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_alpha_f(self.current_opacity.clamp(0.0, 1.0));
                    paint.set_blend_mode(map_blend_mode(&self.current_blend));
                    self.canvas.draw_image(img, (0.0, 0.0), Some(&paint));
                    self.canvas.restore();
                }
            }
            DrawOp::DrawPicture { picture, transform } => {
                let idx = picture.0 as usize;
                if let Some(Some(desc)) = self.pictures.get_mut(idx) {
                    // Preferred path: use a cached picture-local Skia picture
                    // built at record-time, then apply the outer transform
                    // when drawing.
                    if let Some(accel_any) = desc.recording.acceleration.as_ref() {
                        if desc.recording.can_reuse(transform) {
                            if let Some(picture) = accel_any.downcast_ref::<sk::Picture>() {
                                self.canvas.save();
                                self.canvas.reset_matrix();
                                let m = affine_to_matrix(transform);
                                self.canvas.concat(&m);
                                self.canvas.draw_picture(picture, None, None);
                                self.canvas.restore();
                                return;
                            }
                        }
                    }

                    // Fallback: no usable acceleration (e.g., recording was
                    // created by another backend). Replay the IR directly into
                    // this backend, applying the outer transform to any
                    // SetTransform ops.
                    let saved_transform = self.current_transform;
                    let saved_stroke = self.current_stroke.clone();
                    let saved_brush = self.current_brush.clone();
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

        // Build a picture-local Skia picture by replaying the captured ops
        // into a PictureRecorder with identity transform.
        let mut recorder = sk::PictureRecorder::new();
        let cull = sk::Rect::new(-100_000.0, -100_000.0, 100_000.0, 100_000.0);
        let rec_canvas = recorder.begin_recording(cull, false);

        {
            let mut sub_backend = SkiaImagingBackend::new(rec_canvas);
            // Share resource tables; nested pictures are currently ignored
            // for acceleration.
            sub_backend.paths = self.paths.clone();
            sub_backend.images = self.images.clone();
            sub_backend.paints = self.paints.clone();
            sub_backend.pictures = Vec::new();

            let ops_vec: Vec<_> = self.recording_ops.clone();
            for op in ops_vec {
                match op {
                    ImagingOp::State(s) => sub_backend.state(s),
                    ImagingOp::Draw(d) => sub_backend.draw(d),
                }
            }
        }

        let acceleration: Option<Box<dyn Any>> = recorder
            .finish_recording_as_picture(None)
            .map(|p| Box::new(p) as Box<dyn Any>);

        RecordedOps {
            ops: ops_arc,
            acceleration,
            // Picture-local Skia pictures are valid under any affine transform;
            // the outer transform is applied at DrawPicture time.
            valid_under: TransformClass::Affine,
            original_ctm: Some(Affine::IDENTITY),
        }
    }
}
