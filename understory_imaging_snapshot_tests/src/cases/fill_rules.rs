// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::*;

/// Compares `FillRule::NonZero` vs `FillRule::EvenOdd` on the same self-overlapping path.
///
/// The two halves should differ in their interior fill, making fill-rule regressions visually obvious.
pub(super) struct FillRuleNonzeroVsEvenodd;

impl FillRuleNonzeroVsEvenodd {
    const NAME: &'static str = "fill_rule_nonzero_vs_evenodd";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for FillRuleNonzeroVsEvenodd {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 0, 0, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        let outer = star_path_desc(80.0, 70.0, 54.0, 24.0, 5);
        let inner = star_path_desc(80.0, 70.0, 28.0, 12.0, 5);
        let mut commands = Vec::with_capacity(outer.commands.len() + inner.commands.len());
        commands.extend_from_slice(&outer.commands);
        commands.extend_from_slice(&inner.commands);
        let path = backend.create_path(PathDesc {
            commands: commands.into_boxed_slice(),
        });

        let paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(255, 0, 255, 255)),
        });
        backend.state(StateOp::SetPaint(paint));

        backend.state(StateOp::SetTransform(Affine::IDENTITY));
        backend.state(StateOp::SetFillRule(FillRule::NonZero));
        backend.draw(DrawOp::FillPath(path));

        backend.state(StateOp::SetTransform(Affine::translate((160.0, 0.0))));
        backend.state(StateOp::SetFillRule(FillRule::EvenOdd));
        backend.draw(DrawOp::FillPath(path));
    }
}

/// Compares clip behavior for `FillRule::NonZero` vs `FillRule::EvenOdd` using the same path clip.
///
/// This checks that clip fill rules are implemented and routed correctly per backend.
pub(super) struct ClipFillRuleNonzeroVsEvenodd;

impl ClipFillRuleNonzeroVsEvenodd {
    const NAME: &'static str = "clip_fill_rule_nonzero_vs_evenodd";
    const VELLO_GPU_MAX_DIFF_PIXELS: u64 = 0;
}

impl SnapshotCase for ClipFillRuleNonzeroVsEvenodd {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn vello_gpu_max_diff_pixels(&self) -> u64 {
        Self::VELLO_GPU_MAX_DIFF_PIXELS
    }

    fn run(&self, backend: &mut dyn ImagingBackend, width: f32, height: f32) {
        let bg = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 0, 0, 255)),
        });
        backend.state(StateOp::SetPaint(bg));
        backend.draw(DrawOp::FillRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        });

        let outer = star_path_desc(80.0, 70.0, 54.0, 24.0, 5);
        let inner = star_path_desc(80.0, 70.0, 28.0, 12.0, 5);
        let mut commands = Vec::with_capacity(outer.commands.len() + inner.commands.len());
        commands.extend_from_slice(&outer.commands);
        commands.extend_from_slice(&inner.commands);
        let path = backend.create_path(PathDesc {
            commands: commands.into_boxed_slice(),
        });

        let clip_paint = backend.create_paint(PaintDesc {
            brush: Brush::Solid(Color::from_rgba8(0, 255, 255, 255)),
        });
        backend.state(StateOp::SetPaint(clip_paint));

        backend.state(StateOp::SetTransform(Affine::translate((0.0, 40.0))));
        backend.with_clip_path(path, FillRule::NonZero, |backend| {
            backend.draw(DrawOp::FillRect {
                x0: 0.0,
                y0: 0.0,
                x1: width,
                y1: height,
            });
        });

        backend.state(StateOp::SetTransform(Affine::translate((160.0, 40.0))));
        backend.with_clip_path(path, FillRule::EvenOdd, |backend| {
            backend.draw(DrawOp::FillRect {
                x0: 0.0,
                y0: 0.0,
                x1: width,
                y1: height,
            });
        });

        backend.state(StateOp::SetTransform(Affine::IDENTITY));
    }
}
