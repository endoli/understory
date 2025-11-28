// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_text_imaging --heading-base-level=0

//! Understory Text → Imaging helpers.
//!
//! This crate provides small, plain‑old‑data (POD) friendly helpers for
//! expressing text as [`understory_imaging`] operations. It does **not**
//! perform shaping
//! or font resolution itself; instead it assumes some upstream text
//! engine can provide positioned glyphs and, optionally, path resources
//! for glyph outlines.
//!
//! The primary *helper* API is [`draw_text_run`], which:
//! - Accepts font bytes, text, size, and a paint id,
//! - Prefers bitmap glyphs when available (e.g. emoji fonts),
//! - Falls back to outline glyphs otherwise,
//! - Emits imaging ops (`StateOp`/`DrawOp`) suitable for any imaging backend.
//!
//! The *core* API is glyph‑run based: a slice of [`GlyphInstance`] plus a
//! `PaintId` lowered via [`draw_glyph_run`] into imaging operations.
//!
//! In a full text stack, a shaping engine (such as Parley) is expected to
//! produce glyph runs; `draw_text_run` is intentionally a convenience for
//! demos and simple cases rather than a general text layout API.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
#[cfg(not(feature = "std"))]
use kurbo::common::FloatFuncs as _; // for `round`
use kurbo::{Affine, Vec2};
use peniko::{BlendMode, Blob, Brush, Color, Compose, FontData, Mix};
use skrifa::bitmap::{BitmapData, BitmapFormat, BitmapStrikes};
use skrifa::color::{
    Brush as ColrBrush, ColorGlyphFormat, ColorPainter as ColrPainter,
    CompositeMode as ColrCompositeMode, Transform as ColrTransform,
};
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::{BoundingBox, GlyphMetrics};
use skrifa::outline::{
    DrawSettings, Engine as HintEngine, HintingInstance, HintingOptions, OutlineGlyphCollection,
    OutlinePen, SmoothMode as HintSmoothMode, Target as HintTarget,
};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use understory_imaging::{
    ClipOp, ClipShape, DrawOp, FillRule, ImageDesc, ImagingBackend, ImagingOp, LayerOp, PaintDesc,
    PaintId, PathCmd, PathDesc, PathId, PictureDesc, PictureId, RecordedOps, RectF,
    ResourceBackend, StateOp, TransformClass,
};

#[cfg(feature = "std")]
use png::{BitDepth, ColorType, Transformations};

/// Hinting strategy to apply when converting glyph outlines into paths.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextHinting {
    /// Do not apply embedded or automatic hinting. Outlines are scaled
    /// only by the requested size.
    Unhinted,
    /// Apply Skrifa's embedded/automatic hinting using its default
    /// configuration for the font at the requested size.
    Hinted,
}

/// Single positioned glyph in a glyph run, identified by glyph id.
///
/// This associates a font glyph (`GlyphId`) with a position expressed in
/// user space. Text shaping / layout is expected to happen upstream; this
/// type is purely about mapping positioned glyphs into the imaging IR.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GlyphInstance {
    /// Glyph id in the font.
    pub glyph_id: GlyphId,
    /// X position of the glyph origin.
    pub x: f32,
    /// Y position of the glyph origin.
    pub y: f32,
}

/// Convert a normalized alpha value in `[0, 1]` into an 8-bit
/// channel, clamping to the valid range.
#[allow(
    clippy::cast_possible_truncation,
    reason = "alpha is explicitly clamped to [0, 255] before casting"
)]
fn alpha_to_u8(a: f32) -> u8 {
    (a * 255.0).round().clamp(0.0, 255.0) as u8
}

// Match Vello's default hinting options for outline glyphs:
// - AutoFallback engine,
// - Smooth LCD target,
// - preserve_linear_metrics = true.
const HINTING_OPTIONS: HintingOptions = HintingOptions {
    engine: HintEngine::AutoFallback,
    target: HintTarget::Smooth {
        mode: HintSmoothMode::Lcd,
        symmetric_rendering: false,
        preserve_linear_metrics: true,
    },
};

/// Prepared per-run parameters for outline hinting and transforms.
struct PreparedRun {
    /// Transform to use for glyph drawing, with any uniform scale
    /// folded into the outline size when hinting is enabled.
    transform: Affine,
    /// Font size used for outline generation and hinting.
    outline_size: Size,
    /// True if outlines should be hinted for this run.
    use_hinting_for_outlines: bool,
}

/// Prepare a glyph run for rendering, mirroring Vello's `prepare_glyph_run`.
///
/// This is adapted from `vello_common::glyph::prepare_glyph_run` so that
/// transform-aware hinting stays consistent between Vello and
/// `understory_text_imaging`.
fn prepare_run_for_hinting(
    base_transform: Affine,
    font_size_px: f32,
    hinting: TextHinting,
) -> PreparedRun {
    if !matches!(hinting, TextHinting::Hinted) {
        return PreparedRun {
            transform: base_transform,
            outline_size: Size::new(font_size_px),
            use_hinting_for_outlines: false,
        };
    }

    // Vertical-only hinting: extract uniform scale from the transform,
    // applying it to the font size used for hinting while keeping the
    // residual transform scale at 1. This matches Vello's behavior and
    // avoids double-scaling hinted outlines.
    let total_transform = base_transform;
    let [t_a, t_b, t_c, t_d, t_e, t_f] = total_transform.as_coeffs();

    let uniform_scale = t_a == t_d;
    let vertically_uniform = t_b == 0.0;

    if uniform_scale && vertically_uniform {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "transform scale factor from f64 to f32 is acceptable for font size"
        )]
        let vertical_font_size = font_size_px * t_d as f32;
        let size = Size::new(vertical_font_size);

        PreparedRun {
            transform: Affine::new([1.0, 0.0, t_c, 1.0, t_e, t_f]),
            outline_size: size,
            use_hinting_for_outlines: true,
        }
    } else {
        // Transform is not suitable for vertical-only hinting; fall back
        // to unhinted outlines at the requested font size.
        PreparedRun {
            transform: total_transform,
            outline_size: Size::new(font_size_px),
            use_hinting_for_outlines: false,
        }
    }
}

