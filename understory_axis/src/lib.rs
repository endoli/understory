// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Understory Axis: headless numeric axis scale and tick primitives.
//!
//! This crate focuses on one narrow concern: deriving stable, "nice" 1D tick
//! positions from a continuous numeric axis.
//!
//! It owns:
//! - major / medium / minor tick selection
//! - 1-2-5 step sizing
//! - label eligibility decisions based on spacing thresholds
//! - spacing metadata for callers that need consistent axis-derived policy
//! - configurable major-step ladders and subdivision policies for different axis domains
//!
//! It does not own:
//! - domain-specific label formatting
//! - time units or dates
//! - viewport transforms
//! - rendering or text layout
//!
//! It currently models linear numeric axes. A true logarithmic axis needs a
//! different world/view contract and range-dependent tick placement, so it is
//! not represented here as "just another ladder."
//!
//! The intended split is:
//! - a caller supplies world-units-per-pixel and a visible numeric range
//! - this crate returns tick positions plus their semantic kind
//! - the caller formats tick labels appropriate to its own domain
//!
//! ## Minimal example
//!
//! ```rust
//! use understory_axis::{
//!     AxisMajorStepLadder, AxisScale1D, AxisScaleOptions, AxisSubdivisionPolicy,
//!     AxisTickKind,
//! };
//!
//! let scale = AxisScale1D::with_options(
//!     0.5,
//!     AxisScaleOptions {
//!         target_major_spacing_px: 100.0,
//!         min_major_step: 0.0,
//!         medium_label_min_spacing_px: 220.0,
//!         major_step_ladder: AxisMajorStepLadder::Decimal125,
//!         subdivision_policy: AxisSubdivisionPolicy::Auto,
//!     },
//! );
//!
//! let ticks: std::vec::Vec<_> = scale.iter_ticks_in_range(0.0..100.0).collect();
//! assert!(ticks.iter().any(|tick| tick.kind == AxisTickKind::Major && tick.labeled));
//! ```
//!
//! This crate is `no_std` and uses `alloc`.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::ops::Range;

/// Semantic classification for an axis tick.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AxisTickKind {
    /// Primary grid/tick mark.
    Major,
    /// Secondary subdivision that may optionally be labeled.
    Medium,
    /// Fine subdivision without labeling.
    Minor,
}

/// A single axis tick position plus label eligibility.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AxisTick {
    /// Tick coordinate in caller-defined world units.
    pub value: f64,
    /// Semantic tick kind.
    pub kind: AxisTickKind,
    /// Whether a higher layer should consider labeling this tick.
    pub labeled: bool,
}

/// Options controlling automatic 1D axis scale derivation.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AxisScaleOptions {
    /// Desired spacing in pixels between major ticks.
    pub target_major_spacing_px: f64,
    /// Lower bound for the major tick step in world units.
    pub min_major_step: f64,
    /// Minimum major spacing in pixels before medium ticks become label-eligible.
    pub medium_label_min_spacing_px: f64,
    /// Sparse set of canonical major-step anchors.
    ///
    /// This chooses the "nice" major step nearest to the desired spacing. It
    /// does not determine how that major step is subdivided into medium/minor
    /// ticks; that is handled separately by [`AxisSubdivisionPolicy`].
    pub major_step_ladder: AxisMajorStepLadder,
    /// Policy for subdividing the chosen major step into medium/minor ticks.
    ///
    /// This is where values like "3" or "4" usually belong. They tend to make
    /// sense as subdivisions of a major step more often than as globally
    /// canonical major-step anchors.
    pub subdivision_policy: AxisSubdivisionPolicy,
}

impl Default for AxisScaleOptions {
    fn default() -> Self {
        Self {
            target_major_spacing_px: 96.0,
            min_major_step: 0.0,
            medium_label_min_spacing_px: 220.0,
            major_step_ladder: AxisMajorStepLadder::Decimal125,
            subdivision_policy: AxisSubdivisionPolicy::Auto,
        }
    }
}

