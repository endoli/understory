// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Understory Guide: headless 2D guide geometry primitives.
//!
//! This crate owns small 2D geometric adapters above lower-level numeric axis
//! and selection primitives. It is intended for things like floating rulers,
//! measurement guides, and timeline headers attached to arbitrary baselines.
//!
//! It owns:
//! - line-guide pose and projection math
//! - semantic hit targets for guide body and endpoint handles
//! - lifting [`understory_axis::AxisRuler1D`] marks into 2D geometry
//!
//! It does not own:
//! - rendering
//! - text shaping
//! - event routing
//! - domain navigation policy
//!
//! This crate is `no_std` and uses `alloc`.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::f64::consts::PI;

#[cfg(not(feature = "std"))]
use kurbo::common::FloatFuncs as _;
use kurbo::{Point, Vec2};
use understory_axis::{AxisRuler1D, AxisTickKind};

/// Semantic hit targets for a line guide.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GuideHit {
    /// The guide baseline.
    Body,
    /// The guide's start endpoint handle.
    StartHandle,
    /// The guide's end endpoint handle.
    EndHandle,
}

/// A headless 2D line guide pose.
///
/// The guide stores only a center point, direction, and length. Rendering,
/// styling, and interaction state all remain the responsibility of a higher
/// layer.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LineGuide2D {
    center: Point,
    angle_rad: f64,
    length: f64,
}

impl LineGuide2D {
    /// Creates a new guide pose.
    ///
    /// Negative lengths are treated as zero.
    #[must_use]
    pub fn new(center: Point, angle_rad: f64, length: f64) -> Self {
        Self {
            center,
            angle_rad,
            length: length.max(0.0),
        }
    }

    /// Creates a guide from its endpoints.
    ///
    /// Returns `None` when the endpoints coincide.
    #[must_use]
    pub fn from_endpoints(start: Point, end: Point) -> Option<Self> {
        let delta = end - start;
        let length = delta.hypot();
        if length <= 0.0 {
            return None;
        }
        Some(Self::new(start.lerp(end, 0.5), delta.atan2(), length))
    }

    /// Returns the guide center point.
    #[must_use]
    pub fn center(self) -> Point {
        self.center
    }

    /// Returns the guide angle in radians.
    #[must_use]
    pub fn angle_rad(self) -> f64 {
        self.angle_rad
    }

    /// Returns the guide length in view units.
    #[must_use]
    pub fn length(self) -> f64 {
        self.length
    }

    /// Returns the guide tangent unit vector.
    #[must_use]
    pub fn tangent(self) -> Vec2 {
        Vec2::new(self.angle_rad.cos(), self.angle_rad.sin())
    }

    /// Returns the guide left-hand normal unit vector.
    #[must_use]
    pub fn normal(self) -> Vec2 {
        let tangent = self.tangent();
        Vec2::new(-tangent.y, tangent.x)
    }

    /// Returns the guide start point.
    #[must_use]
    pub fn start(self) -> Point {
        self.center - self.tangent() * (self.length * 0.5)
    }

    /// Returns the guide end point.
    #[must_use]
    pub fn end(self) -> Point {
        self.center + self.tangent() * (self.length * 0.5)
    }

    /// Returns the point at a scalar view position along the guide.
    ///
    /// `view_position = 0` corresponds to [`Self::start`] and
    /// `view_position = self.length()` corresponds to [`Self::end`].
    #[must_use]
    pub fn point_at_view_position(self, view_position: f64) -> Point {
        self.start() + self.tangent() * view_position
    }

    /// Projects a point onto the guide tangent and returns its scalar position.
    ///
    /// The returned value is unclamped and may lie outside the guide length.
    #[must_use]
    pub fn project_view_position(self, point: Point) -> f64 {
        let delta = point - self.start();
        delta.dot(self.tangent())
    }

    /// Returns the signed distance from the point to the guide baseline.
    #[must_use]
    pub fn signed_distance_to_baseline(self, point: Point) -> f64 {
        let delta = point - self.start();
        delta.dot(self.normal())
    }

    /// Returns the nearest point on the finite guide baseline.
    #[must_use]
    pub fn nearest_point_on_baseline(self, point: Point) -> Point {
        let scalar = self.project_view_position(point).clamp(0.0, self.length);
        self.point_at_view_position(scalar)
    }

    /// Returns an angle suitable for label placement without upside-down text.
    #[must_use]
    pub fn upright_label_angle_rad(self) -> f64 {
        let tangent = self.tangent();
        if tangent.x < 0.0 {
            self.angle_rad + PI
        } else {
            self.angle_rad
        }
    }

    /// Returns the normal direction associated with [`Self::upright_label_angle_rad`].
    #[must_use]
    pub fn upright_label_normal(self) -> Vec2 {
        let tangent = self.tangent();
        if tangent.x < 0.0 {
            -self.normal()
        } else {
            self.normal()
        }
    }

    /// Hit-tests the guide body and endpoint handles.
    ///
    /// `baseline_tolerance` and `handle_tolerance` are interpreted in view units.
    #[must_use]
    pub fn hit_test(
        self,
        point: Point,
        baseline_tolerance: f64,
        handle_tolerance: f64,
    ) -> Option<GuideHit> {
        if point.distance(self.start()) <= handle_tolerance {
            return Some(GuideHit::StartHandle);
        }
        if point.distance(self.end()) <= handle_tolerance {
            return Some(GuideHit::EndHandle);
        }

        let scalar = self.project_view_position(point);
        ((0.0..=self.length).contains(&scalar)
            && self.signed_distance_to_baseline(point).abs() <= baseline_tolerance)
            .then_some(GuideHit::Body)
    }
}