/// Draw a run of positioned glyph ids using any supported glyph kind.
///
/// For each glyph in the run, this:
/// - Prefers `COLRv1` glyphs when available via `colr_glyph_to_picture`,
/// - Falls back to bitmap strikes when present,
/// - Finally falls back to outline glyphs using `create_glyph_path`.
pub fn draw_glyph_run<B: ImagingBackend + ResourceBackend>(
    backend: &mut B,
    font_bytes: &[u8],
    font_index: u32,
    font_size_px: f32,
    paint: PaintId,
    colr_context_color: Color,
    glyphs: &[GlyphInstance],
    base_transform: Affine,
    hinting: TextHinting,
) {
    let font_ref = match FontRef::new(font_bytes) {
        Ok(f) => f,
        Err(_) => return,
    };

    let color_glyphs = font_ref.color_glyphs();
    let strikes = BitmapStrikes::new(&font_ref);
    let bitmap_size = Size::new(font_size_px);
    let strikes_format = strikes.format();
    let upem = font_ref
        .head()
        .ok()
        .map(|h| h.units_per_em() as f32)
        .unwrap_or(1.0)
        .max(1.0);
    let font_units_to_size = font_size_px / upem;

    // Prepare transform and outline size for hinting, mirroring Vello's
    // transform-aware hinting behavior.
    let prepared = prepare_run_for_hinting(base_transform, font_size_px, hinting);
    let run_transform = prepared.transform;

    for inst in glyphs {
        let gid = inst.glyph_id;
        let glyph_xf = Affine::translate((inst.x as f64, inst.y as f64));

        // COLRv1: try to lower glyph into a picture.
        if color_glyphs.get(gid).is_some()
            && let Some(picture) = colr_glyph_to_picture(
                backend,
                font_bytes,
                font_index,
                gid,
                font_size_px,
                colr_context_color,
            )
        {
            backend.draw(DrawOp::DrawPicture {
                picture,
                transform: run_transform * glyph_xf,
            });
            continue;
        }

        // Bitmap strikes.
        if let Some(bitmap) = strikes.glyph_for_size(bitmap_size, gid) {
            let (width, height) = (bitmap.width, bitmap.height);
            let pixels: Vec<u8> = match bitmap.data {
                BitmapData::Bgra(data) => {
                    let mut out = Vec::with_capacity((width * height * 4) as usize);
                    for chunk in data.chunks_exact(4) {
                        let [b, g, r, a] = <[u8; 4]>::try_from(chunk).unwrap();
                        out.extend_from_slice(&[r, g, b, a]);
                    }
                    out
                }
                BitmapData::Mask(mask) => {
                    let Some(masks) = bitmap_masks(mask.bpp) else {
                        continue;
                    };
                    if !mask.is_packed {
                        continue;
                    }
                    mask.data
                        .iter()
                        .flat_map(|byte| {
                            masks.iter().map(move |m| (byte & m.mask) >> m.right_shift)
                        })
                        .flat_map(|alpha| [u8::MAX, u8::MAX, u8::MAX, alpha])
                        .collect()
                }
                #[cfg(feature = "std")]
                BitmapData::Png(data) => {
                    let mut decoder = png::Decoder::new(data);
                    decoder.set_transformations(Transformations::ALPHA | Transformations::STRIP_16);
                    let Ok(mut reader) = decoder.read_info() else {
                        continue;
                    };
                    if reader.output_color_type() != (ColorType::Rgba, BitDepth::Eight) {
                        continue;
                    }
                    let mut buf = vec![0_u8; reader.output_buffer_size()];
                    let Ok(info) = reader.next_frame(&mut buf) else {
                        continue;
                    };
                    if info.width != width || info.height != height {
                        continue;
                    }
                    buf
                }
                #[cfg(not(feature = "std"))]
                BitmapData::Png(_data) => {
                    continue;
                }
            };

            let image = backend.create_image(
                ImageDesc {
                    width,
                    height,
                    format: peniko::ImageFormat::Rgba8,
                    alpha_type: peniko::ImageAlphaType::Alpha,
                },
                &pixels,
            );
            // Placement and scaling mirror Vello's bitmap glyph handling:
            // - outer bearings in font units adjusted by `font_units_to_size`,
            // - non-uniform scale from strike ppem to `font_size_px`,
            // - inner bearings and placement origin applied in pixel space.
            let x_scale = font_size_px / bitmap.ppem_x.max(1.0);
            let y_scale = font_size_px / bitmap.ppem_y.max(1.0);

            let mut bearing_y = bitmap.bearing_y;
            if bearing_y == 0.0 && strikes_format == Some(BitmapFormat::Sbix) {
                bearing_y = 100.0;
            }

            let origin_shift = match bitmap.placement_origin {
                skrifa::bitmap::Origin::TopLeft => Vec2::default(),
                skrifa::bitmap::Origin::BottomLeft => Vec2 {
                    x: 0.0,
                    y: -(height as f64),
                },
            };

            let mut bitmap_xf = run_transform;
            bitmap_xf = bitmap_xf
                // Glyph origin.
                .pre_translate(Vec2::new(inst.x as f64, inst.y as f64))
                // Outer bearings, in font units.
                .pre_translate(Vec2 {
                    x: (-bitmap.bearing_x * font_units_to_size) as f64,
                    y: (bearing_y * font_units_to_size) as f64,
                })
                // Scale to pixel-space.
                .pre_scale_non_uniform(x_scale as f64, y_scale as f64)
                // Inner bearings, in pixels.
                .pre_translate(Vec2 {
                    x: -bitmap.inner_bearing_x as f64,
                    y: -bitmap.inner_bearing_y as f64,
                })
                .pre_translate(origin_shift);

            backend.draw(DrawOp::DrawImage {
                image,
                transform: bitmap_xf,
                sampler: peniko::ImageSampler::default(),
            });
            continue;
        }

        // Outline fallback.
        if let Some(path) = create_glyph_path(
            backend,
            font_bytes,
            gid,
            font_index,
            prepared.outline_size,
            prepared.use_hinting_for_outlines,
        ) {
            backend.state(StateOp::SetPaint(paint));
            let mut coeffs = (run_transform * glyph_xf).as_coeffs();
            if prepared.use_hinting_for_outlines {
                // When hinting, snap the vertical offset to whole pixels,
                // mirroring Vello's vertical-only hinting behavior.
                coeffs[5] = coeffs[5].round();
            }
            backend.state(StateOp::SetTransform(Affine::new(coeffs)));
            backend.draw(DrawOp::FillPath(path));
        }
    }
}

/// Convert a glyph outline from a font into an imaging path using the
/// provided Skrifa draw settings.
fn glyph_outline_to_path<'a, S>(
    outlines: &OutlineGlyphCollection<'a>,
    glyph_id: GlyphId,
    settings: S,
) -> Option<PathDesc>
where
    S: Into<DrawSettings<'a>>,
{
    struct Recorder {
        cmds: Vec<PathCmd>,
    }

    impl OutlinePen for Recorder {
        fn move_to(&mut self, x: f32, y: f32) {
            self.cmds.push(PathCmd::MoveTo { x, y });
        }

        fn line_to(&mut self, x: f32, y: f32) {
            self.cmds.push(PathCmd::LineTo { x, y });
        }

        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.cmds.push(PathCmd::QuadTo { x1, y1, x, y });
        }

        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.cmds.push(PathCmd::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            });
        }

        fn close(&mut self) {
            self.cmds.push(PathCmd::Close);
        }
    }

    let mut recorder = Recorder { cmds: Vec::new() };
    let outline = outlines.get(glyph_id)?;
    outline.draw(settings, &mut recorder).ok()?;
    if recorder.cmds.is_empty() {
        None
    } else {
        Some(PathDesc {
            commands: recorder.cmds.into_boxed_slice(),
        })
    }
}

