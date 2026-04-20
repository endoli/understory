// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lowering helpers between retained display lists and imaging.

use imaging::{
    Composite, Painter,
    record::{self, Glyph},
};
use kurbo::Affine;
use peniko::BlendMode;
use understory_display::{DisplayEntry, DisplayItem, DisplayList, DisplayOp};

/// Lower one retained display list into an imaging recording.
#[must_use]
pub fn imaging_scene_from_display(list: &DisplayList) -> record::Scene {
    let mut scene = record::Scene::new();
    {
        let mut painter = Painter::new(&mut scene);
        let mut index = 0;
        record_entries(&mut painter, list.entries(), &mut index, Affine::IDENTITY);
    }
    scene
}

fn record_entries(
    painter: &mut Painter<'_, record::Scene>,
    entries: &[DisplayEntry],
    index: &mut usize,
    transform: Affine,
) {
    while *index < entries.len() {
        match &entries[*index] {
            DisplayEntry::Item(item) => {
                record_item(painter, item, transform);
                *index += 1;
            }
            DisplayEntry::PushClipRect(clip) => {
                *index += 1;
                painter.with_fill_clip_transformed(clip.rect, transform, |painter| {
                    record_entries(painter, entries, index, transform);
                });
            }
            DisplayEntry::PopClip => {
                *index += 1;
                return;
            }
            DisplayEntry::PushOpacity(opacity) => {
                *index += 1;
                painter.with_group(
                    imaging::GroupRef::new()
                        .with_composite(Composite::new(BlendMode::default(), opacity.opacity)),
                    |painter| {
                        record_entries(painter, entries, index, transform);
                    },
                );
            }
            DisplayEntry::PopOpacity => {
                *index += 1;
                return;
            }
            DisplayEntry::PushTransform(scope) => {
                *index += 1;
                record_entries(painter, entries, index, transform * scope.transform);
            }
            DisplayEntry::PopTransform => {
                *index += 1;
                return;
            }
        }
    }
}

fn record_item(painter: &mut Painter<'_, record::Scene>, item: &DisplayItem, transform: Affine) {
    match &item.op {
        DisplayOp::FillRect { rect, brush } => {
            painter.fill(*rect, brush).transform(transform).draw();
        }
        DisplayOp::StrokeRect {
            rect,
            stroke,
            brush,
        } => {
            painter
                .stroke(*rect, stroke, brush)
                .transform(transform)
                .draw();
        }
        DisplayOp::FillRoundedRect { rect, brush } => {
            painter.fill(*rect, brush).transform(transform).draw();
        }
        DisplayOp::StrokeRoundedRect {
            rect,
            stroke,
            brush,
        } => {
            painter
                .stroke(*rect, stroke, brush)
                .transform(transform)
                .draw();
        }
        DisplayOp::GlyphRun { run } => {
            let glyphs = run
                .glyphs
                .iter()
                .map(|glyph| Glyph {
                    id: glyph.id,
                    x: glyph.origin.x as f32,
                    y: glyph.origin.y as f32,
                })
                .collect::<Vec<_>>();
            painter
                .glyphs(&run.font, &run.brush)
                .transform(transform)
                .font_size(run.font_size)
                .normalized_coords(&run.normalized_coords)
                .draw(&peniko::Style::Fill(peniko::Fill::NonZero), &glyphs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peniko::{Brush, Color};
    use understory_display::{
        BoxConstraints, DisplayAlign, DisplayNode, DisplayTree, Insets, TextAlign, TextEngine,
    };

    #[test]
    fn display_tree_text_lowering_produces_positioned_glyphs() {
        let mut text = TextEngine::new();
        let mut tree = DisplayTree::new(DisplayNode::fixed_frame(
            kurbo::Size::new(160.0, 48.0),
            DisplayNode::stack(vec![
                DisplayNode::fill_rounded_rect(10.0, Brush::Solid(Color::from_rgb8(240, 240, 240))),
                DisplayNode::align(
                    DisplayAlign::Start,
                    DisplayAlign::Center,
                    DisplayNode::padding(
                        Insets::symmetric(16.0, 0.0),
                        DisplayNode::text(
                            "Overstory",
                            Brush::Solid(Color::BLACK),
                            21.0,
                            "sans-serif",
                            TextAlign::Start,
                        ),
                    ),
                ),
            ]),
        ));
        tree.layout(
            &mut text,
            kurbo::Point::ORIGIN,
            BoxConstraints::tight(kurbo::Size::new(160.0, 48.0)),
        );

        let list = tree.to_display_list();
        let glyph_count: usize = list
            .items()
            .filter_map(|item| match &item.op {
                DisplayOp::GlyphRun { run } => Some(run.glyphs.len()),
                _ => None,
            })
            .sum();
        assert!(glyph_count > 0, "expected at least one positioned glyph");
    }

    #[test]
    fn imaging_lowering_handles_structural_entries() {
        let mut text = TextEngine::new();
        let mut tree = DisplayTree::new(DisplayNode::clip_rect(DisplayNode::opacity(
            0.75,
            DisplayNode::transform(
                Affine::translate((8.0, 6.0)),
                DisplayNode::fixed_frame(
                    kurbo::Size::new(120.0, 40.0),
                    DisplayNode::stack(vec![
                        DisplayNode::fill_rounded_rect(
                            8.0,
                            Brush::Solid(Color::from_rgb8(240, 240, 240)),
                        ),
                        DisplayNode::align(
                            DisplayAlign::Start,
                            DisplayAlign::Center,
                            DisplayNode::padding(
                                Insets::symmetric(16.0, 0.0),
                                DisplayNode::text(
                                    "Layered",
                                    Brush::Solid(Color::BLACK),
                                    21.0,
                                    "sans-serif",
                                    TextAlign::Start,
                                ),
                            ),
                        ),
                    ]),
                ),
            ),
        )));
        tree.layout(
            &mut text,
            kurbo::Point::ORIGIN,
            BoxConstraints::tight(kurbo::Size::new(120.0, 40.0)),
        );

        let list = tree.to_display_list();
        let scene = imaging_scene_from_display(&list);
        assert!(
            !scene.commands().is_empty(),
            "expected retained imaging commands"
        );
    }
}
