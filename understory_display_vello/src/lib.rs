// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Vello backend for `understory_display`.
//!
//! This crate provides helpers to record an [`understory_display::DisplayList`]
//! into a Vello [`vello::Scene`]. It intentionally stays agnostic about window
//! and surface management; those responsibilities live in host applications or
//! examples.
//!
//! ## Overview
//!
//! - You build a [`understory_display::DisplayList`] in **logical coordinates**
//!   (typically logical pixels).
//! - You implement [`ResourceResolver`] to provide paths, paints, strokes, images,
//!   and (optionally) clip shapes for the ids referenced in the list.
//! - You call [`record_scene`] with:
//!   - the display list,
//!   - your resolver,
//!   - a mutable Vello [`Scene`],
//!   - and an [`Affine`] transform mapping logical space into the Vello scene
//!     (for example, scaling by the current window scale factor).
//! - The adapter lowers `FillPath`, `StrokePath`, `Image`, `PushClip`/`PopClip`,
//!   and `Group` ops into Vello drawing commands and layers.
//! - `GlyphRun` ops are currently **ignored**; once the text stack is in place,
//!   a text resolver will be able to map `RunId` handles into glyph runs that
//!   can be drawn via `Scene::draw_glyphs`.
//!
//! This crate does **not**:
//!
//! - create or manage wgpu devices, surfaces, or swapchains,
//! - cache glyphs or images,
//! - own any windowing integration.
//!
//! Those responsibilities are handled by host crates (see `examples` for
//! end-to-end usage with winit and `vello::util::RenderContext`).
//!
//! ## Resource ownership patterns
//!
//! In a real application, you typically do not want to hard-code magic ids
//! inside a [`ResourceResolver`] implementation. A more ergonomic approach is
//! to treat the display-list ids as stable handles into a small resource arena
//! owned by your UI/view layer.

extern crate alloc;

use kurbo::{Affine, BezPath, Rect, Stroke};
use understory_display::{ClipId, DisplayList, ImageId, Op, PathId, StrokeId};
use vello::Scene;
use vello::peniko::{BlendMode as VelloBlendMode, Brush, Fill, ImageBrush};

/// Resolve display-list resources into Vello primitives.
///
/// This keeps the backend independent of how paths, glyph runs, images, paints,
/// and clips are stored in the host application. The adapter will skip any ops
/// whose resources cannot be resolved.
pub trait ResourceResolver {
    /// Look up the path geometry for the given id.
    ///
    /// Returning `None` will cause any ops that reference this path to be
    /// skipped when recording the scene.
    fn path(&self, id: PathId) -> Option<BezPath>;
    /// Look up an image brush for the given id.
    ///
    /// Returning `None` will cause any ops that reference this image to be
    /// skipped when recording the scene.
    fn image(&self, id: ImageId) -> Option<ImageBrush>;
    /// Look up stroke style parameters for the given id.
    ///
    /// Returning `None` will cause any ops that reference this stroke to be
    /// skipped when recording the scene.
    fn stroke(&self, id: StrokeId) -> Option<Stroke>;
    /// Look up a paint brush for the given id.
    ///
    /// Returning `None` will cause any ops that reference this paint to be
    /// skipped when recording the scene.
    fn paint(&self, id: understory_display::PaintId) -> Option<Brush>;
    /// Look up the clip shape for the given id.
    ///
    /// Returning `None` will cause any clip ops that reference this id to be
    /// skipped when recording the scene.
    fn clip_path(&self, id: ClipId) -> Option<BezPath>;
}