/// Create a glyph path resource in a [`ResourceBackend`] from raw font bytes.
fn create_glyph_path<B: ResourceBackend>(
    backend: &mut B,
    font_bytes: &[u8],
    glyph_id: GlyphId,
    font_index: u32,
    size: Size,
    use_hinting: bool,
) -> Option<PathId> {
    let blob: Blob<u8> = Blob::from(font_bytes.to_vec());
    let font_data = FontData::new(blob, font_index);
    let font_ref = FontRef::from_index(font_data.data.as_ref(), font_data.index).ok()?;
    let outlines = font_ref.outline_glyphs();

    let path_desc = if use_hinting {
        // Embedded/automatic hinting using Skrifa's defaults.
        let hi =
            HintingInstance::new(&outlines, size, LocationRef::default(), HINTING_OPTIONS).ok()?;
        glyph_outline_to_path(&outlines, glyph_id, &hi)?
    } else {
        // Unhinted outlines at the requested size.
        glyph_outline_to_path(&outlines, glyph_id, size)?
    };

    // Flip Y so glyphs are upright in the usual screen coordinate system.
    let scale_y = -1.0_f32;

    let scaled_cmds = path_desc
        .commands
        .iter()
        .map(|cmd| match *cmd {
            PathCmd::MoveTo { x, y } => PathCmd::MoveTo { x, y: y * scale_y },
            PathCmd::LineTo { x, y } => PathCmd::LineTo { x, y: y * scale_y },
            PathCmd::QuadTo { x1, y1, x, y } => PathCmd::QuadTo {
                x1,
                y1: y1 * scale_y,
                x,
                y: y * scale_y,
            },
            PathCmd::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => PathCmd::CurveTo {
                x1,
                y1: y1 * scale_y,
                x2,
                y2: y2 * scale_y,
                x,
                y: y * scale_y,
            },
            PathCmd::Close => PathCmd::Close,
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();

    Some(backend.create_path(PathDesc {
        commands: scaled_cmds,
    }))
}

/// Create an *unscaled* glyph path resource from raw font bytes.
///
/// The resulting path is expressed in font units; callers are responsible
/// for applying scaling and coordinate system conversion via transforms.
/// This is primarily useful for `COLRv1` rendering, where the COLR paint graph
/// is evaluated in font space.
fn create_glyph_path_unscaled<B: ResourceBackend>(
    backend: &mut B,
    font_bytes: &[u8],
    glyph_id: GlyphId,
    font_index: u32,
) -> Option<PathId> {
    let blob: Blob<u8> = Blob::from(font_bytes.to_vec());
    let font_data = FontData::new(blob, font_index);
    let font_ref = FontRef::from_index(font_data.data.as_ref(), font_data.index).ok()?;
    let outlines = font_ref.outline_glyphs();

    // For COLRv1 rendering we keep outlines in unscaled font units and
    // avoid size-dependent hinting, since the COLR graph itself controls
    // scaling and composition.
    let size = Size::unscaled();
    let path_desc = glyph_outline_to_path(&outlines, glyph_id, size)?;
    Some(backend.create_path(path_desc))
}

/// Draw a text run using either outline or bitmap glyphs, depending on what
/// the font provides.
///
/// This is a convenience entry point for the current prototype:
/// callers supply font bytes, text, font size, a paint id, and a base
/// transform. The helper will:
/// - Prefer bitmap glyphs when available (e.g. emoji fonts),
/// - Fall back to outline glyphs otherwise.
///
/// In the longer term, this layer is expected to consume *positioned glyphs*
/// from a dedicated text engine (Parley or a sibling crate) rather than raw
/// text; `draw_text_run` should be treated as a temporary, phase‑0 helper
/// rather than a final text API.
pub fn draw_text_run<B>(
    backend: &mut B,
    font_bytes: &[u8],
    text: &str,
    font_index: u32,
    font_size_px: f32,
    paint: PaintId,
    base_transform: Affine,
    hinting: TextHinting,
) where
    B: ImagingBackend + ResourceBackend,
{
    draw_text_run_with_context_color(
        backend,
        font_bytes,
        text,
        font_index,
        font_size_px,
        paint,
        Color::WHITE,
        base_transform,
        hinting,
    );
}

/// Variant of [`draw_text_run`] that lets the caller explicitly specify the
/// COLR "context color". When `COLRv1` glyphs use the context-color palette
/// entry, this color is used as the base, matching Vello's COLR renderer.
pub fn draw_text_run_with_context_color<B>(
    backend: &mut B,
    font_bytes: &[u8],
    text: &str,
    font_index: u32,
    font_size_px: f32,
    paint: PaintId,
    colr_context_color: Color,
    base_transform: Affine,
    hinting: TextHinting,
) where
    B: ImagingBackend + ResourceBackend,
{
    let font_ref = match FontRef::new(font_bytes) {
        Ok(f) => f,
        Err(_) => return,
    };
    let charmap = font_ref.charmap();

    // Prepare run parameters so that layout metrics and glyph rendering
    // use a consistent effective outline size, matching Vello's
    // `prepare_glyph_run` behavior.
    let prepared = prepare_run_for_hinting(base_transform, font_size_px, hinting);
    // Collect glyph ids for all characters we care about.
    let glyph_ids: Vec<GlyphId> = text.chars().filter_map(|ch| charmap.map(ch)).collect();
    if glyph_ids.is_empty() {
        return;
    }

    // Simple horizontal layout using Skrifa glyph metrics (at the
    // prepared outline size) and bitmap advances where available. This
    // keeps spacing reasonable for outline, COLR, and bitmap fonts
    // without attempting full shaping.
    let metrics = GlyphMetrics::new(&font_ref, prepared.outline_size, LocationRef::default());
    let strikes = BitmapStrikes::new(&font_ref);
    let bitmap_size = Size::new(font_size_px);
    let mut x = 0.0_f32;
    let mut glyphs: Vec<GlyphInstance> = Vec::with_capacity(glyph_ids.len());
    for gid in glyph_ids {
        glyphs.push(GlyphInstance {
            glyph_id: gid,
            x,
            y: 0.0,
        });

        // Prefer bitmap advances when a strike is available, scaled to
        // match the requested font size. Fall back to outline metrics,
        // then to a simple heuristic.
        let advance = if let Some(bitmap) = strikes.glyph_for_size(bitmap_size, gid) {
            let ppem = bitmap.ppem_x.max(1.0);
            let raw_advance = bitmap
                .advance
                .unwrap_or_else(|| metrics.advance_width(gid).unwrap_or(font_size_px * 0.6));
            let scale = font_size_px / ppem;
            raw_advance * scale
        } else {
            metrics.advance_width(gid).unwrap_or(font_size_px * 0.6)
        };

        x += advance;
    }

    draw_glyph_run(
        backend,
        font_bytes,
        font_index,
        font_size_px,
        paint,
        colr_context_color,
        &glyphs,
        base_transform,
        hinting,
    );
}

/// Minimal bitmap mask description used for expanding packed mask bitmaps.
struct BitmapMask {
    mask: u8,
    right_shift: u8,
}

const fn byte(value: u8) -> BitmapMask {
    BitmapMask {
        mask: 1 << value,
        right_shift: value,
    }
}

fn bitmap_masks(bpp: u8) -> Option<&'static [BitmapMask]> {
    match bpp {
        1 => {
            const BPP_1_MASK: &[BitmapMask] = &[
                byte(0),
                byte(1),
                byte(2),
                byte(3),
                byte(4),
                byte(5),
                byte(6),
                byte(7),
            ];
            Some(BPP_1_MASK)
        }
        2 => {
            const BPP_2_MASK: &[BitmapMask] = &[
                BitmapMask {
                    mask: 0b1100_0000,
                    right_shift: 6,
                },
                BitmapMask {
                    mask: 0b0011_0000,
                    right_shift: 4,
                },
                BitmapMask {
                    mask: 0b0000_1100,
                    right_shift: 2,
                },
                BitmapMask {
                    mask: 0b0000_0011,
                    right_shift: 0,
                },
            ];
            Some(BPP_2_MASK)
        }
        4 => {
            const BPP_4_MASK: &[BitmapMask] = &[
                BitmapMask {
                    mask: 0b1111_0000,
                    right_shift: 4,
                },
                BitmapMask {
                    mask: 0b0000_1111,
                    right_shift: 0,
                },
            ];
            Some(BPP_4_MASK)
        }
        8 => {
            const BPP_8_MASK: &[BitmapMask] = &[BitmapMask {
                mask: 0xFF,
                right_shift: 0,
            }];
            Some(BPP_8_MASK)
        }
        _ => None,
    }
}

/// Minimal `ColorPainter` that lowers COLR glyphs into imaging ops suitable
/// for a picture. This implementation:
/// - Handles solid brushes via the font's CPAL palette,
/// - Ignores gradients, clip boxes, and layers for now,
/// - Uses `create_glyph_path` to obtain paths for `fill_glyph`.
struct ImagingColorPainter<'a, B> {
    backend: &'a mut B,
    font_bytes: &'a [u8],
    font_index: u32,
    font_ref: FontRef<'a>,
    context_color: Color,
    ops: Vec<ImagingOp>,
    current_transform: Affine,
    transform_stack: Vec<Affine>,
    stack: Vec<PainterStackEntry>,
}

#[derive(Clone, Debug)]
enum PainterStackEntry {
    Clip(ClipOp),
    Group,
}

