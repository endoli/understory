// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Understory Imaging Reference Backend.
//!
//! This crate provides a small, stateful reference implementation of
//! [`understory_imaging::ImagingBackend`] and [`understory_imaging::ResourceBackend`].
//! It is intended primarily for tests, examples, and experiments, not for
//! production rendering.

use std::vec::Vec;

use understory_imaging::{
    ClipShape, DrawOp, ImagingBackend, ImagingOp, PaintDesc, PaintId, PathDesc, PathId,
    PictureDesc, PictureId, RecordedOps, ResourceBackend, StateOp, StrokeStyle, TransformClass,
};

/// Snapshot of the current imaging state inside the backend.
#[derive(Clone, Debug)]
pub struct StateSnapshot {
    /// Current transform.
    pub transform: understory_imaging::Affine,
    /// Current clip shape.
    pub clip: ClipShape,
    /// Current paint, if set.
    pub paint: Option<PaintId>,
    /// Current stroke style, if set.
    pub stroke: Option<StrokeStyle>,
    /// Current blend mode.
    pub blend_mode: understory_imaging::BlendMode,
    /// Current opacity in the range \[0, 1].
    pub opacity: f32,
}

impl Default for StateSnapshot {
    fn default() -> Self {
        Self {
            transform: understory_imaging::Affine::IDENTITY,
            clip: ClipShape::Infinite,
            paint: None,
            stroke: None,
            blend_mode: understory_imaging::BlendMode::default(),
            opacity: 1.0,
        }
    }
}

/// Event recorded by the reference backend.
#[derive(Clone, Debug)]
pub enum Event {
    /// State operation and the resulting state snapshot.
    State {
        /// State operation that was applied.
        op: StateOp,
        /// Snapshot after applying the state operation.
        state: StateSnapshot,
    },
    /// Draw operation and the state snapshot used for drawing.
    Draw {
        /// Draw operation that was applied.
        op: DrawOp,
        /// Snapshot at the time of drawing.
        state: StateSnapshot,
    },
}

/// Simple reference implementation of the imaging backend.
///
/// This backend:
/// - Stores resource descriptors in vectors keyed by their IDs,
/// - Tracks current imaging state,
/// - Records high-level [`Event`]s as state and draw operations are applied,
/// - Supports environment-bound recordings via `begin_record`/`end_record`.
#[derive(Default, Debug)]
pub struct RefBackend {
    paths: Vec<Option<PathDesc>>,
    images: Vec<Option<(understory_imaging::ImageDesc, Vec<u8>)>>,
    paints: Vec<Option<PaintDesc>>,
    pictures: Vec<Option<PictureDesc>>,

    /// Log of events in the order they were applied.
    events: Vec<Event>,
    /// Underlying imaging ops, used to form `RecordedOps`.
    ops: Vec<ImagingOp>,
    /// Start index of the current recording, if any.
    recording_start: Option<usize>,
    /// Current imaging state.
    state: StateSnapshot,
}

impl RefBackend {
    /// Returns a slice of recorded events.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Returns a slice of raw imaging operations.
    pub fn ops(&self) -> &[ImagingOp] {
        &self.ops
    }

    /// Clears all recorded events and ops but keeps resources.
    pub fn clear_events(&mut self) {
        self.events.clear();
        self.ops.clear();
        self.recording_start = None;
    }
}

impl ResourceBackend for RefBackend {
    fn create_path(&mut self, desc: PathDesc) -> PathId {
        let id =
            u32::try_from(self.paths.len()).expect("RefBackend: too many paths for u32 PathId");
        self.paths.push(Some(desc));
        PathId(id)
    }

    fn destroy_path(&mut self, id: PathId) {
        let idx = id.0 as usize;
        if let Some(slot) = self.paths.get_mut(idx) {
            *slot = None;
        }
    }

