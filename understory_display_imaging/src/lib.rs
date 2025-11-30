// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lowering helpers from `understory_display` ops to `understory_imaging` IR.
//!
//! This crate provides a minimal, opinionated mapping from display list
//! operations into imaging ops suitable for feeding an `ImagingBackend`.
//!
//! The current implementation focuses on fills and images and intentionally
//! ignores strokes, glyph runs, groups, and clips. Those will be added once
//! the corresponding imaging semantics are fully in place.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use understory_display as display;
use understory_imaging as imaging;

/// Lower a [`display::DisplayList`] into a sequence of imaging operations.
///
/// This mapping is intentionally conservative:
/// - `FillPath` becomes `SetPaint` + `FillPath`.
/// - `Image` becomes a `DrawImage` with an affine transform that maps a unit
///   rectangle to the destination rect.
/// - Other ops are currently ignored.
pub fn lower_display_to_imaging(list: &display::DisplayList) -> Vec<imaging::ImagingOp> {
    let mut out = Vec::new();
    for op in &list.ops {
        match op {
            display::Op::FillPath { path, paint, .. } => {
                out.push(imaging::ImagingOp::State(imaging::StateOp::SetPaint(
                    imaging::PaintId(paint.0),
                )));
                out.push(imaging::ImagingOp::Draw(imaging::DrawOp::FillPath(
                    imaging::PathId(path.0),
                )));
            }
            display::Op::Image {
                image,
                dest,
                opacity,
                ..
            } => {
                // Map the destination rect into an affine transform that takes
                // the unit rect [0, 0]..[1, 1] into `dest`.
                let w = dest.width();
                let h = dest.height();
                let tx = dest.x0;
                let ty = dest.y0;
                let transform =
                    imaging::Affine::translate((tx, ty)) * imaging::Affine::scale_non_uniform(w, h);

                if (*opacity - 1.0).abs() > f32::EPSILON {
                    out.push(imaging::ImagingOp::State(imaging::StateOp::SetOpacity(
                        *opacity,
                    )));
                }

                out.push(imaging::ImagingOp::Draw(imaging::DrawOp::DrawImage {
                    image: imaging::ImageId(image.0),
                    transform,
                }));
            }
            _ => {
                // Other ops (strokes, glyph runs, clips, groups) require richer
                // imaging semantics; they are intentionally ignored for now.
                // In particular, `StrokePath` lowering is TBD: we expect to
                // use `kurbo::Stroke` via `understory_imaging::StrokeStyle`,
                // but have not yet chosen between inlining stroke styles in
                // the display list vs introducing a `StrokeId` resolver.
            }
        }
    }
    out
}

