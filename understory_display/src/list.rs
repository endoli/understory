// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Display list container, builder, and painter trait.

use alloc::vec::Vec;

use kurbo::{Point, Rect};

use crate::{
    BlendMode, ClipId, GroupId, ImageId, Op, OpHeader, OpId, PaintId, PathId, RunId,
    SemanticRegionId, StrokeId,
};

/// A display list: sequence of render operations with stable ids.
#[derive(Clone, Debug, Default)]
pub struct DisplayList {
    /// Sequence of render operations in draw order.
    pub ops: Vec<Op>,
}

impl DisplayList {
    /// Create an empty display list.
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    /// Returns true if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Return a snapshot of resources referenced by this list.
    pub fn resource_snapshot(&self) -> ResourceSnapshot {
        let mut runs = Vec::new();
        let mut images = Vec::new();
        for op in &self.ops {
            match op {
                Op::GlyphRun { run, .. } => runs.push(*run),
                Op::Image { image, .. } => images.push(*image),
                _ => {}
            }
        }
        ResourceSnapshot { runs, images }
    }

    /// Compute the subset of ops whose bounds intersect any of the given dirty rectangles.
    pub fn culled_by_damage(&self, dirty: &[Rect]) -> Vec<OpId> {
        if dirty.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        'ops: for op in &self.ops {
            let bounds = op.header().bounds;
            for r in dirty {
                if bounds.overlaps(*r) {
                    out.push(op.header().id);
                    continue 'ops;
                }
            }
        }
        out
    }
}

/// Builder for a [`DisplayList`].
///
/// This type helps assign stable [`OpId`]s and construct ops with a shared `GroupId`.
#[derive(Clone, Debug)]
pub struct DisplayListBuilder {
    pub(crate) next_id: u32,
    pub(crate) default_group: GroupId,
    pub(crate) ops: Vec<Op>,
}

impl DisplayListBuilder {
    /// Create a new builder with an initial group id.
    pub fn new(default_group: GroupId) -> Self {
        Self {
            next_id: 0,
            default_group,
            ops: Vec::new(),
        }
    }

    /// Finish building and return a [`DisplayList`].
    pub fn finish(self) -> DisplayList {
        DisplayList { ops: self.ops }
    }

    /// Allocate a fresh [`OpId`].
    fn alloc_id(&mut self) -> OpId {
        let id = OpId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    fn header(&mut self, z: i32, bounds: Rect, semantic: Option<SemanticRegionId>) -> OpHeader {
        OpHeader {
            id: self.alloc_id(),
            group: self.default_group,
            z,
            semantic,
            bounds,
        }
    }

    /// Append a filled path operation.
    pub fn push_fill_path(
        &mut self,
        z: i32,
        bounds: Rect,
        path: PathId,
        paint: PaintId,
        semantic: Option<SemanticRegionId>,
    ) {
        let header = self.header(z, bounds, semantic);
        self.ops.push(Op::FillPath {
            header,
            path,
            paint,
        });
    }

    /// Append a stroked path operation.
    pub fn push_stroke_path(
        &mut self,
        z: i32,
        bounds: Rect,
        path: PathId,
        stroke: StrokeId,
        paint: PaintId,
        semantic: Option<SemanticRegionId>,
    ) {
        let header = self.header(z, bounds, semantic);
        self.ops.push(Op::StrokePath {
            header,
            path,
            stroke,
            paint,
        });
    }

    /// Append a glyph run operation.
    pub fn push_glyph_run(
        &mut self,
        z: i32,
        bounds: Rect,
        run: RunId,
        origin: Point,
        paint: PaintId,
        semantic: Option<SemanticRegionId>,
    ) {
        let header = self.header(z, bounds, semantic);
        self.ops.push(Op::GlyphRun {
            header,
            run,
            origin,
            paint,
        });
    }

    /// Append an image draw operation.
    pub fn push_image(
        &mut self,
        z: i32,
        dest: Rect,
        image: ImageId,
        opacity: f32,
        semantic: Option<SemanticRegionId>,
    ) {
        let header = self.header(z, dest, semantic);
        self.ops.push(Op::Image {
            header,
            image,
            dest,
            opacity,
        });
    }

    /// Append a push-clip operation, starting a new clip stack entry.
    pub fn push_push_clip(
        &mut self,
        z: i32,
        bounds: Rect,
        clip: ClipId,
        semantic: Option<SemanticRegionId>,
    ) {
        let header = self.header(z, bounds, semantic);
        self.ops.push(Op::PushClip { header, clip });
    }

    /// Append a pop-clip operation, ending the most recent clip stack entry.
    pub fn push_pop_clip(&mut self, z: i32, bounds: Rect, semantic: Option<SemanticRegionId>) {
        let header = self.header(z, bounds, semantic);
        self.ops.push(Op::PopClip { header });
    }

    /// Append a compositing group operation.
    pub fn push_group(
        &mut self,
        z: i32,
        bounds: Rect,
        opacity: f32,
        blend: BlendMode,
        semantic: Option<SemanticRegionId>,
    ) {
        let header = self.header(z, bounds, semantic);
        self.ops.push(Op::Group {
            header,
            opacity,
            blend,
        });
    }
}

/// Generic painting interface for building display lists.
///
/// This trait abstracts over concrete builders so that higher layers can depend
/// on a small, stable API instead of the internal layout of [`DisplayListBuilder`].
/// For now it is implemented only for [`DisplayListBuilder`], but it can be
/// implemented by other targets in the future (for example, a direct Vello
/// painter for testing or benchmarking).
pub trait DisplayPainter {
    /// Append a filled path operation.
    fn fill_path(
        &mut self,
        z: i32,
        bounds: Rect,
        path: PathId,
        paint: PaintId,
        semantic: Option<SemanticRegionId>,
    );

