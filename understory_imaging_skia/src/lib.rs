// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_imaging_skia --heading-base-level=0

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
    BlendMode, ClipOp, ClipShape, DrawOp, FillRule, FilterDesc, ImageDesc, ImageId, ImagingBackend,
    ImagingOp, LayerOp, PaintDesc, PaintId, PathCmd, PathDesc, PathId, PictureDesc, PictureId,
    RecordedOps, RectF, ResourceBackend, RoundedRectF, StateOp, TransformClass,
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
    current_fill_rule: FillRule,
    stack: Vec<StackEntry>,

    /// Buffered imaging ops captured between `begin_record`/`end_record`.
    recording_ops: Vec<ImagingOp>,
    /// Whether recording is currently active.
    recording_active: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum StackEntry {
    Noop,
    /// Number of canvas restore operations needed for a single `PushLayer`.
    Restore(u8),
}

fn rectf_to_sk_rect(rect: RectF) -> sk::Rect {
    sk::Rect::new(rect.x0, rect.y0, rect.x1, rect.y1)
}

fn rounded_rectf_to_sk_rrect(rr: RoundedRectF) -> sk::RRect {
    let rect = rectf_to_sk_rect(rr.rect);
    sk::RRect::new_rect_radii(
        rect,
        &[
            sk::Vector::new(rr.radii.top_left, rr.radii.top_left),
            sk::Vector::new(rr.radii.top_right, rr.radii.top_right),
            sk::Vector::new(rr.radii.bottom_right, rr.radii.bottom_right),
            sk::Vector::new(rr.radii.bottom_left, rr.radii.bottom_left),
        ],
    )
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
            current_fill_rule: FillRule::NonZero,
            stack: Vec::new(),
            recording_ops: Vec::new(),
            recording_active: false,
        }
    }

    fn push_layer(&mut self, layer: LayerOp) {
        let mut restores = 0_u8;

        restores += self.push_clip(layer.clip);
        restores += self.push_effect_layer(layer.blend, layer.opacity, layer.filter);

        if restores == 0 {
            self.stack.push(StackEntry::Noop);
        } else {
            self.stack.push(StackEntry::Restore(restores));
        }
    }

    fn push_clip(&mut self, clip: Option<ClipOp>) -> u8 {
        let Some(clip) = clip else {
            return 0;
        };
        let Some(path) = self.clip_path(clip) else {
            return 0;
        };
        self.canvas.save();
        self.canvas.clip_path(&path, None, true);
        1
    }

    fn clip_path(&self, clip: ClipOp) -> Option<sk::Path> {
        match clip {
            ClipOp::Fill { shape, fill_rule } => {
                clip_shape_to_sk_path(&self.paths, &shape, Some(fill_rule))
            }
            ClipOp::Stroke { shape, style } => {
                let src = clip_shape_to_sk_path(&self.paths, &shape, None)?;
                let mut paint = sk::Paint::default();
                apply_stroke_style(&mut paint, &style);

                let mut dst = sk::Path::new();
                let _ok = skia_safe::path_utils::fill_path_with_paint(
                    &src,
                    &paint,
                    &mut dst,
                    None::<&sk::Rect>,
                    None::<sk::Matrix>,
                );
                Some(dst)
            }
        }
    }

    fn push_effect_layer(
        &mut self,
        blend: Option<BlendMode>,
        opacity: Option<f32>,
        filter: Option<FilterDesc>,
    ) -> u8 {
        let mut paint = sk::Paint::default();
        let mut needs_save_layer = false;

        if blend.is_some() || opacity.is_some() {
            let blend = blend.unwrap_or_default();
            let opacity = opacity.unwrap_or(1.0).clamp(0.0, 1.0);

            let sk_blend = map_blend_mode(&blend);
            paint.set_blend_mode(sk_blend);
            paint.set_alpha_f(opacity);
            needs_save_layer = true;
        }

        if let Some(filter) = filter.and_then(|f| self.build_filter(f)) {
            paint.set_image_filter(filter);
            needs_save_layer = true;
        }

        if !needs_save_layer {
            return 0;
        }

        let bounds = sk::Rect::new(-10_000.0, -10_000.0, 10_000.0, 10_000.0);
        let mut rec = sk::canvas::SaveLayerRec::default();
        rec = rec.bounds(&bounds);
        rec = rec.paint(&paint);
        self.canvas.save_layer(&rec);
        1
    }

    fn build_filter(&self, filter: FilterDesc) -> Option<skia_safe::ImageFilter> {
        use skia_safe::image_filters;

        // Match Vello CPU’s semantics: filter parameters are specified in user space
        // and scaled using the current transform when the filter is applied.
        let [a, b, c, d, _e, _f] = self.current_transform.as_coeffs();
        let a = f64_to_f32(a);
        let b = f64_to_f32(b);
        let c = f64_to_f32(c);
        let d = f64_to_f32(d);

        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();

        match filter {
            FilterDesc::Flood { color } => {
                let shader = sk::shaders::color(color_to_sk_color(color));
                image_filters::shader(shader, None)
            }
            FilterDesc::Blur {
                std_deviation_x,
                std_deviation_y,
            } => image_filters::blur(
                (std_deviation_x * scale_x, std_deviation_y * scale_y),
                None,
                None,
                None,
            ),
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

                let sk_color = color_to_sk_color4f(color);
                image_filters::drop_shadow(
                    (offset_x, offset_y),
                    (std_deviation_x * scale_x, std_deviation_y * scale_y),
                    sk_color,
                    None,
                    None,
                    None,
                )
            }
            FilterDesc::Offset { dx, dy } => {
                // Transform the offset vector by the current linear transform.
                let offset_x = dx * a + dy * c;
                let offset_y = dx * b + dy * d;
                image_filters::offset((offset_x, offset_y), None, None)
            }
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
            paint.set_color(color_to_sk_color(color.multiply_alpha(alpha_scale)));
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
            if paint.shader().is_none()
                && let Some(last_stop) = stops.last()
            {
                let color = last_stop
                    .color
                    .to_alpha_color::<peniko::color::Srgb>()
                    .multiply_alpha(alpha_scale);
                paint.set_color(color_to_sk_color(color));
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

fn color_to_sk_color(color: peniko::Color) -> sk::Color {
    let rgba = color.to_rgba8();
    sk::Color::from_argb(rgba.a, rgba.r, rgba.g, rgba.b)
}

fn color_to_sk_color4f(color: peniko::Color) -> sk::Color4f {
    let comps = color.components;
    sk::Color4f::new(comps[0], comps[1], comps[2], comps[3])
}

fn tile_mode_from_extend(extend: peniko::Extend) -> sk::TileMode {
    match extend {
        peniko::Extend::Pad => sk::TileMode::Clamp,
        peniko::Extend::Repeat => sk::TileMode::Repeat,
        peniko::Extend::Reflect => sk::TileMode::Mirror,
    }
}

fn sampling_options_from_image_sampler(sampler: &peniko::ImageSampler) -> sk::SamplingOptions {
    use peniko::ImageQuality;
    use skia_safe::{FilterMode, SamplingOptions};

    match sampler.quality {
        ImageQuality::Low => SamplingOptions::from(FilterMode::Nearest),
        // Skia supports cubic resamplers, but v0 imaging doesn't require matching
        // any specific high-quality kernel; treat High as Linear for now.
        ImageQuality::Medium | ImageQuality::High => SamplingOptions::from(FilterMode::Linear),
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

fn map_blend_mode(mode: &BlendMode) -> sk::BlendMode {
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

fn sk_path_fill_type_from_fill_rule(rule: FillRule) -> sk::PathFillType {
    match rule {
        FillRule::NonZero => sk::PathFillType::Winding,
        FillRule::EvenOdd => sk::PathFillType::EvenOdd,
    }
}

fn path_with_fill_rule(path: &sk::Path, rule: FillRule) -> sk::Path {
    let fill = sk_path_fill_type_from_fill_rule(rule);
    if path.fill_type() == fill {
        path.clone()
    } else {
        path.with_fill_type(fill)
    }
}

fn clip_shape_to_sk_path(
    paths: &[Option<sk::Path>],
    shape: &ClipShape,
    fill_rule: Option<FillRule>,
) -> Option<sk::Path> {
    match shape {
        ClipShape::Rect(rect) => {
            let rect = rectf_to_sk_rect(*rect);
            let mut path = sk::Path::new();
            path.add_rect(rect, None);
            Some(path)
        }
        ClipShape::RoundedRect(rr) => {
            let rrect = rounded_rectf_to_sk_rrect(*rr);
            let mut path = sk::Path::new();
            path.add_rrect(rrect, None);
            Some(path)
        }
        ClipShape::Path(id) => {
            let path = paths.get(id.0 as usize).and_then(|p| p.clone())?;
            Some(match fill_rule {
                Some(rule) => path_with_fill_rule(&path, rule),
                None => path,
            })
        }
    }
}

fn apply_stroke_style(paint: &mut sk::Paint, style: &kurbo::Stroke) {
    paint.set_style(sk::PaintStyle::Stroke);
    paint.set_stroke_width(f64_to_f32(style.width));
    paint.set_stroke_miter(f64_to_f32(style.miter_limit));
    paint.set_stroke_join(match style.join {
        kurbo::Join::Bevel => sk::PaintJoin::Bevel,
        kurbo::Join::Miter => sk::PaintJoin::Miter,
        kurbo::Join::Round => sk::PaintJoin::Round,
    });
    let cap = match style.start_cap {
        kurbo::Cap::Butt => sk::PaintCap::Butt,
        kurbo::Cap::Square => sk::PaintCap::Square,
        kurbo::Cap::Round => sk::PaintCap::Round,
    };
    paint.set_stroke_cap(cap);
    if !style.dash_pattern.is_empty() {
        let intervals: Vec<f32> = style.dash_pattern.iter().map(|v| f64_to_f32(*v)).collect();
        if let Some(effect) =
            sk::PathEffect::dash(intervals.as_slice(), f64_to_f32(style.dash_offset))
        {
            paint.set_path_effect(effect);
        }
    }
}

impl ResourceBackend for SkiaImagingBackend<'_> {
    fn create_path(&mut self, desc: PathDesc) -> PathId {
        let mut path = sk::Path::new();
        for cmd in desc.commands.iter() {
            match *cmd {
                PathCmd::MoveTo { x, y } => {
                    path.move_to((x, y));
                }
                PathCmd::LineTo { x, y } => {
                    path.line_to((x, y));
                }
                PathCmd::QuadTo { x1, y1, x, y } => {
                    path.quad_to((x1, y1), (x, y));
                }
                PathCmd::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    path.cubic_to((x1, y1), (x2, y2), (x, y));
                }
                PathCmd::Close => {
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
        let color_type = match desc.format {
            peniko::ImageFormat::Rgba8 => sk::ColorType::RGBA8888,
            peniko::ImageFormat::Bgra8 => sk::ColorType::BGRA8888,
            // If new formats are added upstream, prefer failing loudly until
            // we define v0 behavior for them.
            _ => sk::ColorType::RGBA8888,
        };
        let alpha_type = match desc.alpha_type {
            peniko::ImageAlphaType::Alpha => sk::AlphaType::Unpremul,
            peniko::ImageAlphaType::AlphaPremultiplied => sk::AlphaType::Premul,
        };
        let info = sk::ImageInfo::new(
            (desc.width as i32, desc.height as i32),
            color_type,
            alpha_type,
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
            StateOp::PushLayer(layer) => {
                self.push_layer(layer);
            }
            StateOp::PopLayer => match self.stack.pop() {
                Some(StackEntry::Noop) => {}
                Some(StackEntry::Restore(n)) => {
                    for _ in 0..n {
                        self.canvas.restore();
                    }
                }
                None => panic!("PopLayer with empty stack"),
            },
            StateOp::SetPaint(id) => {
                if let Some(Some(desc)) = self.paints.get(id.0 as usize) {
                    self.current_brush = desc.brush.clone();
                }
            }
            StateOp::SetStroke(style) => {
                self.current_stroke = Some(style);
            }
            StateOp::SetFillRule(rule) => {
                self.current_fill_rule = rule;
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
                    let path = path_with_fill_rule(path, self.current_fill_rule);
                    let mut paint =
                        brush_to_paint(&self.current_brush, 1.0, self.current_paint_transform);
                    paint.set_style(skia_safe::PaintStyle::Fill);

                    self.canvas.draw_path(&path, &paint);
                }
            }
            DrawOp::FillRect { x0, y0, x1, y1 } => {
                let rect = sk::Rect::new(x0, y0, x1, y1);
                let mut paint =
                    brush_to_paint(&self.current_brush, 1.0, self.current_paint_transform);
                paint.set_style(skia_safe::PaintStyle::Fill);
                self.canvas.draw_rect(rect, &paint);
            }
            DrawOp::StrokePath(id) => {
                if let (Some(Some(path)), Some(stroke)) =
                    (self.paths.get(id.0 as usize), self.current_stroke.clone())
                {
                    let mut paint =
                        brush_to_paint(&self.current_brush, 1.0, self.current_paint_transform);
                    apply_stroke_style(&mut paint, &stroke);

                    self.canvas.draw_path(path, &paint);
                }
            }
            DrawOp::StrokeRect { x0, y0, x1, y1 } => {
                if let Some(stroke) = self.current_stroke.clone() {
                    let rect = sk::Rect::new(x0, y0, x1, y1);
                    let mut paint =
                        brush_to_paint(&self.current_brush, 1.0, self.current_paint_transform);
                    apply_stroke_style(&mut paint, &stroke);
                    self.canvas.draw_rect(rect, &paint);
                }
            }
            DrawOp::DrawImage {
                image,
                transform,
                sampler,
            } => {
                let idx = image.0 as usize;
                if let Some(Some(img)) = self.images.get(idx) {
                    self.canvas.save();
                    // Apply only the image-local transform; current_transform
                    // has already been applied via `SetTransform`.
                    let m = affine_to_matrix(transform);
                    self.canvas.concat(&m);

                    // Use an image shader so we can express sampling and tile modes
                    // consistently with the v0 imaging IR.
                    let tile_x = tile_mode_from_extend(sampler.x_extend);
                    let tile_y = tile_mode_from_extend(sampler.y_extend);
                    let sampling = sampling_options_from_image_sampler(&sampler);
                    let shader = img
                        .to_shader(Some((tile_x, tile_y)), sampling, None)
                        .expect("Skia image shader");

                    let mut paint = sk::Paint::default();
                    paint.set_shader(shader);
                    paint.set_alpha_f(sampler.alpha.clamp(0.0, 1.0));

                    let rect = sk::Rect::new(0.0, 0.0, img.width() as f32, img.height() as f32);
                    self.canvas.draw_rect(rect, &paint);
                    self.canvas.restore();
                }
            }
            DrawOp::DrawImageRect {
                image,
                src,
                dst,
                sampler,
            } => {
                let idx = image.0 as usize;
                if let Some(Some(img)) = self.images.get(idx) {
                    let src = src.unwrap_or(RectF {
                        x0: 0.0,
                        y0: 0.0,
                        x1: img.width() as f32,
                        y1: img.height() as f32,
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

                    self.canvas.save();
                    // Clip in the current transform (SetTransform already applied).
                    let dst_rect = sk::Rect::new(dst.x0, dst.y0, dst.x1, dst.y1);
                    self.canvas.clip_rect(dst_rect, None, true);

                    // Apply mapping transform and draw the full image rect at the origin.
                    self.canvas.concat(&affine_to_matrix(local));

                    let tile_x = tile_mode_from_extend(sampler.x_extend);
                    let tile_y = tile_mode_from_extend(sampler.y_extend);
                    let sampling = sampling_options_from_image_sampler(&sampler);
                    let shader = img
                        .to_shader(Some((tile_x, tile_y)), sampling, None)
                        .expect("Skia image shader");

                    let mut paint = sk::Paint::default();
                    paint.set_shader(shader);
                    paint.set_alpha_f(sampler.alpha.clamp(0.0, 1.0));

                    let rect = sk::Rect::new(0.0, 0.0, img.width() as f32, img.height() as f32);
                    self.canvas.draw_rect(rect, &paint);
                    self.canvas.restore();
                }
            }
            DrawOp::DrawPicture { picture, transform } => {
                let idx = picture.0 as usize;
                if let Some(Some(desc)) = self.pictures.get_mut(idx) {
                    // Preferred path: use a cached picture-local Skia picture
                    // built at record-time, then apply the outer transform
                    // when drawing.
                    if let Some(accel_any) = desc.recording.acceleration.as_ref()
                        && desc.recording.can_reuse(transform)
                        && let Some(picture) = accel_any.downcast_ref::<sk::Picture>()
                    {
                        self.canvas.save();
                        self.canvas.reset_matrix();
                        let m = affine_to_matrix(transform);
                        self.canvas.concat(&m);
                        self.canvas.draw_picture(picture, None, None);
                        self.canvas.restore();
                    } else {
                        // Fallback: no usable acceleration (e.g., recording was
                        // created by another backend). Replay the IR directly into
                        // this backend, applying the outer transform to any
                        // SetTransform ops.
                        let saved_transform = self.current_transform;
                        let saved_stroke = self.current_stroke.clone();
                        let saved_brush = self.current_brush.clone();
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

                        self.current_transform = saved_transform;
                        self.current_stroke = saved_stroke;
                        self.current_brush = saved_brush;
                        self.current_paint_transform = saved_paint_transform;
                    }
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