impl<'a, B> ImagingColorPainter<'a, B>
where
    B: ResourceBackend,
{
    fn new(
        backend: &'a mut B,
        font_bytes: &'a [u8],
        font_index: u32,
        font_size_px: f32,
        font_ref: FontRef<'a>,
        context_color: Color,
    ) -> Self {
        // Establish an initial transform that maps font units to pixels at the
        // requested font size, flipping Y so glyphs appear upright in screen
        // coordinates. COLR transforms are composed in this coordinate space.
        let mut current_transform = Affine::IDENTITY;
        if let Ok(head) = font_ref.head() {
            let upem = head.units_per_em() as f64;
            if upem > 0.0 {
                let sx = font_size_px as f64 / upem;
                let sy = -font_size_px as f64 / upem;
                current_transform = Affine::scale_non_uniform(sx, sy);
            }
        }

        Self {
            backend,
            font_bytes,
            font_index,
            font_ref,
            context_color,
            ops: Vec::new(),
            current_transform,
            transform_stack: Vec::new(),
            stack: Vec::new(),
        }
    }

    fn affine_from_colr(t: ColrTransform) -> Affine {
        Affine::new([
            t.xx as f64,
            t.yx as f64,
            t.xy as f64,
            t.yy as f64,
            t.dx as f64,
            t.dy as f64,
        ])
    }

    /// Normalize gradient color stops to cover [0, 1] and remove redundant
    /// trailing stops at 1.0, matching Vello's COLR handling.
    fn normalize_color_stops(mut stops: Vec<peniko::ColorStop>) -> peniko::ColorStops {
        if stops.is_empty() {
            return peniko::ColorStops::from(&[][..]);
        }

        let first = stops[0];
        let last = *stops.last().unwrap();

        if first.offset != 0.0 {
            let mut new_stop = first;
            new_stop.offset = 0.0;
            stops.insert(0, new_stop);
        }

        if last.offset != 1.0 {
            let mut new_stop = last;
            new_stop.offset = 1.0;
            stops.push(new_stop);
        }

        // If there are multiple stops at offset 1.0, keep only the last one.
        while let Some(stop) = stops.get(stops.len().saturating_sub(2)) {
            if (stop.offset - 1.0).abs() < f32::EPSILON {
                let idx = stops.len().saturating_sub(2);
                stops.remove(idx);
            } else {
                break;
            }
        }

        peniko::ColorStops::from(stops.as_slice())
    }

    /// Ensure any COLR layers/clips are fully unwound so that a single glyph
    /// picture does not leak groups or clips into subsequent imaging ops.
    fn finalize(&mut self) {
        while let Some(entry) = self.stack.pop() {
            match entry {
                PainterStackEntry::Clip(_) => {
                    self.ops.push(ImagingOp::State(StateOp::PopLayer));
                }
                PainterStackEntry::Group => {
                    self.ops.push(ImagingOp::State(StateOp::PopLayer));
                }
            }
        }
    }

    fn current_clip(&self) -> Option<&ClipOp> {
        self.stack.iter().rev().find_map(|entry| match entry {
            PainterStackEntry::Clip(shape) => Some(shape),
            PainterStackEntry::Group => None,
        })
    }
}

