// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lowering helpers between retained display trees and imaging.

use imaging::{
    Composite, Painter,
    record::{self, Glyph},
};
use kurbo::{Affine, RoundedRect};
use peniko::BlendMode;
use understory_display::{DisplayNode, DisplayNodeKind, DisplayTree};

/// Lower one retained display tree into an imaging recording.
///
/// The `scale_factor` scales from logical display-tree coordinates to physical
/// pixels. Pass `1.0` when logical and physical coordinates are the same.
#[must_use]
pub fn imaging_scene_from_display_tree(tree: &DisplayTree, scale_factor: f64) -> record::Scene {
    let mut scene = record::Scene::new();
    {
        let mut painter = Painter::new(&mut scene);
        let root_transform = if (scale_factor - 1.0).abs() < f64::EPSILON {
            Affine::IDENTITY
        } else {
            Affine::scale(scale_factor)
        };
        record_node(&mut painter, tree.root(), root_transform);
    }
    scene
}

fn record_node(painter: &mut Painter<'_, record::Scene>, node: &DisplayNode, transform: Affine) {
    match node.kind() {
        DisplayNodeKind::Stack { children } => {
            for child in children {
                record_node(painter, child, transform);
            }
        }
        DisplayNodeKind::Padding { child, .. }
        | DisplayNodeKind::Align { child, .. }
        | DisplayNodeKind::Offset { child, .. }
        | DisplayNodeKind::FixedFrame { child, .. } => {
            record_node(painter, child, transform);
        }
        DisplayNodeKind::Transform {
            transform: node_transform,
            child,
        } => {
            record_node(painter, child, transform * *node_transform);
        }
        DisplayNodeKind::ClipRect { child } => {
            painter.with_fill_clip_transformed(node.layout().rect(), transform, |painter| {
                record_node(painter, child, transform);
            });
        }
        DisplayNodeKind::Opacity { opacity, child } => {
            if *opacity <= 0.0 {
                return;
            }
            if *opacity >= 1.0 {
                record_node(painter, child, transform);
                return;
            }
            painter.with_group(
                imaging::GroupRef::new()
                    .with_composite(Composite::new(BlendMode::default(), *opacity)),
                |painter| {
                    record_node(painter, child, transform);
                },
            );
        }
        DisplayNodeKind::FillRect { brush } => {
            painter
                .fill(node.layout().rect(), brush)
                .transform(transform)
                .draw();
        }
        DisplayNodeKind::StrokeRect { stroke, brush } => {
            painter
                .stroke(node.layout().rect(), stroke, brush)
                .transform(transform)
                .draw();
        }
        DisplayNodeKind::FillRoundedRect {
            corner_radius,
            brush,
        } => {
            let rect = RoundedRect::from_rect(node.layout().rect(), *corner_radius);
            painter.fill(rect, brush).transform(transform).draw();
        }
        DisplayNodeKind::StrokeRoundedRect {
            corner_radius,
            stroke,
            brush,
        } => {
            let rect = RoundedRect::from_rect(node.layout().rect(), *corner_radius);
            painter
                .stroke(rect, stroke, brush)
                .transform(transform)
                .draw();
        }
        DisplayNodeKind::Text(display_text) => {
            for run in display_text.runs() {
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
        // Placeholders produce no visual output — they reserve space only.
        DisplayNodeKind::Placeholder { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peniko::{Brush, Color};
    use understory_display::{BoxConstraints, DisplayAlign, Insets, TextAlign, TextEngine};

    #[test]
    fn display_tree_text_lowering_produces_imaging_commands() {
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

        let scene = imaging_scene_from_display_tree(&tree, 1.0);
        assert!(
            !scene.commands().is_empty(),
            "expected retained imaging commands"
        );
    }

    #[test]
    fn imaging_lowering_handles_clip_opacity_transform() {
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

        let scene = imaging_scene_from_display_tree(&tree, 1.0);
        assert!(
            !scene.commands().is_empty(),
            "expected retained imaging commands"
        );
    }
}