    /// Append a stroked path operation.
    fn stroke_path(
        &mut self,
        z: i32,
        bounds: Rect,
        path: PathId,
        stroke: StrokeId,
        paint: PaintId,
        semantic: Option<SemanticRegionId>,
    );

    /// Append a glyph run operation.
    fn glyph_run(
        &mut self,
        z: i32,
        bounds: Rect,
        run: RunId,
        origin: Point,
        paint: PaintId,
        semantic: Option<SemanticRegionId>,
    );

    /// Append an image draw operation.
    fn image(
        &mut self,
        z: i32,
        dest: Rect,
        image: ImageId,
        opacity: f32,
        semantic: Option<SemanticRegionId>,
    );

    /// Append a push-clip operation, starting a new clip stack entry.
    fn push_clip(&mut self, z: i32, bounds: Rect, clip: ClipId, semantic: Option<SemanticRegionId>);

    /// Append a pop-clip operation, ending the most recent clip stack entry.
    fn pop_clip(&mut self, z: i32, bounds: Rect, semantic: Option<SemanticRegionId>);

    /// Append a compositing group operation.
    fn group(
        &mut self,
        z: i32,
        bounds: Rect,
        opacity: f32,
        blend: BlendMode,
        semantic: Option<SemanticRegionId>,
    );
}

impl DisplayPainter for DisplayListBuilder {
    fn fill_path(
        &mut self,
        z: i32,
        bounds: Rect,
        path: PathId,
        paint: PaintId,
        semantic: Option<SemanticRegionId>,
    ) {
        self.push_fill_path(z, bounds, path, paint, semantic);
    }

    fn stroke_path(
        &mut self,
        z: i32,
        bounds: Rect,
        path: PathId,
        stroke: StrokeId,
        paint: PaintId,
        semantic: Option<SemanticRegionId>,
    ) {
        self.push_stroke_path(z, bounds, path, stroke, paint, semantic);
    }

    fn glyph_run(
        &mut self,
        z: i32,
        bounds: Rect,
        run: RunId,
        origin: Point,
        paint: PaintId,
        semantic: Option<SemanticRegionId>,
    ) {
        self.push_glyph_run(z, bounds, run, origin, paint, semantic);
    }

    fn image(
        &mut self,
        z: i32,
        dest: Rect,
        image: ImageId,
        opacity: f32,
        semantic: Option<SemanticRegionId>,
    ) {
        self.push_image(z, dest, image, opacity, semantic);
    }

    fn push_clip(
        &mut self,
        z: i32,
        bounds: Rect,
        clip: ClipId,
        semantic: Option<SemanticRegionId>,
    ) {
        self.push_push_clip(z, bounds, clip, semantic);
    }

    fn pop_clip(&mut self, z: i32, bounds: Rect, semantic: Option<SemanticRegionId>) {
        self.push_pop_clip(z, bounds, semantic);
    }

    fn group(
        &mut self,
        z: i32,
        bounds: Rect,
        opacity: f32,
        blend: BlendMode,
        semantic: Option<SemanticRegionId>,
    ) {
        self.push_group(z, bounds, opacity, blend, semantic);
    }
}

/// Snapshot of resources referenced by a display list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResourceSnapshot {
    /// Glyph runs referenced by glyph run ops.
    pub runs: Vec<RunId>,
    /// Images referenced by image ops.
    pub images: Vec<ImageId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect::new(x0, y0, x1, y1)
    }

    #[test]
    fn builder_and_resource_snapshot() {
        let mut b = DisplayListBuilder::new(GroupId(1));
        b.push_glyph_run(
            0,
            rect(0.0, 0.0, 10.0, 10.0),
            RunId(1),
            Point::new(0.0, 0.0),
            PaintId(1),
            Some(SemanticRegionId(7)),
        );
        b.push_image(1, rect(10.0, 10.0, 20.0, 20.0), ImageId(2), 1.0, None);
        let list = b.finish();
        let snapshot = list.resource_snapshot();
        assert_eq!(snapshot.runs, vec![RunId(1)]);
        assert_eq!(snapshot.images, vec![ImageId(2)]);
    }

    #[test]
    fn damage_culling() {
        let mut b = DisplayListBuilder::new(GroupId(1));
        b.push_fill_path(0, rect(0.0, 0.0, 10.0, 10.0), PathId(1), PaintId(1), None);
        b.push_fill_path(0, rect(20.0, 20.0, 30.0, 30.0), PathId(2), PaintId(1), None);
        let list = b.finish();
        let dirty = [rect(5.0, 5.0, 25.0, 25.0)];
        let culled = list.culled_by_damage(&dirty);
        assert_eq!(culled.len(), 2);
        // Both ops intersect the dirty rect.
    }
}