    fn create_image(
        &mut self,
        desc: understory_imaging::ImageDesc,
        pixels: &[u8],
    ) -> understory_imaging::ImageId {
        let id =
            u32::try_from(self.images.len()).expect("RefBackend: too many images for u32 ImageId");
        self.images.push(Some((desc, pixels.to_vec())));
        understory_imaging::ImageId(id)
    }

    fn destroy_image(&mut self, id: understory_imaging::ImageId) {
        let idx = id.0 as usize;
        if let Some(slot) = self.images.get_mut(idx) {
            *slot = None;
        }
    }

    fn create_paint(&mut self, desc: PaintDesc) -> PaintId {
        let id =
            u32::try_from(self.paints.len()).expect("RefBackend: too many paints for u32 PaintId");
        self.paints.push(Some(desc));
        PaintId(id)
    }

    fn destroy_paint(&mut self, id: PaintId) {
        let idx = id.0 as usize;
        if let Some(slot) = self.paints.get_mut(idx) {
            *slot = None;
        }
    }

    fn create_picture(&mut self, desc: PictureDesc) -> PictureId {
        let id = u32::try_from(self.pictures.len())
            .expect("RefBackend: too many pictures for u32 PictureId");
        self.pictures.push(Some(desc));
        PictureId(id)
    }

    fn destroy_picture(&mut self, id: PictureId) {
        let idx = id.0 as usize;
        if let Some(slot) = self.pictures.get_mut(idx) {
            *slot = None;
        }
    }
}

impl ImagingBackend for RefBackend {
    fn state(&mut self, op: StateOp) {
        match &op {
            StateOp::SetTransform(tx) => self.state.transform = *tx,
            StateOp::SetPaintTransform(_tx) => {
                // Paint transform is currently not captured in the snapshot;
                // backends are expected to interpret this state as needed.
            }
            StateOp::SetClip(clip) => self.state.clip = clip.clone(),
            StateOp::SetPaint(id) => self.state.paint = Some(*id),
            StateOp::SetStroke(style) => self.state.stroke = Some(style.clone()),
            StateOp::SetBlendMode(mode) => self.state.blend_mode = *mode,
            StateOp::SetOpacity(v) => self.state.opacity = *v,
            StateOp::BeginGroup { .. } => {
                // Groups are ignored in the reference backend for now; they
                // are modeled only in the event stream, not in state.
            }
            StateOp::EndGroup => {
                // See comment above; no state changes for group boundaries.
            }
        }

        self.ops.push(ImagingOp::State(op.clone()));
        self.events.push(Event::State {
            op,
            state: self.state.clone(),
        });
    }

    fn draw(&mut self, op: DrawOp) {
        self.ops.push(ImagingOp::Draw(op.clone()));
        self.events.push(Event::Draw {
            op,
            state: self.state.clone(),
        });
    }

    fn begin_record(&mut self) {
        self.recording_start = Some(self.ops.len());
    }

    fn end_record(&mut self) -> RecordedOps {
        let start = self.recording_start.take().unwrap_or(self.ops.len());
        let slice = &self.ops[start..];
        RecordedOps {
            ops: std::sync::Arc::from(slice),
            acceleration: None,
            valid_under: TransformClass::Exact,
            original_ctm: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peniko::{Brush, Color};
    use understory_imaging::{
        Affine, BlendMode, ClipShape, ImageDesc, PaintDesc, PathCmd, StateOp,
    };

    #[test]
    fn basic_state_and_draw() {
        let mut backend = RefBackend::default();

        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::WHITE),
        });
        let path = backend.create_path(PathDesc {
            commands: vec![PathCmd::MoveTo { x: 0.0, y: 0.0 }].into_boxed_slice(),
        });

        backend.state(StateOp::SetPaint(paint));
        backend.draw(DrawOp::FillPath(path));