/// Record a [`DisplayList`] into a Vello [`Scene`].
///
/// The `transform` argument maps the display list's coordinate system into
/// the Vello scene's coordinate system (for example, from logical pixels
/// into physical pixels using a scale factor).
///
/// Typical usage is to clear the scene, choose a logical→device transform,
/// then call [`record_scene`] before handing the scene to `vello::Renderer`.
pub fn record_scene<R: ResourceResolver>(
    list: &DisplayList,
    resolver: &R,
    scene: &mut Scene,
    transform: Affine,
) {
    let mut in_group = false;

    for op in &list.ops {
        match op {
            Op::FillPath {
                header: _,
                path,
                paint,
            } => {
                if let (Some(shape), Some(brush)) = (resolver.path(*path), resolver.paint(*paint)) {
                    scene.fill(Fill::NonZero, transform, &brush, None, &shape);
                }
            }
            Op::StrokePath {
                header: _,
                path,
                stroke,
                paint,
                ..
            } => {
                if let (Some(shape), Some(stroke), Some(brush)) = (
                    resolver.path(*path),
                    resolver.stroke(*stroke),
                    resolver.paint(*paint),
                ) {
                    scene.stroke(&stroke, transform, &brush, None, &shape);
                }
            }
            Op::GlyphRun { .. } => {
                // Glyph runs need additional shaping context; for now we expect
                // callers to encode text as paths or handle glyphs separately.
            }
            Op::Image {
                header: _,
                image,
                dest,
                ..
            } => {
                if let Some(img) = resolver.image(*image) {
                    let local = Affine::translate(dest.origin().to_vec2())
                        * Affine::scale_non_uniform(dest.width(), dest.height());
                    scene.draw_image(&img, transform * local);
                }
            }
            Op::PushClip { header: _, clip } => {
                if let Some(shape) = resolver.clip_path(*clip) {
                    scene.push_clip_layer(transform, &shape);
                }
            }
            Op::PopClip { .. } => {
                scene.pop_layer();
            }
            Op::Group {
                header: _,
                opacity,
                blend,
            } => {
                if in_group {
                    scene.pop_layer();
                    in_group = false;
                }

                if *opacity <= 0.0 {
                    continue;
                }

                let alpha = opacity.clamp(0.0, 1.0);
                let mode: VelloBlendMode = *blend;
                let clip_rect = Rect::new(
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                    f64::INFINITY,
                    f64::INFINITY,
                );
                scene.push_layer(mode, alpha, transform, &clip_rect);
                in_group = true;
            }
        }
    }

    if in_group {
        scene.pop_layer();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Rect;
    use understory_display::{ClipId, DisplayListBuilder, GroupId, PaintId, PathId};
    use vello::peniko::Color;

    struct TestResolver;

    impl ResourceResolver for TestResolver {
        fn path(&self, id: PathId) -> Option<BezPath> {
            if id.0 != 1 {
                return None;
            }
            let mut p = BezPath::new();
            p.move_to((0.0, 0.0));
            p.line_to((10.0, 0.0));
            p.line_to((10.0, 10.0));
            p.line_to((0.0, 10.0));
            p.close_path();
            Some(p)
        }

        fn image(&self, _id: ImageId) -> Option<ImageBrush> {
            None
        }

        fn stroke(&self, _id: StrokeId) -> Option<Stroke> {
            None
        }

        fn paint(&self, id: PaintId) -> Option<Brush> {
            if id.0 != 1 {
                return None;
            }
            Some(Brush::Solid(Color::WHITE))
        }

        fn clip_path(&self, id: ClipId) -> Option<BezPath> {
            if id.0 != 1 {
                return None;
            }
            let clip = Rect::new(0.0, 0.0, 5.0, 5.0);
            let mut p = BezPath::new();
            p.move_to((clip.x0, clip.y0));
            p.line_to((clip.x1, clip.y0));
            p.line_to((clip.x1, clip.y1));
            p.line_to((clip.x0, clip.y1));
            p.close_path();
            Some(p)
        }
    }

    #[test]
    fn record_scene_emits_encoding() {
        let mut builder = DisplayListBuilder::new(GroupId(0));
        builder.push_push_clip(0, Rect::new(0.0, 0.0, 5.0, 5.0), ClipId(1), None);
        builder.push_fill_path(
            0,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            PathId(1),
            PaintId(1),
            None,
        );
        builder.push_pop_clip(0, Rect::new(0.0, 0.0, 5.0, 5.0), None);
        let list = builder.finish();

        let mut scene = Scene::new();
        let xf = Affine::IDENTITY;
        record_scene(&list, &TestResolver, &mut scene, xf);

        let encoding = scene.encoding();
        assert!(
            !encoding.draw_tags.is_empty(),
            "expected some draw tags in encoding"
        );
    }
}