/// Options for lifting an axis ruler onto a 2D guide.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AxisGuideOptions {
    /// Extra distance between a tick tip and its label anchor.
    pub label_offset: f64,
}

impl Default for AxisGuideOptions {
    fn default() -> Self {
        Self { label_offset: 14.0 }
    }
}

/// A single axis ruler mark lifted into 2D.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AxisGuideMark2D {
    /// Tick coordinate in caller-defined domain units.
    pub value: f64,
    /// Semantic tick kind.
    pub kind: AxisTickKind,
    /// Whether a higher layer should consider labeling this mark.
    pub labeled: bool,
    /// Point where the mark meets the guide baseline.
    pub baseline_point: Point,
    /// Mark tip point.
    pub tip_point: Point,
    /// Suggested label anchor point.
    pub label_anchor: Point,
}

/// A 2D axis guide derived from a scalar ruler snapshot and a line pose.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisGuide2D {
    line: LineGuide2D,
    label_angle_rad: f64,
    marks: Vec<AxisGuideMark2D>,
}

impl AxisGuide2D {
    /// Builds a 2D guide from a scalar ruler snapshot.
    #[must_use]
    pub fn from_ruler(ruler: &AxisRuler1D, line: LineGuide2D, options: AxisGuideOptions) -> Self {
        let mark_normal = line.normal();
        let label_normal = line.upright_label_normal();
        let label_angle_rad = line.upright_label_angle_rad();
        let marks = ruler
            .marks()
            .iter()
            .map(|mark| {
                let baseline_point = line.point_at_view_position(mark.view_position);
                let tip_point = baseline_point + mark_normal * mark.mark_extent;
                let label_anchor =
                    baseline_point + label_normal * (mark.mark_extent + options.label_offset);
                AxisGuideMark2D {
                    value: mark.value,
                    kind: mark.kind,
                    labeled: mark.labeled,
                    baseline_point,
                    tip_point,
                    label_anchor,
                }
            })
            .collect();
        Self {
            line,
            label_angle_rad,
            marks,
        }
    }

    /// Returns the underlying line guide pose.
    #[must_use]
    pub fn line(&self) -> LineGuide2D {
        self.line
    }

    /// Returns the angle that label text should use to remain upright.
    #[must_use]
    pub fn label_angle_rad(&self) -> f64 {
        self.label_angle_rad
    }

    /// Returns the lifted 2D marks in ruler order.
    #[must_use]
    pub fn marks(&self) -> &[AxisGuideMark2D] {
        &self.marks
    }
}

#[cfg(test)]
mod tests {
    use super::{AxisGuide2D, AxisGuideOptions, GuideHit, LineGuide2D};
    use kurbo::Point;
    use understory_axis::{
        AxisMajorStepLadder, AxisMapping1D, AxisRuler1D, AxisRulerOptions, AxisScale1D,
        AxisScaleOptions, AxisSubdivisionPolicy,
    };

    #[test]
    fn line_guide_projects_and_hits_points() {
        let guide = LineGuide2D::new(Point::new(50.0, 40.0), 0.0, 100.0);
        assert_eq!(guide.start(), Point::new(0.0, 40.0));
        assert_eq!(guide.end(), Point::new(100.0, 40.0));
        assert_eq!(
            guide.hit_test(Point::new(0.0, 40.0), 6.0, 8.0),
            Some(GuideHit::StartHandle)
        );
        assert_eq!(
            guide.hit_test(Point::new(100.0, 40.0), 6.0, 8.0),
            Some(GuideHit::EndHandle)
        );
        assert_eq!(
            guide.hit_test(Point::new(40.0, 44.0), 6.0, 8.0),
            Some(GuideHit::Body)
        );
        assert_eq!(guide.project_view_position(Point::new(25.0, 50.0)), 25.0);
        assert_eq!(
            guide.signed_distance_to_baseline(Point::new(25.0, 50.0)),
            10.0
        );
    }

    #[test]
    fn axis_guide_lifts_ruler_marks_into_2d() {
        let mapping = AxisMapping1D::linear(0.0..200.0, 0.0..100.0);
        let scale = AxisScale1D::from_mapping(
            &mapping,
            AxisScaleOptions {
                target_major_spacing_px: 100.0,
                min_major_step: 0.0,
                medium_label_min_spacing_px: 220.0,
                major_step_ladder: AxisMajorStepLadder::Decimal125,
                subdivision_policy: AxisSubdivisionPolicy::Auto,
            },
        );
        let ruler = AxisRuler1D::from_mapping(&mapping, &scale, AxisRulerOptions::default());
        let line = LineGuide2D::new(Point::new(100.0, 100.0), 0.0, 200.0);
        let guide = AxisGuide2D::from_ruler(&ruler, line, AxisGuideOptions::default());

        let first_major = guide
            .marks()
            .iter()
            .find(|mark| matches!(mark.kind, understory_axis::AxisTickKind::Major))
            .expect("expected a major tick");
        assert!((first_major.baseline_point.y - 100.0).abs() < 1e-6);
        assert!(first_major.tip_point.y > first_major.baseline_point.y);
        assert!(first_major.label_anchor.y > first_major.tip_point.y);
    }
}
