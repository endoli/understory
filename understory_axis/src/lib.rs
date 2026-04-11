// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Understory Axis: headless axis mapping and tick primitives.
//!
//! This crate focuses on one narrow concern: mapping numeric domains onto a 1D
//! view span and deriving stable, "nice" tick positions for that mapping.
//!
//! It owns:
//! - linear and logarithmic 1D axis mappings
//! - major / medium / minor tick selection
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
//! The intended split is:
//! - a caller supplies a headless axis mapping plus tick policy
//! - this crate returns tick positions plus their semantic kind
//! - the caller formats tick labels appropriate to its own domain
//!
//! ## Minimal example
//!
//! ```rust
//! use understory_axis::{
//!     AxisMajorStepLadder, AxisMapping1D, AxisScale1D, AxisScaleOptions,
//!     AxisSubdivisionPolicy, AxisTickKind,
//! };
//!
//! let mapping = AxisMapping1D::linear(0.0..200.0, 0.0..100.0);
//! let scale = AxisScale1D::from_mapping(
//!     &mapping,
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

/// A headless 1D axis mapping over a visible domain range and view span.
#[derive(Clone, Debug, PartialEq)]
pub enum AxisMapping1D {
    /// Linear mapping with a constant domain-units-per-pixel ratio.
    Linear(AxisLinearMapping1D),
    /// Logarithmic mapping over a positive domain.
    Log(AxisLogMapping1D),
}

impl AxisMapping1D {
    /// Creates a linear mapping for a visible domain and view span.
    #[must_use]
    pub fn linear(view_span: Range<f64>, visible_domain: Range<f64>) -> Self {
        Self::Linear(AxisLinearMapping1D {
            view_span,
            visible_domain,
        })
    }

    /// Creates a logarithmic mapping for a visible positive domain and view span.
    ///
    /// Invalid bases fall back to `10.0`.
    #[must_use]
    pub fn log(view_span: Range<f64>, visible_domain: Range<f64>, base: f64) -> Self {
        Self::Log(AxisLogMapping1D {
            view_span,
            visible_domain,
            base: normalized_log_base(base),
        })
    }

    /// Returns the current view span in device coordinates.
    #[must_use]
    pub fn view_span(&self) -> Range<f64> {
        match self {
            Self::Linear(mapping) => mapping.view_span.clone(),
            Self::Log(mapping) => mapping.view_span.clone(),
        }
    }

    /// Returns the currently visible domain range.
    #[must_use]
    pub fn visible_domain(&self) -> Range<f64> {
        match self {
            Self::Linear(mapping) => mapping.visible_domain.clone(),
            Self::Log(mapping) => mapping.visible_domain.clone(),
        }
    }

    /// Maps a domain value into the view span.
    #[must_use]
    pub fn domain_to_view(&self, value: f64) -> f64 {
        match self {
            Self::Linear(mapping) => mapping.domain_to_view(value),
            Self::Log(mapping) => mapping.domain_to_view(value),
        }
    }

    /// Maps a view coordinate back into the domain.
    #[must_use]
    pub fn view_to_domain(&self, value: f64) -> f64 {
        match self {
            Self::Linear(mapping) => mapping.view_to_domain(value),
            Self::Log(mapping) => mapping.view_to_domain(value),
        }
    }
}

/// Linear 1D axis mapping with a constant domain-units-per-pixel ratio.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisLinearMapping1D {
    view_span: Range<f64>,
    visible_domain: Range<f64>,
}

impl AxisLinearMapping1D {
    fn domain_to_view(&self, value: f64) -> f64 {
        let domain_len = self.visible_domain.end - self.visible_domain.start;
        let view_len = self.view_span.end - self.view_span.start;
        if domain_len == 0.0 {
            return self.view_span.start;
        }
        self.view_span.start + (value - self.visible_domain.start) * view_len / domain_len
    }

    fn view_to_domain(&self, value: f64) -> f64 {
        let view_len = self.view_span.end - self.view_span.start;
        let domain_len = self.visible_domain.end - self.visible_domain.start;
        if view_len == 0.0 {
            return self.visible_domain.start;
        }
        self.visible_domain.start + (value - self.view_span.start) * domain_len / view_len
    }
}

