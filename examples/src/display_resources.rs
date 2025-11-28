// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use kurbo::{BezPath, Rect, Stroke};
use understory_display::{ClipId, ImageId, PathId, StrokeId};
use vello::peniko::ImageBrush;

/// Simple arena for geometry and related resources used by Vello examples.
///
/// Examples can allocate paths, clip shapes, strokes, and images once and
/// then refer to them by `PathId`, `ClipId`, `StrokeId`, or `ImageId` in
/// their display lists and resolver implementations.
#[derive(Default, Debug)]
pub struct DisplayResources {
    paths: Vec<BezPath>,
    clips: Vec<BezPath>,
    strokes: Vec<Stroke>,
    images: Vec<ImageBrush>,
}

impl DisplayResources {
    /// Create an empty resource arena.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store an arbitrary path and return its [`PathId`].
    pub fn add_path(&mut self, path: BezPath) -> PathId {
        let id = PathId(self.paths.len() as u32);
        self.paths.push(path);
        id
    }

    /// Store a rectangular path and return its [`PathId`].
    pub fn add_rect_path(&mut self, rect: Rect) -> PathId {
        let mut p = BezPath::new();
        p.move_to((rect.x0, rect.y0));
        p.line_to((rect.x1, rect.y0));
        p.line_to((rect.x1, rect.y1));
        p.line_to((rect.x0, rect.y1));
        p.close_path();
        self.add_path(p)
    }

    /// Store an arbitrary clip shape and return its [`ClipId`].
    pub fn add_clip_path(&mut self, path: BezPath) -> ClipId {
        let id = ClipId(self.clips.len() as u32);
        self.clips.push(path);
        id
    }

    /// Store a rectangular clip shape and return its [`ClipId`].
    pub fn add_clip_rect(&mut self, rect: Rect) -> ClipId {
        let mut p = BezPath::new();
        p.move_to((rect.x0, rect.y0));
        p.line_to((rect.x1, rect.y0));
        p.line_to((rect.x1, rect.y1));
        p.line_to((rect.x0, rect.y1));
        p.close_path();
        self.add_clip_path(p)
    }

    /// Store a stroke style and return its [`StrokeId`].
    pub fn add_stroke(&mut self, stroke: Stroke) -> StrokeId {
        let id = StrokeId(self.strokes.len() as u32);
        self.strokes.push(stroke);
        id
    }

    /// Store an image brush and return its [`ImageId`].
    pub fn add_image(&mut self, image: ImageBrush) -> ImageId {
        let id = ImageId(self.images.len() as u32);
        self.images.push(image);
        id
    }

    /// Look up a path by id.
    pub fn path(&self, id: PathId) -> Option<&BezPath> {
        self.paths.get(id.0 as usize)
    }

    /// Look up a clip shape by id.
    pub fn clip(&self, id: ClipId) -> Option<&BezPath> {
        self.clips.get(id.0 as usize)
    }

    /// Look up a stroke by id.
    pub fn stroke(&self, id: StrokeId) -> Option<&Stroke> {
        self.strokes.get(id.0 as usize)
    }

    /// Look up an image by id.
    pub fn image(&self, id: ImageId) -> Option<&ImageBrush> {
        self.images.get(id.0 as usize)
    }
}
