// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal Parley-backed text shaping for retained glyph runs.

use alloc::borrow::Cow;
use alloc::vec::Vec;
use core::fmt;

use kurbo::Rect;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};
use peniko::Brush;

use crate::{DisplayGlyph, DisplayGlyphRun, TextAlign};

/// Shared Parley shaping resources for retained display glyph runs.
#[derive(Clone, Default)]
pub struct TextEngine {
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
}

impl fmt::Debug for TextEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TextEngine { .. }")
    }
}

impl TextEngine {
    /// Returns mutable references to both the font context and layout context.
    pub fn contexts(&mut self) -> (&mut FontContext, &mut LayoutContext<Brush>) {
        (&mut self.font_cx, &mut self.layout_cx)
    }

    /// Creates a new text engine with its own Parley font and layout contexts.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Shapes one text string into retained glyph runs.
    #[must_use]
    pub fn shape_text(&mut self, request: &TextRunRequest<'_>) -> Vec<DisplayGlyphRun> {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, request.text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(
            Cow::Borrowed(request.font_family),
        )));
        builder.push_default(StyleProperty::FontSize(request.font_size));
        builder.push_default(StyleProperty::Brush(request.brush.clone()));

        let mut layout = builder.build(request.text);
        layout.break_all_lines(request.max_advance);
        layout.align(
            request.max_advance,
            request.alignment.into(),
            AlignmentOptions::default(),
        );

        let mut runs = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };

                let run = glyph_run.run();
                let metrics = run.metrics();
                let mut glyph_origin_x = glyph_run.offset();
                let baseline_y = glyph_run.baseline();
                let bounds = Rect::new(
                    glyph_run.offset() as f64,
                    (baseline_y - metrics.ascent) as f64,
                    (glyph_run.offset() + glyph_run.advance()) as f64,
                    (baseline_y + metrics.descent) as f64,
                );

                let glyphs = glyph_run
                    .glyphs()
                    .map(|glyph| {
                        let origin = kurbo::Point::new(
                            f64::from(glyph_origin_x + glyph.x),
                            f64::from(baseline_y - glyph.y),
                        );
                        glyph_origin_x += glyph.advance;
                        DisplayGlyph {
                            id: glyph.id,
                            origin,
                        }
                    })
                    .collect();

                runs.push(DisplayGlyphRun {
                    font: run.font().clone(),
                    font_size: run.font_size(),
                    normalized_coords: run.normalized_coords().to_vec(),
                    brush: glyph_run.style().brush.clone(),
                    glyphs,
                    bounds,
                });
            }
        }

        runs
    }

    /// Measures the size of a text string with given font properties and max width.
    ///
    /// Returns the bounding size after Parley text shaping and line breaking.
    /// This is the accurate measurement primitive for layout — no heuristics.
    #[must_use]
    pub fn measure_text(
        &mut self,
        text: &str,
        font_size: f32,
        font_family: &str,
        max_width: Option<f32>,
    ) -> kurbo::Size {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(
            Cow::Borrowed(font_family),
        )));
        builder.push_default(StyleProperty::FontSize(font_size));
        builder.push_default(StyleProperty::Brush(Brush::Solid(peniko::Color::BLACK)));

        let mut layout = builder.build(text);
        layout.break_all_lines(max_width);
        layout.align(max_width, Alignment::Start, AlignmentOptions::default());

        let width = layout.full_width() as f64;
        let height = layout.height() as f64;
        kurbo::Size::new(width, height)
    }
}

/// Simple text shaping request for one retained label or line of text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRunRequest<'a> {
    /// Text content to shape.
    pub text: &'a str,
    /// Brush used for glyph painting.
    pub brush: Brush,
    /// Font size in display/user space.
    pub font_size: f32,
    /// CSS-like font-family source string, for example `"system-ui, sans-serif"`.
    pub font_family: &'a str,
    /// Optional wrap width.
    pub max_advance: Option<f32>,
    /// Requested paragraph alignment.
    pub alignment: TextAlign,
}

impl From<TextAlign> for Alignment {
    fn from(value: TextAlign) -> Self {
        match value {
            TextAlign::Start => Self::Start,
            TextAlign::Center => Self::Center,
            TextAlign::End => Self::End,
        }
    }
}