/// Logarithmic 1D axis mapping over a positive domain.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisLogMapping1D {
    view_span: Range<f64>,
    visible_domain: Range<f64>,
    base: f64,
}

impl AxisLogMapping1D {
    fn domain_to_view(&self, value: f64) -> f64 {
        let min = self.visible_domain.start.min(self.visible_domain.end);
        let max = self.visible_domain.start.max(self.visible_domain.end);
        if value <= 0.0 || min <= 0.0 || max <= 0.0 {
            return self.view_span.start;
        }
        let log_min = log_in_base(min, self.base);
        let log_max = log_in_base(max, self.base);
        let view_len = self.view_span.end - self.view_span.start;
        if log_max == log_min {
            return self.view_span.start;
        }
        self.view_span.start
            + (log_in_base(value, self.base) - log_min) * view_len / (log_max - log_min)
    }

    fn view_to_domain(&self, value: f64) -> f64 {
        let min = self.visible_domain.start.min(self.visible_domain.end);
        let max = self.visible_domain.start.max(self.visible_domain.end);
        if min <= 0.0 || max <= 0.0 {
            return min.max(f64::MIN_POSITIVE);
        }
        let log_min = log_in_base(min, self.base);
        let log_max = log_in_base(max, self.base);
        let view_len = self.view_span.end - self.view_span.start;
        if view_len == 0.0 {
            return min;
        }
        let log_value = log_min + (value - self.view_span.start) * (log_max - log_min) / view_len;
        libm::pow(self.base, log_value)
    }
}