/// Sparse set of canonical major-step anchors for a linear numeric axis.
///
/// A ladder answers one narrow question: once a caller knows the approximate
/// major spacing it wants, which "nice" major step should that snap to?
///
/// `1-2-5` is the common default because it gives stable, memorable breakpoints
/// across decades: `... 0.1, 0.2, 0.5, 1, 2, 5, 10 ...`.
///
/// Values like `3` and `4` are usually more useful as subdivisions of a chosen
/// major step than as globally canonical major anchors. For example, a major
/// step of `20` often wants four minor `5`s; that does not mean `4` itself
/// should become a top-level major-step rung.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AxisMajorStepLadder {
    /// Decimal `1-2-5` major steps: `... 0.1, 0.2, 0.5, 1, 2, 5, 10 ...`.
    Decimal125,
    /// Binary power-of-two major steps: `... 1, 2, 4, 8, 16 ...`.
    ///
    /// This is useful for sample indices, memory-like domains, and other
    /// quantities that naturally prefer binary breakpoints over decimal ones.
    BinaryPowerOfTwo,
    /// Time-like major steps using decimal sub-second spacing and sexagesimal
    /// larger units.
    ///
    /// `units_per_second` declares how many caller-defined world units make up
    /// one second. For example:
    ///
    /// - `1.0` for world units already expressed in seconds
    /// - `1_000.0` for milliseconds
    /// - `1_000_000.0` for microseconds
    ///
    /// Below one second this falls back to decimal `1-2-5` steps; at and above
    /// one second it prefers time-oriented anchors such as `1s`, `2s`, `5s`,
    /// `10s`, `15s`, `30s`, `1m`, `2m`, `5m`, `10m`, `15m`, `30m`, `1h`, and so on.
    TimeLike {
        /// Number of caller-defined world units that correspond to one second.
        units_per_second: f64,
    },
}

/// Policy for subdividing a chosen major step into medium/minor ticks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AxisSubdivisionPolicy {
    /// Use the ladder's default subdivision behavior.
    Auto,
    /// Divide each major step into a fixed number of equal minor intervals.
    ///
    /// `0` is treated as `1`, which yields no effective subdivision.
    Fixed(usize),
}

/// A derived 1D axis scale over a continuous numeric domain.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AxisScale1D {
    world_units_per_pixel: f64,
    major_step: f64,
    minor_step: f64,
    subdivisions: usize,
    medium_interval: Option<usize>,
    medium_labels: bool,
}

impl AxisScale1D {
    /// Derive a scale from a world-units-per-pixel ratio using default options.
    #[must_use]
    pub fn new(world_units_per_pixel: f64) -> Self {
        Self::with_options(world_units_per_pixel, AxisScaleOptions::default())
    }

    /// Derive a scale from a world-units-per-pixel ratio and explicit options.
    #[must_use]
    pub fn with_options(world_units_per_pixel: f64, options: AxisScaleOptions) -> Self {
        let world_units_per_pixel = world_units_per_pixel.abs().max(f64::MIN_POSITIVE);
        let target_major_step = world_units_per_pixel * options.target_major_spacing_px;
        let major_step = choose_step(
            target_major_step.max(options.min_major_step).max(1e-12),
            options.major_step_ladder,
        );
        let subdivisions = subdivisions_for_step(
            major_step,
            options.major_step_ladder,
            options.subdivision_policy,
        );
        let minor_step = major_step / subdivisions as f64;
        let medium_interval = if subdivisions.is_multiple_of(2) {
            Some(subdivisions / 2)
        } else {
            None
        };
        let major_spacing_px = major_step / world_units_per_pixel;
        let medium_labels = major_spacing_px >= options.medium_label_min_spacing_px;

        Self {
            world_units_per_pixel,
            major_step,
            minor_step,
            subdivisions,
            medium_interval,
            medium_labels,
        }
    }

    /// Returns the world-units-per-pixel ratio used to derive this axis scale.
    #[must_use]
    pub fn world_units_per_pixel(&self) -> f64 {
        self.world_units_per_pixel
    }

    /// Returns the derived major step in world units.
    #[must_use]
    pub fn major_step(&self) -> f64 {
        self.major_step
    }

    /// Returns the step in world units implied by the smallest label-eligible ticks.
    #[must_use]
    pub fn label_step(&self) -> f64 {
        if self.medium_labels {
            self.medium_step().unwrap_or(self.major_step)
        } else {
            self.major_step
        }
    }

    /// Returns the derived medium step in world units when the scale has one.
    #[must_use]
    pub fn medium_step(&self) -> Option<f64> {
        self.medium_interval
            .map(|interval| self.minor_step * interval as f64)
    }

    /// Returns the derived minor step in world units.
    #[must_use]
    pub fn minor_step(&self) -> f64 {
        self.minor_step
    }

    /// Returns the spacing in pixels between major ticks.
    #[must_use]
    pub fn major_spacing_px(&self) -> f64 {
        self.major_step / self.world_units_per_pixel
    }