impl<B> ColrPainter for ImagingColorPainter<'_, B>
where
    B: ResourceBackend,
{
    fn push_transform(&mut self, transform: ColrTransform) {
        self.transform_stack.push(self.current_transform);
        let a = Self::affine_from_colr(transform);
        self.current_transform *= a;
    }

    fn pop_transform(&mut self) {
        if let Some(prev) = self.transform_stack.pop() {
            self.current_transform = prev;
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        // Map COLR glyph clips into imaging clips by creating a glyph path
        // resource and emitting a clip state op that refers to it.
        if let Some(path) =
            create_glyph_path_unscaled(self.backend, self.font_bytes, glyph_id, self.font_index)
        {
            let clip = ClipOp::Fill {
                shape: ClipShape::Path(path),
                fill_rule: FillRule::NonZero,
            };
            self.stack.push(PainterStackEntry::Clip(clip.clone()));
            // Ensure the clip path and subsequent fills see the same transform.
            self.ops.push(ImagingOp::State(StateOp::SetTransform(
                self.current_transform,
            )));
            self.ops.push(ImagingOp::State(StateOp::PushLayer(LayerOp {
                clip: Some(clip),
                filter: None,
                blend: None,
                opacity: None,
            })));
        }
    }

    fn push_clip_box(&mut self, clip_box: BoundingBox) {
        // Map COLR clip boxes into imaging rect clips in font units; the
        // initial transform established in `new` maps into pixel space.
        let x0 = clip_box.x_min;
        let y0 = clip_box.y_min;
        let x1 = clip_box.x_max;
        let y1 = clip_box.y_max;

        let clip = ClipOp::Fill {
            shape: ClipShape::Rect(RectF { x0, y0, x1, y1 }),
            fill_rule: FillRule::NonZero,
        };
        self.stack.push(PainterStackEntry::Clip(clip.clone()));
        self.ops.push(ImagingOp::State(StateOp::SetTransform(
            self.current_transform,
        )));
        self.ops.push(ImagingOp::State(StateOp::PushLayer(LayerOp {
            clip: Some(clip),
            filter: None,
            blend: None,
            opacity: None,
        })));
    }

    fn pop_clip(&mut self) {
        if matches!(self.stack.last(), Some(PainterStackEntry::Clip(_))) {
            self.stack.pop();
            self.ops.push(ImagingOp::State(StateOp::PopLayer));
        }
    }

    #[allow(
        clippy::field_reassign_with_default,
        reason = "gradient fields are set in stages for clarity"
    )]
    fn fill(&mut self, brush: ColrBrush<'_>) {
        // Fill the current clip region with the specified brush. We interpret
        // the current clip as the geometry to be filled, similar to Vello's
        // COLR renderer. If there is no meaningful clip, there is nothing to
        // fill.

        // Determine a path to fill based on the current clip.
        let path = match self.current_clip() {
            Some(ClipOp::Fill {
                shape: ClipShape::Path(id),
                ..
            }) => Some(*id),
            Some(ClipOp::Fill {
                shape: ClipShape::Rect(rect),
                ..
            }) => {
                // Create a simple rectangular path for the clip box.
                let desc = PathDesc {
                    commands: vec![
                        PathCmd::MoveTo {
                            x: rect.x0,
                            y: rect.y0,
                        },
                        PathCmd::LineTo {
                            x: rect.x1,
                            y: rect.y0,
                        },
                        PathCmd::LineTo {
                            x: rect.x1,
                            y: rect.y1,
                        },
                        PathCmd::LineTo {
                            x: rect.x0,
                            y: rect.y1,
                        },
                        PathCmd::Close,
                    ]
                    .into_boxed_slice(),
                };
                Some(self.backend.create_path(desc))
            }
            Some(ClipOp::Fill {
                shape: ClipShape::RoundedRect(rr),
                ..
            }) => {
                // Create a simple rectangular path for the clip box.
                let rect = rr.rect;
                let desc = PathDesc {
                    commands: vec![
                        PathCmd::MoveTo {
                            x: rect.x0,
                            y: rect.y0,
                        },
                        PathCmd::LineTo {
                            x: rect.x1,
                            y: rect.y0,
                        },
                        PathCmd::LineTo {
                            x: rect.x1,
                            y: rect.y1,
                        },
                        PathCmd::LineTo {
                            x: rect.x0,
                            y: rect.y1,
                        },
                        PathCmd::Close,
                    ]
                    .into_boxed_slice(),
                };
                Some(self.backend.create_path(desc))
            }
            _ => None,
        };

        let Some(_path) = path else {
            return;
        };

        // Convert COLR brush into an imaging brush.
        let is_sweep = matches!(brush, ColrBrush::SweepGradient { .. });
        let brush = match brush {
            ColrBrush::Solid {
                palette_index,
                alpha,
            } => {
                // Palette index 0xFFFF indicates "use the context color".
                if palette_index == u16::MAX {
                    let color = self.context_color.multiply_alpha(alpha);
                    Brush::Solid(color)
                } else {
                    let palettes = self.font_ref.color_palettes();
                    let Some(palette) = palettes.get(0) else {
                        return;
                    };
                    let colors = palette.colors();
                    let Some(base) = colors.get(palette_index as usize) else {
                        return;
                    };

                    let base_alpha = base.alpha as f32 / 255.0;
                    let a = (base_alpha * alpha).clamp(0.0, 1.0);
                    let color = Color::from_rgba8(base.red, base.green, base.blue, alpha_to_u8(a));
                    Brush::Solid(color)
                }
            }
            ColrBrush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => {
                let extend_mode = match extend {
                    skrifa::color::Extend::Pad => peniko::Extend::Pad,
                    skrifa::color::Extend::Repeat => peniko::Extend::Repeat,
                    skrifa::color::Extend::Reflect => peniko::Extend::Reflect,
                    _ => peniko::Extend::Pad,
                };

                let palettes = self.font_ref.color_palettes();
                let palette = match palettes.get(0) {
                    Some(p) => p,
                    None => return,
                };
                let colors = palette.colors();

                let stops_raw: Vec<peniko::ColorStop> = color_stops
                    .iter()
                    .filter_map(|stop| {
                        let idx = stop.palette_index as usize;
                        colors.get(idx).map(|c| {
                            let base_alpha = c.alpha as f32 / 255.0;
                            let a = (base_alpha * stop.alpha).clamp(0.0, 1.0);
                            let color = Color::from_rgba8(c.red, c.green, c.blue, alpha_to_u8(a));
                            peniko::ColorStop::from((stop.offset, color))
                        })
                    })
                    .collect();
                let stops = Self::normalize_color_stops(stops_raw);
                if stops.len() == 1 {
                    let solid = stops[0].color.to_alpha_color::<peniko::color::Srgb>();
                    Brush::Solid(solid)
                } else {
                    let kind = peniko::GradientKind::Linear(peniko::LinearGradientPosition::new(
                        (p0.x as f64, p0.y as f64),
                        (p1.x as f64, p1.y as f64),
                    ));
                    Brush::Gradient(peniko::Gradient {
                        kind,
                        extend: extend_mode,
                        stops,
                        ..peniko::Gradient::default()
                    })
                }
            }
            ColrBrush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => {
                let extend_mode = match extend {
                    skrifa::color::Extend::Pad => peniko::Extend::Pad,
                    skrifa::color::Extend::Repeat => peniko::Extend::Repeat,
                    skrifa::color::Extend::Reflect => peniko::Extend::Reflect,
                    _ => peniko::Extend::Pad,
                };

                let palettes = self.font_ref.color_palettes();
                let palette = match palettes.get(0) {
                    Some(p) => p,
                    None => return,
                };
                let colors = palette.colors();

                let stops_raw: Vec<peniko::ColorStop> = color_stops
                    .iter()
                    .filter_map(|stop| {
                        let idx = stop.palette_index as usize;
                        colors.get(idx).map(|c| {
                            let base_alpha = c.alpha as f32 / 255.0;
                            let a = (base_alpha * stop.alpha).clamp(0.0, 1.0);
                            let color = Color::from_rgba8(c.red, c.green, c.blue, alpha_to_u8(a));
                            peniko::ColorStop::from((stop.offset, color))
                        })
                    })
                    .collect();
                let stops = Self::normalize_color_stops(stops_raw);
                if r1 <= 0.0 || stops.len() == 1 {
                    let solid = stops[0].color.to_alpha_color::<peniko::color::Srgb>();
                    Brush::Solid(solid)
                } else {
                    let kind = peniko::GradientKind::Radial(
                        peniko::RadialGradientPosition::new_two_point(
                            (c0.x as f64, c0.y as f64),
                            r0,
                            (c1.x as f64, c1.y as f64),
                            r1,
                        ),
                    );
                    Brush::Gradient(peniko::Gradient {
                        kind,
                        extend: extend_mode,
                        stops,
                        ..peniko::Gradient::default()
                    })
                }
            }
            ColrBrush::SweepGradient {
                c0,
                start_angle,
                mut end_angle,
                color_stops,
                extend,
            } => {
                let extend_mode = match extend {
                    skrifa::color::Extend::Pad => peniko::Extend::Pad,
                    skrifa::color::Extend::Repeat => peniko::Extend::Repeat,
                    skrifa::color::Extend::Reflect => peniko::Extend::Reflect,
                    _ => peniko::Extend::Pad,
                };
                let palettes = self.font_ref.color_palettes();
                let palette = match palettes.get(0) {
                    Some(p) => p,
                    None => return,
                };
                let colors = palette.colors();

                let stops_raw: Vec<peniko::ColorStop> = color_stops
                    .iter()
                    .filter_map(|stop| {
                        let idx = stop.palette_index as usize;
                        colors.get(idx).map(|c| {
                            let base_alpha = c.alpha as f32 / 255.0;
                            let a = (base_alpha * stop.alpha).clamp(0.0, 1.0);
                            let color = Color::from_rgba8(c.red, c.green, c.blue, alpha_to_u8(a));
                            peniko::ColorStop::from((stop.offset, color))
                        })
                    })
                    .collect();
                let stops = Self::normalize_color_stops(stops_raw);
                if stops.len() == 1 {
                    let solid = stops[0].color.to_alpha_color::<peniko::color::Srgb>();
                    Brush::Solid(solid)
                } else {
                    if start_angle == end_angle {
                        if matches!(extend, skrifa::color::Extend::Pad) {
                            end_angle += 0.01;
                        } else {
                            debug_assert!(
                                false,
                                "unexpected non-Pad extend for sweep with equal angles"
                            );
                        }
                    }
                    let kind = peniko::GradientKind::Sweep(peniko::SweepGradientPosition::new(
                        (c0.x as f64, -c0.y as f64),
                        start_angle.to_radians(),
                        end_angle.to_radians(),
                    ));
                    Brush::Gradient(peniko::Gradient {
                        kind,
                        extend: extend_mode,
                        stops,
                        ..peniko::Gradient::default()
                    })
                }
            }
        };

        let paint = self.backend.create_paint(PaintDesc { brush });

        // Large coverage rect; backends will clip this to the active clip
        // region (glyph clip, clip box, etc.).
        const BOUND: f32 = 100_000.0;
        let coverage_path = self.backend.create_path(PathDesc {
            commands: vec![
                PathCmd::MoveTo {
                    x: -BOUND,
                    y: -BOUND,
                },
                PathCmd::LineTo {
                    x: BOUND,
                    y: -BOUND,
                },
                PathCmd::LineTo { x: BOUND, y: BOUND },
                PathCmd::LineTo {
                    x: -BOUND,
                    y: BOUND,
                },
                PathCmd::Close,
            ]
            .into_boxed_slice(),
        });

        // For sweep gradients, apply an extra Y-flip in paint space so that
        // COLR's clockwise convention in a Y-up font space maps correctly
        // into Vello's Y-down paint space.
        if is_sweep {
            let paint_xf = self.current_transform * Affine::scale_non_uniform(1.0, -1.0);
            self.ops
                .push(ImagingOp::State(StateOp::SetPaintTransform(paint_xf)));
        }

        self.ops.push(ImagingOp::State(StateOp::SetPaint(paint)));
        self.ops.push(ImagingOp::State(StateOp::SetTransform(
            self.current_transform,
        )));
        self.ops
            .push(ImagingOp::Draw(DrawOp::FillPath(coverage_path)));
    }

    fn push_layer(&mut self, composite_mode: ColrCompositeMode) {
        // Map COLR composite modes into imaging groups using the same mapping
        // as Vello's COLR renderer.
        let blend = match composite_mode {
            ColrCompositeMode::Clear => BlendMode::new(Mix::Normal, Compose::Clear),
            ColrCompositeMode::Src => BlendMode::new(Mix::Normal, Compose::Copy),
            ColrCompositeMode::Dest => BlendMode::new(Mix::Normal, Compose::Dest),
            ColrCompositeMode::SrcOver => BlendMode::new(Mix::Normal, Compose::SrcOver),
            ColrCompositeMode::DestOver => BlendMode::new(Mix::Normal, Compose::DestOver),
            ColrCompositeMode::SrcIn => BlendMode::new(Mix::Normal, Compose::SrcIn),
            ColrCompositeMode::DestIn => BlendMode::new(Mix::Normal, Compose::DestIn),
            ColrCompositeMode::SrcOut => BlendMode::new(Mix::Normal, Compose::SrcOut),
            ColrCompositeMode::DestOut => BlendMode::new(Mix::Normal, Compose::DestOut),
            ColrCompositeMode::SrcAtop => BlendMode::new(Mix::Normal, Compose::SrcAtop),
            ColrCompositeMode::DestAtop => BlendMode::new(Mix::Normal, Compose::DestAtop),
            ColrCompositeMode::Xor => BlendMode::new(Mix::Normal, Compose::Xor),
            ColrCompositeMode::Plus => BlendMode::new(Mix::Normal, Compose::Plus),
            ColrCompositeMode::Screen => BlendMode::new(Mix::Screen, Compose::SrcOver),
            ColrCompositeMode::Overlay => BlendMode::new(Mix::Overlay, Compose::SrcOver),
            ColrCompositeMode::Darken => BlendMode::new(Mix::Darken, Compose::SrcOver),
            ColrCompositeMode::Lighten => BlendMode::new(Mix::Lighten, Compose::SrcOver),
            ColrCompositeMode::ColorDodge => BlendMode::new(Mix::ColorDodge, Compose::SrcOver),
            ColrCompositeMode::ColorBurn => BlendMode::new(Mix::ColorBurn, Compose::SrcOver),
            ColrCompositeMode::HardLight => BlendMode::new(Mix::HardLight, Compose::SrcOver),
            ColrCompositeMode::SoftLight => BlendMode::new(Mix::SoftLight, Compose::SrcOver),
            ColrCompositeMode::Difference => BlendMode::new(Mix::Difference, Compose::SrcOver),
            ColrCompositeMode::Exclusion => BlendMode::new(Mix::Exclusion, Compose::SrcOver),
            ColrCompositeMode::Multiply => BlendMode::new(Mix::Multiply, Compose::SrcOver),
            ColrCompositeMode::HslHue => BlendMode::new(Mix::Hue, Compose::SrcOver),
            ColrCompositeMode::HslSaturation => BlendMode::new(Mix::Saturation, Compose::SrcOver),
            ColrCompositeMode::HslColor => BlendMode::new(Mix::Color, Compose::SrcOver),
            ColrCompositeMode::HslLuminosity => BlendMode::new(Mix::Luminosity, Compose::SrcOver),
            ColrCompositeMode::Unknown => BlendMode::new(Mix::Normal, Compose::SrcOver),
        };

        self.stack.push(PainterStackEntry::Group);
        self.ops.push(ImagingOp::State(StateOp::PushLayer(LayerOp {
            clip: None,
            filter: None,
            blend: (blend != BlendMode::default()).then_some(blend),
            opacity: None,
        })));
    }

    fn pop_layer(&mut self) {
        if matches!(self.stack.last(), Some(PainterStackEntry::Group)) {
            self.stack.pop();
            self.ops.push(ImagingOp::State(StateOp::PopLayer));
        }
    }

    fn fill_glyph(
        &mut self,
        glyph_id: GlyphId,
        brush_transform: Option<ColrTransform>,
        brush: ColrBrush<'_>,
    ) {
        // Convert COLR brush into an imaging brush.
        let is_sweep = matches!(brush, ColrBrush::SweepGradient { .. });
        let brush = match brush {
            ColrBrush::Solid {
                palette_index,
                alpha,
            } => {
                // Palette index 0xFFFF indicates "use the context color".
                if palette_index == u16::MAX {
                    let color = self.context_color.multiply_alpha(alpha);
                    Brush::Solid(color)
                } else {
                    // Resolve palette color.
                    let palettes = self.font_ref.color_palettes();
                    let Some(palette) = palettes.get(0) else {
                        return;
                    };
                    let colors = palette.colors();
                    let Some(base) = colors.get(palette_index as usize) else {
                        return;
                    };

                    let base_alpha = base.alpha as f32 / 255.0;
                    let a = (base_alpha * alpha).clamp(0.0, 1.0);
                    let color = Color::from_rgba8(base.red, base.green, base.blue, alpha_to_u8(a));
                    Brush::Solid(color)
                }
            }
            ColrBrush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => {
                let extend_mode = match extend {
                    skrifa::color::Extend::Pad => peniko::Extend::Pad,
                    skrifa::color::Extend::Repeat => peniko::Extend::Repeat,
                    skrifa::color::Extend::Reflect => peniko::Extend::Reflect,
                    _ => peniko::Extend::Pad,
                };

                // Map COLR linear gradient stops via CPAL palette.
                let palettes = self.font_ref.color_palettes();
                let palette = match palettes.get(0) {
                    Some(p) => p,
                    None => return,
                };
                let colors = palette.colors();

                let stops_raw: Vec<peniko::ColorStop> = color_stops
                    .iter()
                    .filter_map(|stop| {
                        let idx = stop.palette_index as usize;
                        colors.get(idx).map(|c| {
                            let base_alpha = c.alpha as f32 / 255.0;
                            let a = (base_alpha * stop.alpha).clamp(0.0, 1.0);
                            let color = Color::from_rgba8(c.red, c.green, c.blue, alpha_to_u8(a));
                            peniko::ColorStop::from((stop.offset, color))
                        })
                    })
                    .collect();
                let stops = Self::normalize_color_stops(stops_raw);
                if stops.len() == 1 {
                    let solid = stops[0].color.to_alpha_color::<peniko::color::Srgb>();
                    Brush::Solid(solid)
                } else {
                    let kind = peniko::GradientKind::Linear(peniko::LinearGradientPosition::new(
                        (p0.x as f64, p0.y as f64),
                        (p1.x as f64, p1.y as f64),
                    ));
                    Brush::Gradient(peniko::Gradient {
                        kind,
                        extend: extend_mode,
                        stops,
                        ..peniko::Gradient::default()
                    })
                }
            }
            ColrBrush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => {
                let extend_mode = match extend {
                    skrifa::color::Extend::Pad => peniko::Extend::Pad,
                    skrifa::color::Extend::Repeat => peniko::Extend::Repeat,
                    skrifa::color::Extend::Reflect => peniko::Extend::Reflect,
                    _ => peniko::Extend::Pad,
                };

                // Map COLR radial gradient stops via CPAL palette.
                let palettes = self.font_ref.color_palettes();
                let palette = match palettes.get(0) {
                    Some(p) => p,
                    None => return,
                };
                let colors = palette.colors();

                let stops_raw: Vec<peniko::ColorStop> = color_stops
                    .iter()
                    .filter_map(|stop| {
                        let idx = stop.palette_index as usize;
                        colors.get(idx).map(|c| {
                            let base_alpha = c.alpha as f32 / 255.0;
                            let a = (base_alpha * stop.alpha).clamp(0.0, 1.0);
                            let color = Color::from_rgba8(c.red, c.green, c.blue, alpha_to_u8(a));
                            peniko::ColorStop::from((stop.offset, color))
                        })
                    })
                    .collect();
                let stops = Self::normalize_color_stops(stops_raw);
                if r1 <= 0.0 || stops.len() == 1 {
                    let solid = stops[0].color.to_alpha_color::<peniko::color::Srgb>();
                    Brush::Solid(solid)
                } else {
                    let kind = peniko::GradientKind::Radial(
                        peniko::RadialGradientPosition::new_two_point(
                            (c0.x as f64, c0.y as f64),
                            r0,
                            (c1.x as f64, c1.y as f64),
                            r1,
                        ),
                    );
                    Brush::Gradient(peniko::Gradient {
                        kind,
                        extend: extend_mode,
                        stops,
                        ..peniko::Gradient::default()
                    })
                }
            }
            ColrBrush::SweepGradient {
                c0,
                start_angle,
                mut end_angle,
                color_stops,
                extend,
            } => {
                let extend_mode = match extend {
                    skrifa::color::Extend::Pad => peniko::Extend::Pad,
                    skrifa::color::Extend::Repeat => peniko::Extend::Repeat,
                    skrifa::color::Extend::Reflect => peniko::Extend::Reflect,
                    _ => peniko::Extend::Pad,
                };

                let palettes = self.font_ref.color_palettes();
                let palette = match palettes.get(0) {
                    Some(p) => p,
                    None => return,
                };
                let colors = palette.colors();

                let stops_raw: Vec<peniko::ColorStop> = color_stops
                    .iter()
                    .filter_map(|stop| {
                        let idx = stop.palette_index as usize;
                        colors.get(idx).map(|c| {
                            let base_alpha = c.alpha as f32 / 255.0;
                            let a = (base_alpha * stop.alpha).clamp(0.0, 1.0);
                            let color = Color::from_rgba8(c.red, c.green, c.blue, alpha_to_u8(a));
                            peniko::ColorStop::from((stop.offset, color))
                        })
                    })
                    .collect();
                let stops = Self::normalize_color_stops(stops_raw);
                if stops.len() == 1 {
                    let solid = stops[0].color.to_alpha_color::<peniko::color::Srgb>();
                    Brush::Solid(solid)
                } else {
                    if start_angle == end_angle {
                        if matches!(extend, skrifa::color::Extend::Pad) {
                            end_angle += 0.01;
                        } else {
                            debug_assert!(
                                false,
                                "unexpected non-Pad extend for sweep with equal angles"
                            );
                        }
                    }
                    let kind = peniko::GradientKind::Sweep(peniko::SweepGradientPosition::new(
                        (c0.x as f64, -c0.y as f64),
                        start_angle.to_radians(),
                        end_angle.to_radians(),
                    ));
                    Brush::Gradient(peniko::Gradient {
                        kind,
                        extend: extend_mode,
                        stops,
                        ..peniko::Gradient::default()
                    })
                }
            }
        };

        let paint = self.backend.create_paint(PaintDesc { brush });

        // Create a path for this glyph in font units; the initial transform
        // established in `new` scales and flips into pixel space.
        let Some(path) =
            create_glyph_path_unscaled(self.backend, self.font_bytes, glyph_id, self.font_index)
        else {
            return;
        };

        // Compose transform.
        let mut xf = self.current_transform;
        if let Some(bt) = brush_transform {
            let a = Self::affine_from_colr(bt);
            xf *= a;
        }

        // For sweep gradients, apply an extra Y-flip in paint space.
        if is_sweep {
            let paint_xf = self.current_transform * Affine::scale_non_uniform(1.0, -1.0);
            self.ops
                .push(ImagingOp::State(StateOp::SetPaintTransform(paint_xf)));
        }

        self.ops.push(ImagingOp::State(StateOp::SetPaint(paint)));
        self.ops.push(ImagingOp::State(StateOp::SetTransform(xf)));
        self.ops.push(ImagingOp::Draw(DrawOp::FillPath(path)));
    }
}

