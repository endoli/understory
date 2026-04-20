// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Retained glyph data for display text nodes.

use alloc::vec::Vec;

use kurbo::{Point, Rect, Vec2};
use peniko::Brush;

/// One positioned glyph within a retained glyph run.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayGlyph {
    /// Glyph identifier within the selected font.
    pub id: u32,
    /// Glyph draw origin in display/user space.
    pub origin: Point,
}

/// One retained glyph run with font data and positioned glyphs.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayGlyphRun {
    /// Font resource referenced by this run.
    pub font: parley::FontData,
    /// Font size used during shaping.
    pub font_size: f32,
    /// Normalized variation coordinates used during shaping.
    pub normalized_coords: Vec<i16>,
    /// Brush used to paint the glyphs in this run.
    pub brush: Brush,
    /// Glyphs positioned in display/user space.
    pub glyphs: Vec<DisplayGlyph>,
    /// Conservative logical bounds for the run.
    pub bounds: Rect,
}

impl DisplayGlyphRun {
    /// Returns a translated copy of the run.
    #[must_use]
    pub fn translated(&self, delta: Vec2) -> Self {
        let mut translated = self.clone();
        translated.translate(delta);
        translated
    }

    /// Translates the run in place.
    pub fn translate(&mut self, delta: Vec2) {
        self.bounds = self.bounds + delta;
        for glyph in &mut self.glyphs {
            glyph.origin += delta;
        }
    }
}