/// A derived 1D axis tick guide over a numeric mapping.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AxisScale1D {
    kind: AxisScaleKind,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum AxisScaleKind {
    Linear {
        world_units_per_pixel: f64,
        major_step: f64,
        minor_step: f64,
        subdivisions: usize,
        medium_interval: Option<usize>,
        medium_labels: bool,
    },
    Log {
        base: f64,
        log_units_per_pixel: f64,
        major_log_step: f64,
        subdivisions: usize,
        medium_interval: Option<usize>,
        medium_labels: bool,
        subdivision_mode: LogSubdivisionMode,
    },
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum LogSubdivisionMode {
    None,
    IntegerMultiples { base_int: usize },
    EvenLogIntervals,
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
        Self::linear_from_world_units_per_pixel(world_units_per_pixel, options)
    }

    /// Derive a scale from an explicit axis mapping and tick policy.
    #[must_use]
    pub fn from_mapping(mapping: &AxisMapping1D, options: AxisScaleOptions) -> Self {
        match mapping {
            AxisMapping1D::Linear(mapping) => {
                let view_len = (mapping.view_span.end - mapping.view_span.start).abs();
                let domain_len = (mapping.visible_domain.end - mapping.visible_domain.start).abs();
                let world_units_per_pixel = if view_len > 0.0 {
                    domain_len / view_len
                } else {
                    f64::MIN_POSITIVE
                };
                Self::linear_from_world_units_per_pixel(world_units_per_pixel, options)
            }
            AxisMapping1D::Log(mapping) => Self::log_from_mapping(mapping, options),
        }
    }

    fn linear_from_world_units_per_pixel(
        world_units_per_pixel: f64,
        options: AxisScaleOptions,
    ) -> Self {
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
            kind: AxisScaleKind::Linear {
                world_units_per_pixel,
                major_step,
                minor_step,
                subdivisions,
                medium_interval,
                medium_labels,
            },
        }
    }

    fn log_from_mapping(mapping: &AxisLogMapping1D, options: AxisScaleOptions) -> Self {
        let min = mapping.visible_domain.start.min(mapping.visible_domain.end);
        let max = mapping.visible_domain.start.max(mapping.visible_domain.end);
        let view_len = (mapping.view_span.end - mapping.view_span.start)
            .abs()
            .max(f64::MIN_POSITIVE);
        let log_units_per_pixel = if min > 0.0 && max > 0.0 {
            (log_in_base(max, mapping.base) - log_in_base(min, mapping.base)).abs() / view_len
        } else {
            f64::MIN_POSITIVE
        };
        let target_major_step = log_units_per_pixel * options.target_major_spacing_px;
        let major_log_step = choose_log_major_step(target_major_step.max(1e-12));
        let major_spacing_px = major_log_step / log_units_per_pixel.max(f64::MIN_POSITIVE);
        let medium_labels = major_spacing_px >= options.medium_label_min_spacing_px;
        let (subdivisions, medium_interval, subdivision_mode) =
            derive_log_subdivision_mode(mapping.base, major_log_step, options.subdivision_policy);

        Self {
            kind: AxisScaleKind::Log {
                base: mapping.base,
                log_units_per_pixel,
                major_log_step,
                subdivisions,
                medium_interval,
                medium_labels,
                subdivision_mode,
            },
        }
    }

    /// Returns the domain-units-per-pixel ratio used to derive this axis scale when constant.
    #[must_use]
    pub fn world_units_per_pixel(&self) -> Option<f64> {
        match self.kind {
            AxisScaleKind::Linear {
                world_units_per_pixel,
                ..
            } => Some(world_units_per_pixel),
            AxisScaleKind::Log { .. } => None,
        }
    }

    /// Returns the derived major step in domain units when the axis has a uniform domain step.
    #[must_use]
    pub fn major_step(&self) -> Option<f64> {
        match self.kind {
            AxisScaleKind::Linear { major_step, .. } => Some(major_step),
            AxisScaleKind::Log { .. } => None,
        }
    }

    /// Returns the step in domain units implied by the smallest label-eligible ticks when uniform.
    #[must_use]
    pub fn label_step(&self) -> Option<f64> {
        match self.kind {
            AxisScaleKind::Linear {
                major_step,
                minor_step,
                medium_interval,
                medium_labels,
                ..
            } => {
                if medium_labels {
                    Some(
                        medium_interval
                            .map(|interval| minor_step * interval as f64)
                            .unwrap_or(major_step),
                    )
                } else {
                    Some(major_step)
                }
            }
            AxisScaleKind::Log { .. } => None,
        }
    }

    /// Returns the derived medium step in domain units when the axis has a uniform domain step.
    #[must_use]
    pub fn medium_step(&self) -> Option<f64> {
        match self.kind {
            AxisScaleKind::Linear {
                minor_step,
                medium_interval,
                ..
            } => medium_interval.map(|interval| minor_step * interval as f64),
            AxisScaleKind::Log { .. } => None,
        }
    }

    /// Returns the derived minor step in domain units when the axis has a uniform domain step.
    #[must_use]
    pub fn minor_step(&self) -> Option<f64> {
        match self.kind {
            AxisScaleKind::Linear { minor_step, .. } => Some(minor_step),
            AxisScaleKind::Log { .. } => None,
        }
    }

    /// Returns the spacing in pixels between major ticks when uniform in view space.
    #[must_use]
    pub fn major_spacing_px(&self) -> Option<f64> {
        match self.kind {
            AxisScaleKind::Linear {
                world_units_per_pixel,
                major_step,
                ..
            } => Some(major_step / world_units_per_pixel),
            AxisScaleKind::Log {
                log_units_per_pixel,
                major_log_step,
                ..
            } => Some(major_log_step / log_units_per_pixel),
        }
    }

    /// Returns the spacing in pixels between medium ticks when it is uniform in view space.
    #[must_use]
    pub fn medium_spacing_px(&self) -> Option<f64> {
        match self.kind {
            AxisScaleKind::Linear {
                world_units_per_pixel,
                minor_step,
                medium_interval,
                ..
            } => {
                medium_interval.map(|interval| minor_step * interval as f64 / world_units_per_pixel)
            }
            AxisScaleKind::Log {
                log_units_per_pixel,
                major_log_step,
                medium_interval,
                subdivision_mode: LogSubdivisionMode::EvenLogIntervals,
                ..
            } => medium_interval.map(|interval| {
                let log_step = major_log_step / interval as f64;
                log_step / log_units_per_pixel
            }),
            AxisScaleKind::Log { .. } => None,
        }
    }

    /// Returns the spacing in pixels between minor ticks when it is uniform in view space.
    #[must_use]
    pub fn minor_spacing_px(&self) -> Option<f64> {
        match self.kind {
            AxisScaleKind::Linear {
                world_units_per_pixel,
                minor_step,
                ..
            } => Some(minor_step / world_units_per_pixel),
            AxisScaleKind::Log {
                log_units_per_pixel,
                major_log_step,
                subdivisions,
                subdivision_mode: LogSubdivisionMode::EvenLogIntervals,
                ..
            } => Some((major_log_step / subdivisions as f64) / log_units_per_pixel),
            AxisScaleKind::Log { .. } => None,
        }
    }

    /// Returns whether medium ticks are eligible for labeling under this scale.
    #[must_use]
    pub fn medium_ticks_are_labeled(&self) -> bool {
        match self.kind {
            AxisScaleKind::Linear { medium_labels, .. } => medium_labels,
            AxisScaleKind::Log { medium_labels, .. } => medium_labels,
        }
    }

    /// Iterates ticks covering the provided visible range plus one minor step on each side.
    #[must_use]
    pub fn iter_ticks_in_range(&self, visible: Range<f64>) -> AxisTicksIter {
        AxisTicksIter {
            inner: self.ticks_in_range(visible).into_iter(),
        }
    }

    /// Returns ticks covering the provided visible range plus one minor step on each side.
    #[must_use]
    pub fn ticks_in_range(&self, visible: Range<f64>) -> Vec<AxisTick> {
        match self.kind {
            AxisScaleKind::Linear {
                minor_step,
                subdivisions,
                medium_interval,
                medium_labels,
                ..
            } => build_linear_ticks(
                visible,
                minor_step,
                subdivisions,
                medium_interval,
                medium_labels,
            ),
            AxisScaleKind::Log {
                base,
                major_log_step,
                subdivisions,
                medium_interval,
                medium_labels,
                subdivision_mode,
                ..
            } => build_log_ticks(
                visible,
                base,
                major_log_step,
                subdivisions,
                medium_interval,
                medium_labels,
                subdivision_mode,
            ),
        }
    }
}