/// Feed a display list directly into an imaging backend.
///
/// This is a convenience wrapper around [`lower_display_to_imaging`].
pub fn record_into_backend<B: imaging::ImagingBackend>(
    backend: &mut B,
    list: &display::DisplayList,
) {
    for op in lower_display_to_imaging(list) {
        match op {
            imaging::ImagingOp::State(s) => backend.state(s),
            imaging::ImagingOp::Draw(d) => backend.draw(d),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use display::{DisplayListBuilder, GroupId, ImageId, PaintId, PathId, SemanticRegionId};
    use kurbo::{Point, Rect};
    use understory_imaging::{
        DrawOp, ImageId as ImagingImageId, ImagingBackend, ImagingOp, PaintId as ImagingPaintId,
        PathId as ImagingPathId, ResourceBackend, StateOp,
    };
    use understory_imaging_ref::RefBackend;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect::new(x0, y0, x1, y1)
    }

    #[test]
    fn lowers_fill_and_image_ops() {
        let mut b = DisplayListBuilder::new(GroupId(1));
        b.push_fill_path(
            0,
            rect(0.0, 0.0, 10.0, 10.0),
            PathId(1),
            PaintId(2),
            Some(SemanticRegionId(7)),
        );
        b.push_image(1, rect(10.0, 10.0, 20.0, 20.0), ImageId(3), 0.5, None);
        let list = b.finish();

        let ops = lower_display_to_imaging(&list);
        assert_eq!(ops.len(), 4);

        match &ops[0] {
            ImagingOp::State(StateOp::SetPaint(id)) => assert_eq!(id.0, 2),
            _ => panic!("expected SetPaint"),
        }
        match &ops[1] {
            ImagingOp::Draw(DrawOp::FillPath(id)) => assert_eq!(id.0, 1),
            _ => panic!("expected FillPath"),
        }
        match &ops[2] {
            ImagingOp::State(StateOp::SetOpacity(v)) => assert!((*v - 0.5).abs() < 1e-6),
            _ => panic!("expected SetOpacity"),
        }
        match &ops[3] {
            ImagingOp::Draw(DrawOp::DrawImage { image, transform }) => {
                assert_eq!(image.0, 3);
                let p = (*transform) * Point::new(0.0, 0.0);
                assert_eq!(p, Point::new(10.0, 10.0));
            }
            _ => panic!("expected DrawImage"),
        }
    }

    /// Minimal backend used to test `record_into_backend`.
    #[derive(Default)]
    struct TestBackend {
        ops: Vec<ImagingOp>,
    }

    impl ResourceBackend for TestBackend {
        fn create_path(&mut self, _desc: imaging::PathDesc) -> imaging::PathId {
            imaging::PathId(0)
        }

        fn destroy_path(&mut self, _id: imaging::PathId) {}

        fn create_image(&mut self, _desc: imaging::ImageDesc, _pixels: &[u8]) -> imaging::ImageId {
            imaging::ImageId(0)
        }

        fn destroy_image(&mut self, _id: imaging::ImageId) {}

        fn create_paint(&mut self, desc: imaging::PaintDesc) -> imaging::PaintId {
            let id = u32::try_from(self.ops.len())
                .expect("TestBackend: too many paint ops for u32 PaintId");
            let _ = desc;
            imaging::PaintId(id)
        }

        fn destroy_paint(&mut self, _id: imaging::PaintId) {}

        fn create_picture(&mut self, _desc: imaging::PictureDesc) -> imaging::PictureId {
            imaging::PictureId(0)
        }

        fn destroy_picture(&mut self, _id: imaging::PictureId) {}
    }

    impl ImagingBackend for TestBackend {
        fn state(&mut self, op: StateOp) {
            self.ops.push(ImagingOp::State(op));
        }

        fn draw(&mut self, op: DrawOp) {
            self.ops.push(ImagingOp::Draw(op));
        }

        fn begin_record(&mut self) {}

        fn end_record(&mut self) -> imaging::RecordedOps {
            imaging::RecordedOps {
                ops: alloc::sync::Arc::new([]),
                acceleration: None,
                valid_under: imaging::TransformClass::Exact,
                original_ctm: None,
            }
        }
    }

    #[test]
    fn record_into_backend_streams_ops() {
        let mut b = DisplayListBuilder::new(GroupId(1));
        b.push_fill_path(0, rect(0.0, 0.0, 10.0, 10.0), PathId(1), PaintId(2), None);
        let list = b.finish();

        let mut backend = TestBackend::default();
        record_into_backend(&mut backend, &list);
        assert_eq!(backend.ops.len(), 2);
    }

    #[test]
    fn end_to_end_with_ref_backend() {
        let mut b = DisplayListBuilder::new(GroupId(1));
        b.push_fill_path(0, rect(0.0, 0.0, 10.0, 10.0), PathId(1), PaintId(2), None);
        b.push_image(1, rect(5.0, 5.0, 15.0, 15.0), ImageId(3), 1.0, None);
        let list = b.finish();

        let mut backend = RefBackend::default();
        record_into_backend(&mut backend, &list);

        let ops = backend.ops();
        // Expect: SetPaint + FillPath + DrawImage (opacity is 1.0 so no SetOpacity).
        assert_eq!(ops.len(), 3);

        let expected_transform =
            imaging::Affine::translate((5.0, 5.0)) * imaging::Affine::scale_non_uniform(10.0, 10.0);

        let expected = [
            ImagingOp::State(StateOp::SetPaint(ImagingPaintId(2))),
            ImagingOp::Draw(DrawOp::FillPath(ImagingPathId(1))),
            ImagingOp::Draw(DrawOp::DrawImage {
                image: ImagingImageId(3),
                transform: expected_transform,
            }),
        ];

        assert_eq!(ops, &expected);
    }
}