    /// Returns the spacing in pixels between medium ticks when the scale has one.
    #[must_use]
    pub fn medium_spacing_px(&self) -> Option<f64> {
        self.medium_step()
            .map(|step| step / self.world_units_per_pixel)
    }

    /// Returns the spacing in pixels between minor ticks.
    #[must_use]
    pub fn minor_spacing_px(&self) -> f64 {
        self.minor_step / self.world_units_per_pixel
    }

    /// Returns whether medium ticks are eligible for labeling under this scale.
    #[must_use]
    pub fn medium_ticks_are_labeled(&self) -> bool {
        self.medium_labels
    }

    /// Iterates ticks covering the provided visible range plus one minor step on each side.
    #[must_use]
    pub fn iter_ticks_in_range(&self, visible: Range<f64>) -> AxisTicksIter {
        AxisTicksIter {
            scale: *self,
            visible_start: visible.start,
            visible_end: visible.end,
            next_index: floor_to_i64(visible.start / self.minor_step) - 1,
            end_index: ceil_to_i64(visible.end / self.minor_step) + 1,
        }
    }

    /// Returns ticks covering the provided visible range plus one minor step on each side.
    #[must_use]
    pub fn ticks_in_range(&self, visible: Range<f64>) -> Vec<AxisTick> {
        self.iter_ticks_in_range(visible).collect()
    }
}

/// Iterator over ticks produced by an [`AxisScale1D`] for a visible numeric range.
#[derive(Clone, Debug)]
pub struct AxisTicksIter {
    scale: AxisScale1D,
    visible_start: f64,
    visible_end: f64,
    next_index: i64,
    end_index: i64,
}

impl Iterator for AxisTicksIter {
    type Item = AxisTick;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next_index <= self.end_index {
            let index = self.next_index;
            self.next_index += 1;
            let value = index as f64 * self.scale.minor_step;
            if value < self.visible_start - self.scale.minor_step
                || value > self.visible_end + self.scale.minor_step
            {
                continue;
            }
            let sub_index = usize::try_from(index.rem_euclid(self.scale.subdivisions as i64))
                .expect("rem_euclid stays within subdivision count");
            let (kind, labeled) = if sub_index == 0 {
                (AxisTickKind::Major, true)
            } else if self
                .scale
                .medium_interval
                .is_some_and(|interval| sub_index.is_multiple_of(interval))
            {
                (AxisTickKind::Medium, self.scale.medium_labels)
            } else {
                (AxisTickKind::Minor, false)
            };
            return Some(AxisTick {
                value,
                kind,
                labeled,
            });
        }

        None
    }
}

fn choose_step(target: f64, ladder: AxisMajorStepLadder) -> f64 {
    match ladder {
        AxisMajorStepLadder::Decimal125 => choose_decimal_125_step(target),
        AxisMajorStepLadder::BinaryPowerOfTwo => {
            let mut step = 1.0_f64;
            if target >= 1.0 {
                while step < target {
                    step *= 2.0;
                }
            } else {
                while step * 0.5 >= target {
                    step *= 0.5;
                }
            }
            step
        }
        AxisMajorStepLadder::TimeLike { units_per_second } => {
            choose_time_like_step(target, units_per_second)
        }
    }
}

fn choose_decimal_125_step(target: f64) -> f64 {
    let mut unit = 1.0_f64;
    if target >= 1.0 {
        while unit * 10.0 <= target {
            unit *= 10.0;
        }
    } else {
        while unit > target {
            unit /= 10.0;
        }
    }

    for mantissa in [1.0_f64, 2.0, 5.0, 10.0] {
        let step = mantissa * unit;
        if step >= target {
            return step;
        }
    }

    10.0 * unit
}

fn choose_time_like_step(target: f64, units_per_second: f64) -> f64 {
    const LARGE_TIME_STEPS_SECONDS: &[f64] = &[
        1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1_800.0, 3_600.0,
        7_200.0, 10_800.0, 21_600.0, 43_200.0, 86_400.0, 172_800.0, 604_800.0,
    ];

    let units_per_second = units_per_second.abs().max(f64::MIN_POSITIVE);
    let target_seconds = target / units_per_second;
    if target_seconds < 1.0 {
        return choose_decimal_125_step(target_seconds) * units_per_second;
    }

    for &step_seconds in LARGE_TIME_STEPS_SECONDS {
        if step_seconds >= target_seconds {
            return step_seconds * units_per_second;
        }
    }

    choose_decimal_125_step(target_seconds / 86_400.0) * 86_400.0 * units_per_second
}