/// Iterator over ticks produced by an [`AxisScale1D`] for a visible numeric range.
#[derive(Debug)]
pub struct AxisTicksIter {
    inner: alloc::vec::IntoIter<AxisTick>,
}

impl Iterator for AxisTicksIter {
    type Item = AxisTick;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

fn build_linear_ticks(
    visible: Range<f64>,
    minor_step: f64,
    subdivisions: usize,
    medium_interval: Option<usize>,
    medium_labels: bool,
) -> Vec<AxisTick> {
    let start_index = floor_to_i64(visible.start / minor_step) - 1;
    let end_index = ceil_to_i64(visible.end / minor_step) + 1;
    let mut ticks = Vec::new();

    for index in start_index..=end_index {
        let value = index as f64 * minor_step;
        if value < visible.start - minor_step || value > visible.end + minor_step {
            continue;
        }
        let sub_index = usize::try_from(index.rem_euclid(subdivisions as i64))
            .expect("rem_euclid stays within subdivision count");
        let (kind, labeled) = if sub_index == 0 {
            (AxisTickKind::Major, true)
        } else if medium_interval.is_some_and(|interval| sub_index.is_multiple_of(interval)) {
            (AxisTickKind::Medium, medium_labels)
        } else {
            (AxisTickKind::Minor, false)
        };
        ticks.push(AxisTick {
            value,
            kind,
            labeled,
        });
    }

    ticks
}

fn build_log_ticks(
    visible: Range<f64>,
    base: f64,
    major_log_step: f64,
    subdivisions: usize,
    medium_interval: Option<usize>,
    medium_labels: bool,
    subdivision_mode: LogSubdivisionMode,
) -> Vec<AxisTick> {
    let min = visible.start.min(visible.end).max(f64::MIN_POSITIVE);
    let max = visible.start.max(visible.end);
    if !min.is_finite() || !max.is_finite() || max <= 0.0 {
        return Vec::new();
    }

    let log_min = log_in_base(min, base);
    let log_max = log_in_base(max.max(min), base);
    let start_index = floor_to_i64(log_min / major_log_step) - 1;
    let end_index = ceil_to_i64(log_max / major_log_step) + 1;
    let mut ticks = Vec::new();

    for major_index in start_index..=end_index {
        let major_log = major_index as f64 * major_log_step;
        match subdivision_mode {
            LogSubdivisionMode::None => push_log_tick(
                &mut ticks,
                libm::pow(base, major_log),
                min,
                max,
                AxisTickKind::Major,
                true,
            ),
            LogSubdivisionMode::IntegerMultiples { base_int } => {
                let decade_value = libm::pow(base, major_log);
                let medium_factor = integer_multiple_medium_factor(base_int);
                for factor in 1..base_int {
                    let value = decade_value * factor as f64;
                    let (kind, labeled) = if factor == 1 {
                        (AxisTickKind::Major, true)
                    } else if Some(factor) == medium_factor {
                        (AxisTickKind::Medium, medium_labels)
                    } else {
                        (AxisTickKind::Minor, false)
                    };
                    push_log_tick(&mut ticks, value, min, max, kind, labeled);
                }
            }
            LogSubdivisionMode::EvenLogIntervals => {
                let medium_index = medium_interval.unwrap_or(0);
                let log_minor_step = major_log_step / subdivisions as f64;
                for sub_index in 0..subdivisions {
                    let value = libm::pow(base, major_log + sub_index as f64 * log_minor_step);
                    let (kind, labeled) = if sub_index == 0 {
                        (AxisTickKind::Major, true)
                    } else if medium_interval.is_some_and(|_| sub_index == medium_index) {
                        (AxisTickKind::Medium, medium_labels)
                    } else {
                        (AxisTickKind::Minor, false)
                    };
                    push_log_tick(&mut ticks, value, min, max, kind, labeled);
                }
            }
        }
    }

    ticks
}

fn push_log_tick(
    ticks: &mut Vec<AxisTick>,
    value: f64,
    min: f64,
    max: f64,
    kind: AxisTickKind,
    labeled: bool,
) {
    if value.is_finite() && value >= min && value <= max {
        ticks.push(AxisTick {
            value,
            kind,
            labeled,
        });
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

fn choose_log_major_step(target: f64) -> f64 {
    choose_decimal_125_step(target)
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

fn normalized_log_base(base: f64) -> f64 {
    if base.is_finite() && base > 0.0 && !approx_eq(base, 1.0) {
        base
    } else {
        10.0
    }
}

fn log_in_base(value: f64, base: f64) -> f64 {
    libm::log(value) / libm::log(normalized_log_base(base))
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

fn derive_log_subdivision_mode(
    base: f64,
    major_log_step: f64,
    policy: AxisSubdivisionPolicy,
) -> (usize, Option<usize>, LogSubdivisionMode) {
    match policy {
        AxisSubdivisionPolicy::Fixed(count) => {
            let subdivisions = count.max(1);
            let medium_interval = if subdivisions.is_multiple_of(2) {
                Some(subdivisions / 2)
            } else {
                None
            };
            let mode = if subdivisions > 1 {
                LogSubdivisionMode::EvenLogIntervals
            } else {
                LogSubdivisionMode::None
            };
            (subdivisions, medium_interval, mode)
        }
        AxisSubdivisionPolicy::Auto => {
            if approx_eq(major_log_step, 1.0)
                && let Some(base_int) = small_integer_base(base)
                && base_int > 2
            {
                (
                    base_int - 1,
                    None,
                    LogSubdivisionMode::IntegerMultiples { base_int },
                )
            } else {
                (1, None, LogSubdivisionMode::None)
            }
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

fn small_integer_base(base: f64) -> Option<usize> {
    let rounded = libm::round(base);
    if approx_eq(base, rounded) && (2.0..=16.0).contains(&rounded) {
        #[expect(
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation,
            reason = "bounded to a small positive integer range"
        )]
        {
            Some(rounded as usize)
        }
    } else {
        None
    }
}

fn integer_multiple_medium_factor(base_int: usize) -> Option<usize> {
    if base_int >= 4 && base_int.is_multiple_of(2) {
        Some(base_int / 2)
    } else if base_int >= 5 {
        Some(5.min(base_int - 1))
    } else {
        None
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
        AxisMajorStepLadder, AxisMapping1D, AxisScale1D, AxisScaleOptions, AxisSubdivisionPolicy,
        AxisTickKind,
    };

    #[test]
    fn larger_world_units_produce_larger_major_steps() {
        let coarse = AxisScale1D::new(2.0);
        let fine = AxisScale1D::new(0.2);
        assert!(
            coarse.major_step().expect("linear scale has a major step")
                > fine.major_step().expect("linear scale has a major step")
        );
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
        let major_step = scale.major_step().expect("linear scale has a major step");
        let minor_step = scale.minor_step().expect("linear scale has a minor step");
        let major_spacing = scale
            .major_spacing_px()
            .expect("linear scale has major spacing");
        let minor_spacing = scale
            .minor_spacing_px()
            .expect("linear scale has minor spacing");
        assert!((major_spacing - major_step / 0.5).abs() < 1e-9);
        assert!((minor_spacing - minor_step / 0.5).abs() < 1e-9);
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
        assert_eq!(scale.major_step(), Some(8.0));
        assert_eq!(scale.medium_step(), Some(4.0));
        assert_eq!(scale.minor_step(), Some(2.0));
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
            Some(
                fine.medium_step()
                    .unwrap_or(fine.major_step().expect("linear scale has a major step"))
            )
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
        assert_eq!(scale.major_step(), Some(15_000.0));
        assert_eq!(scale.minor_step(), Some(5_000.0));
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
        assert_eq!(scale.major_step(), Some(8.0));
        assert_eq!(scale.minor_step(), Some(1.0));
        assert_eq!(scale.medium_step(), Some(4.0));
    }

    #[test]
    fn explicit_linear_mapping_matches_linear_convenience() {
        let mapping = AxisMapping1D::linear(20.0..220.0, 50.0..150.0);
        let from_mapping = AxisScale1D::from_mapping(&mapping, AxisScaleOptions::default());
        let from_ratio = AxisScale1D::with_options(0.5, AxisScaleOptions::default());
        assert_eq!(from_mapping.major_step(), from_ratio.major_step());
        assert_eq!(from_mapping.minor_step(), from_ratio.minor_step());
    }

    #[test]
    fn log_mapping_generates_power_ticks_without_uniform_domain_step() {
        let mapping = AxisMapping1D::log(0.0..300.0, 1.0..1_000.0, 10.0);
        let scale = AxisScale1D::from_mapping(&mapping, AxisScaleOptions::default());
        let ticks = scale.ticks_in_range(1.0..1_000.0);
        assert!(
            ticks
                .iter()
                .any(|tick| tick.value == 1.0 && tick.kind == AxisTickKind::Major)
        );
        assert!(
            ticks
                .iter()
                .any(|tick| tick.value == 10.0 && tick.kind == AxisTickKind::Major)
        );
        assert!(
            ticks
                .iter()
                .any(|tick| tick.value == 100.0 && tick.kind == AxisTickKind::Major)
        );
        assert_eq!(scale.major_step(), None);
        assert_eq!(scale.label_step(), None);
    }

    #[test]
    fn log_mapping_round_trips_domain_and_view_values() {
        let mapping = AxisMapping1D::log(10.0..210.0, 1.0..100.0, 10.0);
        let value = 10.0;
        let view = mapping.domain_to_view(value);
        let round_trip = mapping.view_to_domain(view);
        assert!((round_trip - value).abs() < 1e-9);
    }
}