/// Lower a single COLR glyph into a picture resource.
fn colr_glyph_to_picture<B>(
    backend: &mut B,
    font_bytes: &[u8],
    font_index: u32,
    glyph_id: GlyphId,
    font_size_px: f32,
    context_color: Color,
) -> Option<PictureId>
where
    B: ResourceBackend,
{
    let font_ref = FontRef::new(font_bytes).ok()?;
    let color_glyphs = font_ref.color_glyphs();
    let color_glyph = color_glyphs
        .get_with_format(glyph_id, ColorGlyphFormat::ColrV1)
        .or_else(|| color_glyphs.get(glyph_id))?;

    let mut painter = ImagingColorPainter::new(
        backend,
        font_bytes,
        font_index,
        font_size_px,
        font_ref,
        context_color,
    );

    // Use default location (no variation) for now.
    let result = color_glyph.paint(LocationRef::default(), &mut painter);
    // Defensively unwind any remaining COLR layers/clips so that a single
    // glyph picture cannot leak group or clip state into subsequent ops.
    painter.finalize();
    if result.is_err() || painter.ops.is_empty() {
        return None;
    }

    let ops = painter.ops.into_boxed_slice();
    // COLR glyph pictures contain only vector/gradient/image ops; hinting
    // decisions for text happen upstream in the text helper. The recorded
    // imaging program is therefore valid to replay under any affine
    // transform, so we advertise `Affine` here.
    let picture = backend.create_picture(PictureDesc {
        recording: RecordedOps {
            ops: ops.into(),
            acceleration: None,
            valid_under: TransformClass::Affine,
            original_ctm: None,
        },
    });
    Some(picture)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use peniko::{Brush, Color, ColorStop};
    use understory_imaging::{
        DrawOp, ImageId, ImagingOp, PaintDesc, PathDesc, ResourceBackend, StateOp,
    };

    #[test]
    fn normalize_color_stops_pads_ends() {
        // Single mid-stop should be expanded to [0.0, mid, 1.0].
        let mid = Color::from_rgba8(10, 20, 30, 255);
        let stops = vec![ColorStop::from((0.5, mid))];
        let normalized =
            ImagingColorPainter::<PictureCapturingBackend>::normalize_color_stops(stops);

        assert_eq!(normalized.len(), 3);
        assert!((normalized[0].offset - 0.0).abs() < f32::EPSILON);
        assert!((normalized[1].offset - 0.5).abs() < f32::EPSILON);
        assert!((normalized[2].offset - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn normalize_color_stops_dedup_trailing_ones() {
        let c0 = Color::from_rgba8(0, 0, 0, 255);
        let c1 = Color::from_rgba8(10, 20, 30, 255);
        let c2 = Color::from_rgba8(40, 50, 60, 255);

        let stops = vec![
            ColorStop::from((0.0, c0)),
            ColorStop::from((1.0, c1)),
            ColorStop::from((1.0, c2)),
        ];
        let normalized =
            ImagingColorPainter::<PictureCapturingBackend>::normalize_color_stops(stops);

        // We expect 2 stops: 0.0 and a single 1.0 (the last).
        assert_eq!(normalized.len(), 2);
        assert!((normalized[0].offset - 0.0).abs() < f32::EPSILON);
        assert!((normalized[1].offset - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn normalize_color_stops_keeps_properly_bounded_stops() {
        let c0 = Color::from_rgba8(0, 0, 0, 255);
        let c1 = Color::from_rgba8(10, 20, 30, 255);

        let stops = vec![ColorStop::from((0.0, c0)), ColorStop::from((1.0, c1))];
        let normalized =
            ImagingColorPainter::<PictureCapturingBackend>::normalize_color_stops(stops);

        assert_eq!(normalized.len(), 2);
        assert!((normalized[0].offset - 0.0).abs() < f32::EPSILON);
        assert!((normalized[1].offset - 1.0).abs() < f32::EPSILON);
    }

    /// Minimal backend that captures the last created picture description.
    #[derive(Default)]
    struct PictureCapturingBackend {
        next_path: u32,
        next_image: u32,
        next_paint: u32,
        next_picture: u32,
        last_picture: Option<PictureDesc>,
        last_paint: Option<PaintDesc>,
    }

    impl ResourceBackend for PictureCapturingBackend {
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
            self.last_paint = Some(_desc);
            PaintId(id)
        }

        fn destroy_paint(&mut self, _id: PaintId) {}

        fn create_picture(&mut self, desc: PictureDesc) -> PictureId {
            let id = self.next_picture;
            self.next_picture += 1;
            self.last_picture = Some(desc);
            PictureId(id)
        }

        fn destroy_picture(&mut self, _id: PictureId) {}
    }

    /// Solid COLR brushes that use the "context color" sentinel palette index
    /// should resolve to the COLR context color multiplied by the brush alpha.
    #[test]
    fn colr_context_color_sentinel_uses_context_color() {
        use crate::ColrPainter;

        // Use a simple outline font; any font with a valid `head` table is fine
        // here because we only exercise the brush <-> paint conversion.
        let font_bytes: &[u8] = include_bytes!("../../assets/fonts/roboto/Roboto-Regular.ttf");
        let font_ref = FontRef::new(font_bytes).expect("valid font");

        let mut backend = PictureCapturingBackend::default();

        // Pick a context color with a non-trivial alpha so we can observe the
        // effect of COLR's additional brush alpha.
        let context_color = Color::from_rgba8(10, 20, 30, 128);

        let mut painter =
            ImagingColorPainter::new(&mut backend, font_bytes, 0, 40.0, font_ref, context_color);

        // Ensure there is a non-infinite clip so that `fill` does work.
        ColrPainter::push_clip_box(
            &mut painter,
            BoundingBox {
                x_min: 0.0,
                y_min: 0.0,
                x_max: 10.0,
                y_max: 10.0,
            },
        );

        // Fill with a solid brush that uses the context color sentinel.
        ColrPainter::fill(
            &mut painter,
            ColrBrush::Solid {
                palette_index: u16::MAX,
                alpha: 0.5,
            },
        );

        let paint_desc = backend
            .last_paint
            .as_ref()
            .expect("expected paint to be created via context-color fill");

        match &paint_desc.brush {
            Brush::Solid(color) => {
                let expected = context_color.multiply_alpha(0.5);
                assert_eq!(
                    *color, expected,
                    "solid brush should use context color multiplied by COLR alpha"
                );
            }
            other => panic!("expected solid brush, got {other:?}"),
        }
    }

    /// Ensure that a COLR glyph from the Noto Color Emoji subset lowers into
    /// a picture with some draw/state ops (i.e. it is not empty).
    #[test]
    fn colr_subset_glyph_produces_ops() {
        // This subset font matches the one used in Vello's COLR emoji example.
        let font_bytes: &[u8] =
            include_bytes!("../../assets/fonts/noto_color_emoji/NotoColorEmoji-Subset.ttf");
        let font_ref = FontRef::new(font_bytes).expect("valid font");
        let cmap = font_ref.charmap();

        // Check mark emoji U+2705 is included in the subset.
        let ch = '✅';
        let gid = cmap.map(ch).expect("glyph id for check mark");

        let mut backend = PictureCapturingBackend::default();
        let picture = colr_glyph_to_picture(&mut backend, font_bytes, 0, gid, 40.0, Color::WHITE)
            .expect("expected COLR picture for subset glyph");

        assert_eq!(picture.0, 0, "first picture id should be 0");

        let desc = backend
            .last_picture
            .as_ref()
            .expect("picture description should be captured");
        assert!(
            !desc.recording.ops.is_empty(),
            "expected some imaging ops in COLR picture"
        );

        let paint_count = desc
            .recording
            .ops
            .iter()
            .filter(|op| matches!(op, ImagingOp::State(StateOp::SetPaint(_))))
            .count();
        let fill_count = desc
            .recording
            .ops
            .iter()
            .filter(|op| matches!(op, ImagingOp::Draw(DrawOp::FillPath(_))))
            .count();

        assert!(
            paint_count >= 2,
            "expected at least two SetPaint ops (green box + white check), found {}",
            paint_count
        );
        assert!(
            fill_count >= 2,
            "expected at least two FillPath ops (green box + white check), found {}",
            fill_count
        );
    }

    /// Unbalanced COLR layers should be defensively unwound so that glyph
    /// pictures do not leak `PushLayer` without matching `PopLayer`.
    #[test]
    fn imaging_color_painter_unwinds_layers_and_clips() {
        let font_bytes: &[u8] = include_bytes!("../../assets/fonts/roboto/Roboto-Regular.ttf");
        let font_ref = FontRef::new(font_bytes).expect("valid font");

        let mut backend = PictureCapturingBackend::default();
        let context_color = Color::from_rgba8(200, 100, 50, 255);

        let mut painter =
            ImagingColorPainter::new(&mut backend, font_bytes, 0, 40.0, font_ref, context_color);

        // Simulate a COLR paint graph that pushes a layer and clip but forgets
        // to pop them. We call the trait methods directly.
        painter.push_layer(ColrCompositeMode::SrcOver);
        painter.push_clip_box(BoundingBox {
            x_min: 0.0,
            y_min: 0.0,
            x_max: 10.0,
            y_max: 10.0,
        });

        // Finalize should emit balancing PopLayer ops and
        // reset the internal stack.
        painter.finalize();

        // Expect the unmatched clip and layer to be unwound.
        use understory_imaging::{ImagingOp, StateOp};

        assert!(
            painter
                .ops
                .iter()
                .any(|op| matches!(op, ImagingOp::State(StateOp::PopLayer))),
            "expected at least one PopLayer emitted during finalize"
        );

        assert!(
            painter.stack.is_empty(),
            "expected stack to be empty after finalize"
        );

        let tail = &painter.ops[painter.ops.len().saturating_sub(2)..];
        assert!(
            matches!(
                tail,
                [
                    ImagingOp::State(StateOp::PopLayer),
                    ImagingOp::State(StateOp::PopLayer)
                ]
            ),
            "expected finalize to end with two PopLayer ops, got {tail:?}"
        );
    }

    /// Glyph-id runs should fall back to bitmap strikes when no outlines are
    /// available for the requested glyphs.
    #[test]
    fn glyph_ids_run_uses_bitmaps_when_no_outlines() {
        use understory_imaging_ref::RefBackend;

        // Bitmap-only Noto Color Emoji subset used in Vello examples.
        let font_bytes: &[u8] =
            include_bytes!("../../assets/fonts/noto_color_emoji/NotoColorEmoji-CBTF-Subset.ttf");
        let font_ref = FontRef::new(font_bytes).expect("valid font");
        let cmap = font_ref.charmap();

        // Pick one of the emoji known to be present in the subset.
        let ch = '🎉';
        let _gid = cmap.map(ch).expect("glyph id for party popper");

        let mut backend = RefBackend::default();

        // Create a dummy paint; bitmap rendering ignores it but the API
        // expects a paint id.
        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::WHITE),
        });

        let base = Affine::IDENTITY;

        // Ensure that bitmap-only fonts still hit the bitmap path when
        // addressed via Unicode text. This exercises `draw_text_run`'s
        // bitmap fallback.
        draw_text_run(
            &mut backend,
            font_bytes,
            &ch.to_string(),
            0,
            40.0,
            paint,
            base,
            TextHinting::Hinted,
        );

        // Verify that at least one DrawImage op was emitted.
        use understory_imaging::{DrawOp, ImagingOp};

        let image_draws = backend
            .ops()
            .iter()
            .filter(|op| matches!(op, ImagingOp::Draw(DrawOp::DrawImage { .. })))
            .count();

        assert!(
            image_draws > 0,
            "expected bitmap fallback to emit DrawImage ops for bitmap-only font"
        );
    }
}