fn subdivisions_for_step(
    step: f64,
    ladder: AxisMajorStepLadder,
    policy: AxisSubdivisionPolicy,
) -> usize {
    match policy {
        AxisSubdivisionPolicy::Auto => auto_subdivisions_for_step(step, ladder),
        AxisSubdivisionPolicy::Fixed(count) => count.max(1),
    }
}

fn auto_subdivisions_for_step(step: f64, ladder: AxisMajorStepLadder) -> usize {
    match ladder {
        AxisMajorStepLadder::Decimal125 => decimal_125_subdivisions(step),
        AxisMajorStepLadder::BinaryPowerOfTwo => 4,
        AxisMajorStepLadder::TimeLike { units_per_second } => {
            time_like_subdivisions(step, units_per_second)
        }
    }
}

fn decimal_125_subdivisions(step: f64) -> usize {
    let step = step.abs().max(1e-12);
    let mut scale = 1.0_f64;
    if step >= 1.0 {
        while scale * 10.0 <= step {
            scale *= 10.0;
        }
    } else {
        while scale > step {
            scale /= 10.0;
        }
    }
    let normalized = step / scale;
    if normalized <= 1.0 + 1e-6 {
        10
    } else if normalized <= 2.0 + 1e-6 {
        4
    } else {
        5
    }
}

fn time_like_subdivisions(step: f64, units_per_second: f64) -> usize {
    let units_per_second = units_per_second.abs().max(f64::MIN_POSITIVE);
    let step_seconds = step / units_per_second;
    if step_seconds < 1.0 {
        return decimal_125_subdivisions(step_seconds);
    }

    if approx_eq(step_seconds, 15.0) || approx_eq(step_seconds, 30.0) {
        3
    } else if approx_eq(step_seconds, 60.0)
        || approx_eq(step_seconds, 3_600.0)
        || approx_eq(step_seconds, 21_600.0)
        || approx_eq(step_seconds, 86_400.0)
    {
        6
    } else if approx_eq(step_seconds, 120.0)
        || approx_eq(step_seconds, 7_200.0)
        || approx_eq(step_seconds, 43_200.0)
        || approx_eq(step_seconds, 172_800.0)
    {
        4
    } else if approx_eq(step_seconds, 300.0) || approx_eq(step_seconds, 600.0) {
        5
    } else if approx_eq(step_seconds, 900.0)
        || approx_eq(step_seconds, 1_800.0)
        || approx_eq(step_seconds, 10_800.0)
    {
        3
    } else if approx_eq(step_seconds, 604_800.0) {
        7
    } else {
        decimal_125_subdivisions(step_seconds)
    }
}

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

fn floor_to_i64(value: f64) -> i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "deliberate truncation step for small axis tick indexing"
    )]
    let truncated = value as i64;
    if (truncated as f64) > value {
        truncated - 1
    } else {
        truncated
    }
}