        assert_eq!(backend.events().len(), 2);
        assert_eq!(backend.ops().len(), 2);
    }

    #[test]
    fn recording_captures_suffix_of_ops() {
        let mut backend = RefBackend::default();

        let img = backend.create_image(
            ImageDesc {
                width: 1,
                height: 1,
            },
            &[0_u8, 0, 0, 0],
        );

        // One op before recording.
        backend.state(StateOp::SetOpacity(0.5));

        backend.begin_record();
        backend.draw(DrawOp::DrawImage {
            image: img,
            transform: Affine::IDENTITY,
        });
        let rec = backend.end_record();

        assert_eq!(backend.ops().len(), 2);
        assert_eq!(rec.ops.len(), 1);
        assert!(matches!(rec.valid_under, TransformClass::Exact));
    }

    #[test]
    fn state_snapshot_updates() {
        let mut backend = RefBackend::default();

        backend.state(StateOp::SetTransform(Affine::scale(2.0)));
        backend.state(StateOp::SetOpacity(0.25));
        backend.state(StateOp::SetBlendMode(BlendMode::default()));

        let last = backend.events().last().expect("at least one event");
        let Event::State { state, .. } = last else {
            panic!("expected final event to be State");
        };

        assert_eq!(state.opacity, 0.25);
        assert_eq!(state.transform, Affine::scale(2.0));
        matches!(state.clip, ClipShape::Infinite);
    }

    #[test]
    fn clear_events_keeps_resources_usable() {
        let mut backend = RefBackend::default();

        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::WHITE),
        });
        let path = backend.create_path(PathDesc {
            commands: vec![PathCmd::MoveTo { x: 0.0, y: 0.0 }].into_boxed_slice(),
        });

        backend.state(StateOp::SetPaint(paint));
        backend.draw(DrawOp::FillPath(path));
        assert_eq!(backend.events().len(), 2);

        backend.clear_events();
        assert!(backend.events().is_empty());
        assert!(backend.ops().is_empty());

        // Using the same paint/path after clearing events should still work.
        backend.state(StateOp::SetPaint(paint));
        backend.draw(DrawOp::FillPath(path));
        assert_eq!(backend.events().len(), 2);
    }

    #[test]
    fn nested_begin_record_uses_latest_start() {
        let mut backend = RefBackend::default();

        let path = backend.create_path(PathDesc {
            commands: vec![PathCmd::MoveTo { x: 0.0, y: 0.0 }].into_boxed_slice(),
        });

        backend.state(StateOp::SetOpacity(1.0));
        backend.begin_record();
        backend.draw(DrawOp::FillPath(path));
        // Start a new recording; this should overwrite the start index.
        backend.begin_record();
        backend.draw(DrawOp::StrokePath(path));
        let rec = backend.end_record();

        // We recorded only the last draw.
        assert_eq!(backend.ops().len(), 3);
        assert_eq!(rec.ops.len(), 1);
    }

    #[test]
    fn empty_recording_is_valid() {
        let mut backend = RefBackend::default();

        backend.begin_record();
        let rec = backend.end_record();

        assert_eq!(rec.ops.len(), 0);
    }

    #[test]
    fn resource_destroy_is_tolerant() {
        let mut backend = RefBackend::default();

        let path = backend.create_path(PathDesc {
            commands: vec![PathCmd::MoveTo { x: 0.0, y: 0.0 }].into_boxed_slice(),
        });
        let img = backend.create_image(
            ImageDesc {
                width: 1,
                height: 1,
            },
            &[0_u8, 0, 0, 0],
        );
        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::WHITE),
        });
        let picture = backend.create_picture(PictureDesc {
            recording: RecordedOps {
                ops: Vec::new().into_boxed_slice().into(),
                acceleration: None,
                valid_under: TransformClass::Exact,
                original_ctm: None,
            },
        });

        backend.destroy_path(path);
        backend.destroy_image(img);
        backend.destroy_paint(paint);
        backend.destroy_picture(picture);

        // Double-destroy should not panic.
        backend.destroy_path(path);
        backend.destroy_image(img);
        backend.destroy_paint(paint);
        backend.destroy_picture(picture);
    }
}
