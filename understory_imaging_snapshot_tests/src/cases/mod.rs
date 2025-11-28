// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use kurbo::Affine;
use peniko::{
    BlendMode, Brush, Color, ColorStop, Extend, Gradient, GradientKind, ImageAlphaType,
    ImageFormat, ImageSampler, LinearGradientPosition, Mix, RadialGradientPosition,
};
use understory_imaging::{
    ClipShape, DrawOp, FillRule, ImageDesc, ImageId, ImagingBackend, ImagingBackendExt, PaintDesc,
    PathCmd, PathDesc, RectF, StateOp, record_picture,
};

mod basic;
mod clips;
mod compositing;
mod fill_rules;
mod filters;
mod gradients;
mod images;
mod pictures;
mod strokes;

pub const DEFAULT_WIDTH: u16 = 320;
pub const DEFAULT_HEIGHT: u16 = 200;

pub trait SnapshotCase: Sync {
    fn name(&self) -> &'static str;

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        0
    }

    fn skia_max_diff_pixels(&self) -> u64 {
        0
    }

    fn supports_backend(&self, _backend: &str) -> bool {
        true
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32);
}

fn matches_glob(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == text;
    }

    let mut remainder = text;
    let mut first_part = true;
    for part in pattern.split('*') {
        if part.is_empty() {
            continue;
        }
        match remainder.find(part) {
            Some(idx) => {
                if first_part && !pattern.starts_with('*') && idx != 0 {
                    return false;
                }
                remainder = &remainder[idx + part.len()..];
            }
            None => return false,
        }
        first_part = false;
    }
    if !pattern.ends_with('*') {
        remainder.is_empty()
    } else {
        true
    }
}

fn case_filters() -> Option<Vec<String>> {
    let raw = std::env::var("UNDERSTORY_IMAGING_CASE").ok()?;
    let filters: Vec<String> = raw
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    (!filters.is_empty()).then_some(filters)
}

pub fn selected_cases() -> Vec<&'static dyn SnapshotCase> {
    let Some(filters) = case_filters() else {
        return CASES.to_vec();
    };

    let selected: Vec<&'static dyn SnapshotCase> = CASES
        .iter()
        .copied()
        .filter(|case| filters.iter().any(|f| matches_glob(f, case.name())))
        .collect();

    if selected.is_empty() {
        let available: Vec<&str> = CASES.iter().map(|c| c.name()).collect();
        panic!(
            "UNDERSTORY_IMAGING_CASE matched no snapshot cases.\n  filter: {filters:?}\n  available: {available:?}"
        );
    }

    selected
}

pub fn selected_cases_for_backend(backend: &str) -> Vec<&'static dyn SnapshotCase> {
    selected_cases()
        .into_iter()
        .filter(|case| case.supports_backend(backend))
        .collect()
}

fn rect_path_desc(x0: f32, y0: f32, x1: f32, y1: f32) -> PathDesc {
    PathDesc {
        commands: Box::new([
            PathCmd::MoveTo { x: x0, y: y0 },
            PathCmd::LineTo { x: x1, y: y0 },
            PathCmd::LineTo { x: x1, y: y1 },
            PathCmd::LineTo { x: x0, y: y1 },
            PathCmd::Close,
        ]),
    }
}

fn star_path_desc(cx: f32, cy: f32, r0: f32, r1: f32, points: usize) -> PathDesc {
    let mut cmds = Vec::with_capacity(points * 2 + 2);
    let step = std::f32::consts::TAU / points as f32;
    for i in 0..(points * 2) {
        let r = if i % 2 == 0 { r0 } else { r1 };
        let a = i as f32 * (step * 0.5) - std::f32::consts::FRAC_PI_2;
        let x = cx + r * a.cos();
        let y = cy + r * a.sin();
        if i == 0 {
            cmds.push(PathCmd::MoveTo { x, y });
        } else {
            cmds.push(PathCmd::LineTo { x, y });
        }
    }
    cmds.push(PathCmd::Close);
    PathDesc {
        commands: cmds.into_boxed_slice(),
    }
}

fn solid_rgba_image<B>(backend: &mut B, width: u32, height: u32, rgba: [u8; 4]) -> ImageId
where
    B: ImagingBackend + ?Sized,
{
    let mut pixels = vec![0_u8; (width as usize) * (height as usize) * 4];
    for px in pixels.chunks_exact_mut(4) {
        px.copy_from_slice(&rgba);
    }
    backend.create_image(
        ImageDesc {
            width,
            height,
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
        },
        &pixels,
    )
}

fn alpha_edge_image<B>(backend: &mut B, width: u32, height: u32) -> ImageId
where
    B: ImagingBackend + ?Sized,
{
    // RGBA8 straight-alpha, with a transparent border and a soft alpha interior.
    let mut pixels = vec![0_u8; (width as usize) * (height as usize) * 4];
    for y in 0..height {
        for x in 0..width {
            let border = x == 0 || y == 0 || x + 1 == width || y + 1 == height;
            let (r, g, b, a) = if border {
                (0, 0, 0, 0)
            } else if (x + y) % 7 == 0 {
                (255, 255, 255, 255)
            } else {
                (255, 0, 255, 160)
            };
            let i = ((y * width + x) as usize) * 4;
            pixels[i..i + 4].copy_from_slice(&[r, g, b, a]);
        }
    }
    backend.create_image(
        ImageDesc {
            width,
            height,
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
        },
        &pixels,
    )
}

pub const CASES: &[&dyn SnapshotCase] = &[
    &basic::BasicShapesClipGroup,
    &gradients::LinearGradientPaintTransform,
    &pictures::PictureRecordingReuse,
    &images::ImageScaled,
    &strokes::StrokeStyles,
    &clips::ClipPathAndRoundedRect,
    &clips::ClipStrokeDashesAndCaps,
    &clips::ClipRoundedRectPerCorner,
    &clips::ClipNestingTransform,
    &gradients::RadialGradientPaintTransform,
    &compositing::GroupOpacityStack,
    &filters::FilterBlurLayer,
    &filters::FilterDropShadowLayer,
    &filters::FilterOffsetLayer,
    &filters::FilterFloodLayer,
    &images::ImageAlphaEdges,
    &images::ImageTransformComposition,
    &images::ImageClipRectAndPath,
    &pictures::PictureWithImagesAndState,
    &images::ImageRectSrcDst,
    &images::ImageSamplingNearestVsLinear,
    &images::ImageExtendModes,
    &fill_rules::FillRuleNonzeroVsEvenodd,
    &fill_rules::ClipFillRuleNonzeroVsEvenodd,
];