fn ceil_to_i64(value: f64) -> i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "deliberate truncation step for small axis tick indexing"
    )]
    let truncated = value as i64;
    if (truncated as f64) < value {
        truncated + 1
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{
        AxisMajorStepLadder, AxisScale1D, AxisScaleOptions, AxisSubdivisionPolicy, AxisTickKind,
    };

    #[test]
    fn larger_world_units_produce_larger_major_steps() {
        let coarse = AxisScale1D::new(2.0);
        let fine = AxisScale1D::new(0.2);
        assert!(coarse.major_step() > fine.major_step());
    }

    #[test]
    fn medium_ticks_can_be_label_eligible() {
        let scale = AxisScale1D::with_options(
            0.05,
            AxisScaleOptions {
                target_major_spacing_px: 320.0,
                min_major_step: 0.0,
                medium_label_min_spacing_px: 220.0,
                major_step_ladder: AxisMajorStepLadder::Decimal125,
                subdivision_policy: AxisSubdivisionPolicy::Auto,
            },
        );
        let ticks = scale.ticks_in_range(0.0..100.0);
        assert!(
            ticks
                .iter()
                .any(|tick| tick.kind == AxisTickKind::Medium && tick.labeled)
        );
    }

    #[test]
    fn ticks_cover_requested_range() {
        let scale = AxisScale1D::new(0.5);
        let ticks = scale.ticks_in_range(10.0..40.0);
        assert!(!ticks.is_empty());
        assert!(ticks.iter().any(|tick| tick.value <= 10.0));
        assert!(ticks.iter().any(|tick| tick.value >= 40.0));
    }

    #[test]
    fn iterator_matches_vec_helper() {
        let scale = AxisScale1D::new(0.25);
        let via_iter: Vec<_> = scale.iter_ticks_in_range(-15.0..42.0).collect();
        let via_vec = scale.ticks_in_range(-15.0..42.0);
        assert_eq!(via_iter, via_vec);
    }

    #[test]
    fn spacing_metadata_matches_steps() {
        let scale = AxisScale1D::with_options(
            0.5,
            AxisScaleOptions {
                target_major_spacing_px: 96.0,
                min_major_step: 0.0,
                medium_label_min_spacing_px: 220.0,
                major_step_ladder: AxisMajorStepLadder::Decimal125,
                subdivision_policy: AxisSubdivisionPolicy::Auto,
            },
        );
        assert!((scale.major_spacing_px() - scale.major_step() / 0.5).abs() < 1e-9);
        assert!((scale.minor_spacing_px() - scale.minor_step() / 0.5).abs() < 1e-9);
        if let Some(medium_step) = scale.medium_step() {
            let medium_spacing = scale
                .medium_spacing_px()
                .expect("medium step implies medium spacing");
            assert!((medium_spacing - medium_step / 0.5).abs() < 1e-9);
        }
    }

    #[test]
    fn binary_major_step_ladder_prefers_power_of_two_steps() {
        let scale = AxisScale1D::with_options(
            0.75,
            AxisScaleOptions {
                target_major_spacing_px: 8.0,
                min_major_step: 0.0,
                medium_label_min_spacing_px: 220.0,
                major_step_ladder: AxisMajorStepLadder::BinaryPowerOfTwo,
                subdivision_policy: AxisSubdivisionPolicy::Auto,
            },
        );
        assert_eq!(scale.major_step(), 8.0);
        assert_eq!(scale.medium_step(), Some(4.0));
        assert_eq!(scale.minor_step(), 2.0);
    }

    #[test]
    fn label_step_tracks_smallest_label_eligible_ticks() {
        let coarse = AxisScale1D::with_options(
            1.0,
            AxisScaleOptions {
                target_major_spacing_px: 96.0,
                min_major_step: 0.0,
                medium_label_min_spacing_px: 220.0,
                major_step_ladder: AxisMajorStepLadder::Decimal125,
                subdivision_policy: AxisSubdivisionPolicy::Auto,
            },
        );
        assert_eq!(coarse.label_step(), coarse.major_step());

        let fine = AxisScale1D::with_options(
            0.05,
            AxisScaleOptions {
                target_major_spacing_px: 320.0,
                min_major_step: 0.0,
                medium_label_min_spacing_px: 220.0,
                major_step_ladder: AxisMajorStepLadder::Decimal125,
                subdivision_policy: AxisSubdivisionPolicy::Auto,
            },
        );
        assert_eq!(
            fine.label_step(),
            fine.medium_step().unwrap_or(fine.major_step())
        );
    }

    #[test]
    fn time_like_major_step_ladder_prefers_15_and_30_boundaries() {
        let scale = AxisScale1D::with_options(
            125.0,
            AxisScaleOptions {
                target_major_spacing_px: 96.0,
                min_major_step: 0.0,
                medium_label_min_spacing_px: 220.0,
                major_step_ladder: AxisMajorStepLadder::TimeLike {
                    units_per_second: 1_000.0,
                },
                subdivision_policy: AxisSubdivisionPolicy::Auto,
            },
        );
        assert_eq!(scale.major_step(), 15_000.0);
        assert_eq!(scale.minor_step(), 5_000.0);
    }

    #[test]
    fn fixed_subdivision_policy_overrides_ladder_defaults() {
        let scale = AxisScale1D::with_options(
            0.75,
            AxisScaleOptions {
                target_major_spacing_px: 8.0,
                min_major_step: 0.0,
                medium_label_min_spacing_px: 220.0,
                major_step_ladder: AxisMajorStepLadder::BinaryPowerOfTwo,
                subdivision_policy: AxisSubdivisionPolicy::Fixed(8),
            },
        );
        assert_eq!(scale.major_step(), 8.0);
        assert_eq!(scale.minor_step(), 1.0);
        assert_eq!(scale.medium_step(), Some(4.0));
    }
}
