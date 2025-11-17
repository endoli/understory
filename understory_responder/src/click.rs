// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Generic click state helper: press-release tracking with path filtering and distance/time constraints.
//!
//! This module provides a generic [`ClickState<K>`] that tracks clicks on paths (`Vec<K>`) and applies
//! sophisticated filtering logic based on path intersection and threshold constraints.
//!
//! ## Path-Based Usage
//!
//! 1) On pointer down, call [`ClickState::on_down`] with the path, position, and timing.
//! 2) Optionally, during pointer movement, call [`ClickState::on_move`] to filter out nodes that exceed movement thresholds.
//! 3) On pointer up, call [`ClickState::on_up`] with current path and position to get filtered results.
//! 4) Use [`ClickState::is_clicking`] with a predicate closure to check if any currently pressed path matches specific criteria.
//!
//! Click recognition follows path filtering logic:
//! - If a node is in both the press path AND current path: include it (ignore thresholds)
//! - If a node is only in the press path: include it only if distance/time thresholds are met
//! - Returns the filtered path as `ClickResult::Click(filtered_path)` or `ClickResult::None(original_path)`
//!
//! ## Basic example (path-based)
//!
//! ```
//! use understory_responder::click::{ClickState, ClickResult};
//!
//! let mut state: ClickState<u32> = ClickState::new();
//! let press_path = vec![1, 42];
//!
//! // Press down at (10, 20) on the path (using default pointer ID)
//! state.on_down(None, press_path.clone(), kurbo::Point::new(10.0, 20.0), Some(0), 1000);
//!
//! // Release at nearby position with no current path - should generate a click
//! let result = state.on_up(None, None, kurbo::Point::new(11.0, 21.0), Some(0), 1050, |_| None);
//! match result {
//!     ClickResult::Click(click_path) => {
//!         assert_eq!(click_path, press_path);
//!     }
//!     ClickResult::None(_) => panic!("Expected click"),
//! }
//! ```
//!
//! ## Path filtering example
//!
//! ```
//! use understory_responder::click::{ClickState, ClickResult, Threshold};
//!
//! // Distance-only filtering (time ignored)
//! let mut state: ClickState<u32> = ClickState::distance_only(5.0);
//! let press_path = vec![1, 2, 3];
//!
//! // Press down on path [1, 2, 3]
//! state.on_down(None, press_path, kurbo::Point::new(0.0, 0.0), Some(0), 1000);
//!
//! // Release far away (exceeds distance) with current path [2, 4]
//! let current_path = vec![2, 4];
//! let result = state.on_up(None, Some(&current_path), kurbo::Point::new(10.0, 0.0), Some(0), 1050, |_| None);
//!
//! match result {
//!     ClickResult::Click(filtered_path) => {
//!         // Only node 2 is included (shared between press and current paths)
//!         // Nodes 1 and 3 are filtered out (not in current path, distance exceeded)
//!         assert_eq!(filtered_path, vec![2]);
//!     }
//!     ClickResult::None(_) => panic!("Expected filtered path"),
//! }
//! ```
//!
//! ## Movement filtering example
//!
//! ```
//! use understory_responder::click::{ClickState, ClickResult, PxPct};
//!
//! // Set up click state with movement filtering
//! let mut state: ClickState<u32> = ClickState::new()
//!     .with_outside_distance_limit(PxPct::Px(20.0)); // Filter nodes if pointer moves >20px outside their bounds
//! let press_path = vec![1, 2, 3];
//!
//! // Press down on path [1, 2, 3]
//! state.on_down(None, press_path, kurbo::Point::new(0.0, 0.0), Some(0), 1000);
//!
//! // Move pointer - this will filter nodes based on their bounds and current path
//! let current_path = vec![2]; // Only node 2 is under the current pointer position
//! let filtered_count = state.on_move(
//!     None,
//!     Some(&current_path),
//!     kurbo::Point::new(100.0, 0.0), // Far from original press position
//!     1050,
//!     |&node| match node {
//!         1 => Some(kurbo::Rect::new(0.0, 0.0, 10.0, 10.0)),   // Far from current position
//!         2 => Some(kurbo::Rect::new(95.0, 0.0, 105.0, 10.0)), // Near current position  
//!         3 => Some(kurbo::Rect::new(200.0, 0.0, 210.0, 10.0)), // Far from current position
//!         _ => None,
//!     }
//! );
//!
//! // Nodes 1 and 3 are filtered out (not in current path and far from pointer)
//! // Node 2 is kept (in current path)
//! assert_eq!(filtered_count, 2);
//!
//! // Later pointer up will only consider remaining nodes
//! let result = state.on_up(None, Some(&current_path), kurbo::Point::new(100.0, 0.0), Some(0), 1100, |_| None);
//! // Only node 2 will be in the click result since 1 and 3 were filtered out during movement
//! ```
//!
//! ## Router integration example (path-based)
//!
//! ```no_run
//! use understory_responder::click::{ClickState, ClickResult};
//! use understory_responder::router::Router;
//! use understory_responder::types::{ResolvedHit, DepthKey, Localizer};
//! # use understory_responder::types::{ParentLookup, WidgetLookup};
//! #
//! # // Minimal types for demonstration
//! # #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
//! # struct Node(u32);
//! #
//! # struct Lookup;
//! # impl WidgetLookup<Node> for Lookup {
//! #     type WidgetId = u32;
//! #     fn widget_of(&self, n: &Node) -> Option<u32> { Some(n.0) }
//! # }
//! #
//! # struct Parents;
//! # impl ParentLookup<Node> for Parents {
//! #     fn parent_of(&self, _n: &Node) -> Option<Node> { None }
//! # }
//! #
//! # let router: Router<Node, Lookup, Parents> = Router::with_parent(Lookup, Parents);
//! # let press_hits = vec![ResolvedHit {
//! #     node: Node(1),
//! #     path: None,
//! #     depth_key: DepthKey::Z(10),
//! #     localizer: Localizer::default(),
//! #     meta: (),
//! # }];
//! # let current_hits = vec![ResolvedHit {
//! #     node: Node(2),
//! #     path: None,
//! #     depth_key: DepthKey::Z(10),
//! #     localizer: Localizer::default(),
//! #     meta: (),
//! # }];
//! #
//! let mut click_state: ClickState<Node> = ClickState::new();
//!
//! // On pointer down: get path from spatial query and record the press
//! let press_path: Vec<Node> = press_hits.iter().map(|hit| hit.node).collect();
//! click_state.on_down(None, press_path, kurbo::Point::new(10.0, 20.0), Some(0), 1000);
//!
//! // On pointer up: check for click with current path before routing
//! let current_path: Vec<Node> = current_hits.iter().map(|hit| hit.node).collect();
//! if let ClickResult::Click(filtered_path) = click_state.on_up(None, Some(&current_path), kurbo::Point::new(11.0, 21.0), Some(0), 1050, |_| None) {
//!     // Dispatch click events for each node in the filtered path
//!     for node in filtered_path {
//!         let click_seq = router.dispatch_for::<()>(node);
//!         // ... handle click event for this node
//!     }
//! }
//! ```
//!
//! ## Configuration
//!
//! Create with custom thresholds using the `Threshold` enum:
//! ```
//! use understory_responder::click::{ClickState, Threshold, PxPct};
//!
//! // Direct constructor with explicit threshold configurations
//! let state1: ClickState<u32> = ClickState::with_thresholds(
//!     Threshold::Limit(8.0),       // Distance limit for click recognition
//!     Threshold::Limit(2000),      // Time limit in ms for click recognition
//!     Threshold::Limit(PxPct::Px(15.0))  // Outside distance limit for node bound filtering
//! );
//!
//! // Common patterns via convenience constructors
//! let distance_only: ClickState<u32> = ClickState::distance_only(5.0);           // Distance only, ignore time and bounds
//! let time_only: ClickState<u32> = ClickState::time_only(1000);                 // Time only, ignore distance and bounds
//! let movement_only: ClickState<u32> = ClickState::movement_only(PxPct::Px(20.0)); // Node bound filtering only
//! let distance_and_movement: ClickState<u32> = ClickState::with_distance_and_movement(5.0, PxPct::Px(20.0)); // Distance + bounds
//! let unlimited: ClickState<u32> = ClickState::unlimited();                     // No constraints
//! let strict: ClickState<u32> = ClickState::strict();                           // Path intersection only
//!
//! // Builder pattern
//! let state2: ClickState<u32> = ClickState::new()
//!     .with_distance_limit(8.0)
//!     .with_time_limit(2000);
//! ```
//!
//! ## Threshold Configurations
//!
//! - **`Threshold::Limit(value)`**: Specific threshold that can be met or exceeded
//! - **`Threshold::Unlimited`**: Always considered met - no constraint applied  
//! - **`Threshold::Ignore`**: Don't consider this threshold - defer to the other one
//!
//! ## Behavior Combinations
//!
//! Thresholds are applied in order: `(distance, time, outside_distance)`
//!
//! - **`Limit` + `Limit` + `*`**: Traditional either/or threshold behavior for click recognition
//! - **`Limit` + `Ignore` + `*`**: Only distance matters for clicks, time irrelevant
//! - **`Ignore` + `Limit` + `*`**: Only time matters for clicks, distance irrelevant  
//! - **`*` + `*` + `Limit`**: Movement filtering active regardless of click thresholds
//! - **`Unlimited` + `Unlimited` + `Unlimited`**: Most permissive (all nodes included)
//! - **`Ignore` + `Ignore` + `Ignore`**: Most conservative (only shared path nodes included)

use core::num::NonZeroU64;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use kurbo::Point;

use kurbo::Rect;

/// Configuration for a threshold constraint.
#[derive(Clone, Debug, PartialEq)]
pub enum Threshold<T> {
    /// Always passes - no constraint applied
    Unlimited,
    /// Don't consider this threshold - defer to the other one
    Ignore,
    /// Specific threshold value to check against
    Limit(T),
}

/// Distance measurement unit for click thresholds.
#[derive(Clone, Debug, PartialEq)]
pub enum PxPct {
    /// Distance in pixels (world coordinates)
    Px(f64),
    /// Distance as percentage
    ///
    /// The percentage value MUST be in the range 0.0 to 1.0.
    /// Values outside this range will be clamped.
    Pct(f32),
}

/// Default distance threshold for click recognition (in world coordinates).
/// This represents approximately 5 physical pixels in most UI contexts.
const DEFAULT_DISTANCE_THRESHOLD: f64 = 5.0;

/// Pointer identifier type for tracking multiple concurrent presses.
pub type PointerId = NonZeroU64;

/// Mouse button identifier for tracking which button was pressed.
pub type Button = u8;

/// A generic click state machine that tracks press-release pairs.
///
/// Manages active presses per pointer and applies distance/time constraints
/// to determine when a pointer up should be considered a "click" and on which target.
///
/// ## Click Recognition Logic
///
/// When `on_up` is called, the system applies threshold-based logic:
/// 1. Button must match the original press
/// 2. If either time OR distance threshold contributes and is met → include all press nodes
/// 3. If both thresholds are ignored → only shared path nodes included
/// 4. Otherwise → filter based on path intersection + threshold results
///
/// Key behaviors:
/// - Tracks press position in world coordinates for stable distance measurement
/// - Configurable distance and time thresholds with Unlimited/Ignore/Limit options
/// - Handles multiple concurrent pointers independently
/// - Generic over path element type K
#[derive(Clone, Debug)]
pub struct ClickState<K> {
    /// Active presses per pointer ID
    presses: BTreeMap<PointerId, Press<K>>,
    /// Distance threshold configuration
    distance_threshold: Threshold<f64>,
    /// Time threshold configuration
    time_threshold: Threshold<u64>,
    /// Distance threshold for filtering individual nodes during pointer movement.
    /// When the pointer moves outside a node's bounds by more than this distance,
    /// that node is filtered out of the potential click path (but other nodes remain)
    outside_distance_threshold: Threshold<PxPct>,
    /// the last pointer down
    last_press: Option<Press<K>>,
}

/// State for an active press
#[derive(Clone, Debug)]
pub struct Press<K> {
    /// The path that was pressed
    pub path: Vec<K>,
    /// World position where the press occurred
    pub world_down_pos: Point,
    /// Timestamp when the press occurred
    pub when_ms: u64,
    /// Which button was pressed
    pub button: Button,
    /// Nodes that have been filtered out during movement
    pub filtered_nodes: Vec<K>,
}

/// Result of processing a pointer up event
#[derive(Clone, Debug)]
pub enum ClickResult<K> {
    /// A click was recognized for the given path
    Click(Vec<K>),
    /// No click recognized, but here's the original press path
    None(Vec<K>),
}

impl PxPct {
    /// Calculate the actual distance threshold in pixels for a given size.
    ///
    /// For Pct variants, the percentage is clamped to 0.0-1.0 range.
    fn to_pixels(&self, size: f64) -> f64 {
        match self {
            Self::Px(pixels) => *pixels,
            Self::Pct(pct) => {
                let clamped_pct = pct.clamp(0.0, 1.0) as f64;
                size * clamped_pct
            }
        }
    }
}

fn distance_outside_rect(point: Point, rect: Rect) -> f64 {
    let clamped = Point::new(
        point.x.clamp(rect.x0, rect.x1),
        point.y.clamp(rect.y0, rect.y1),
    );

    (point - clamped).length()
}

/// Helper function to check if a node should be filtered based on outside distance threshold.
///
/// This is shared logic between `on_move` and `on_up` methods.
///
/// # Arguments
/// * `world_pos` - Current pointer position in world coordinates
/// * `outside_distance_threshold` - The threshold configuration to check against
/// * `rect_lookup` - Function to get world bounds for a node
/// * `node` - The node to check
/// * `for_filtering` - If true, return true when node should be filtered out (`on_move` logic).
///   If false, return true when node should be included (`on_up` logic).
///
/// # Returns
/// Boolean indicating whether the node meets the outside distance criteria
fn check_outside_distance<K, F>(
    world_pos: Point,
    outside_distance_threshold: &Threshold<PxPct>,
    rect_lookup: &F,
    node: &K,
    for_filtering: bool,
) -> bool
where
    F: Fn(&K) -> Option<Rect>,
{
    let Some(bounds) = rect_lookup(node) else {
        // Without bounds, defer to threshold configuration
        return match outside_distance_threshold {
            Threshold::Unlimited => !for_filtering, // Include if not filtering, exclude if filtering
            Threshold::Ignore => !for_filtering, // Include if not filtering, exclude if filtering
            Threshold::Limit(_) => !for_filtering, // Include if not filtering, exclude if filtering
        };
    };

    let outside_distance = distance_outside_rect(world_pos, bounds);

    match outside_distance_threshold {
        Threshold::Unlimited => !for_filtering, // Include if not filtering, exclude if filtering
        Threshold::Ignore => !for_filtering,    // Include if not filtering, exclude if filtering
        Threshold::Limit(threshold) => {
            let size = bounds.width().max(bounds.height());
            let threshold_px = threshold.to_pixels(size);
            if for_filtering {
                outside_distance > threshold_px // Filter out if distance exceeded
            } else {
                outside_distance <= threshold_px // Include if distance within threshold
            }
        }
    }
}

impl<K: Clone> ClickState<K> {
    /// Create a new click state with default thresholds.
    ///
    /// Default: distance limit of 5.0 world units, time ignored
    /// Movement thresholds default to same as click thresholds
    pub fn new() -> Self {
        Self::with_thresholds(
            Threshold::Limit(DEFAULT_DISTANCE_THRESHOLD),
            Threshold::Ignore,
            Threshold::Ignore,
        )
    }

    /// Create a new click state with custom threshold configurations.
    ///
    /// # Arguments
    /// * `distance_threshold` - Distance threshold configuration for click recognition
    /// * `time_threshold` - Time threshold configuration for click recognition
    /// * `outside_distance_threshold` - Distance threshold for filtering nodes based on their bounds
    pub fn with_thresholds(
        distance_threshold: Threshold<f64>,
        time_threshold: Threshold<u64>,
        outside_distance_threshold: Threshold<PxPct>,
    ) -> Self {
        Self {
            presses: BTreeMap::new(),
            distance_threshold,
            time_threshold,
            outside_distance_threshold,
            last_press: None,
        }
    }

    /// Create a click state with only distance constraints (time ignored).
    pub fn distance_only(distance_limit: f64) -> Self {
        Self::with_thresholds(
            Threshold::Limit(distance_limit),
            Threshold::Ignore,
            Threshold::Ignore,
        )
    }

    /// Create a click state with only time constraints (distance ignored).
    pub fn time_only(time_limit_ms: u64) -> Self {
        Self::with_thresholds(
            Threshold::Ignore,
            Threshold::Limit(time_limit_ms),
            Threshold::Ignore,
        )
    }

    /// Create a click state with no constraints (all press nodes included).
    pub fn unlimited() -> Self {
        Self::with_thresholds(
            Threshold::Unlimited,
            Threshold::Unlimited,
            Threshold::Unlimited,
        )
    }

    /// Create a click state with strict path intersection only (all thresholds ignored).
    pub fn strict() -> Self {
        Self::with_thresholds(Threshold::Ignore, Threshold::Ignore, Threshold::Ignore)
    }

    /// Create a click state with distance and outside distance constraints (time ignored).
    ///
    /// # Arguments
    /// * `distance_limit` - Distance threshold for click recognition
    /// * `outside_distance_limit` - Distance threshold for node bound filtering
    pub fn with_distance_and_movement(distance_limit: f64, outside_distance_limit: PxPct) -> Self {
        Self::with_thresholds(
            Threshold::Limit(distance_limit),
            Threshold::Ignore,
            Threshold::Limit(outside_distance_limit),
        )
    }

    /// Create a click state with time and outside distance constraints (click distance ignored).
    ///
    /// # Arguments
    /// * `time_limit_ms` - Time threshold for click recognition  
    /// * `outside_distance_limit` - Distance threshold for node bound filtering
    pub fn with_time_and_movement(time_limit_ms: u64, outside_distance_limit: PxPct) -> Self {
        Self::with_thresholds(
            Threshold::Ignore,
            Threshold::Limit(time_limit_ms),
            Threshold::Limit(outside_distance_limit),
        )
    }

    /// Create a click state with only movement filtering (click thresholds ignored).
    ///
    /// # Arguments
    /// * `outside_distance_limit` - Distance threshold for node bound filtering
    pub fn movement_only(outside_distance_limit: PxPct) -> Self {
        Self::with_thresholds(
            Threshold::Ignore,
            Threshold::Ignore,
            Threshold::Limit(outside_distance_limit),
        )
    }

    /// Get the current distance threshold configuration
    pub fn distance_threshold(&self) -> &Threshold<f64> {
        &self.distance_threshold
    }

    /// Get the current time threshold configuration
    pub fn time_threshold(&self) -> &Threshold<u64> {
        &self.time_threshold
    }

    /// Get the current outside distance threshold configuration
    pub fn outside_distance_threshold(&self) -> &Threshold<PxPct> {
        &self.outside_distance_threshold
    }

    /// Get the last press
    pub fn last_press(&self) -> Option<&Press<K>> {
        self.last_press.as_ref()
    }

    /// Set a new distance threshold configuration.
    pub fn with_distance_threshold(mut self, threshold: Threshold<f64>) -> Self {
        self.distance_threshold = threshold;
        self
    }

    /// Set a new time threshold configuration.
    pub fn with_time_threshold(mut self, threshold: Threshold<u64>) -> Self {
        self.time_threshold = threshold;
        self
    }

    /// Set distance threshold to a specific limit.
    pub fn with_distance_limit(mut self, limit: f64) -> Self {
        self.distance_threshold = Threshold::Limit(limit);
        self
    }

    /// Set time threshold to a specific limit.
    pub fn with_time_limit(mut self, limit_ms: u64) -> Self {
        self.time_threshold = Threshold::Limit(limit_ms);
        self
    }

    /// Set distance threshold to unlimited.
    pub fn with_unlimited_distance(mut self) -> Self {
        self.distance_threshold = Threshold::Unlimited;
        self
    }

    /// Set time threshold to unlimited.
    pub fn with_unlimited_time(mut self) -> Self {
        self.time_threshold = Threshold::Unlimited;
        self
    }

    /// Ignore distance threshold.
    pub fn with_distance_ignored(mut self) -> Self {
        self.distance_threshold = Threshold::Ignore;
        self
    }

    /// Ignore time threshold.
    pub fn with_time_ignored(mut self) -> Self {
        self.time_threshold = Threshold::Ignore;
        self
    }

    /// Set outside distance threshold configuration.
    pub fn with_outside_distance_threshold(mut self, threshold: Threshold<PxPct>) -> Self {
        self.outside_distance_threshold = threshold;
        self
    }

    /// Set outside distance threshold to a specific limit.
    pub fn with_outside_distance_limit(mut self, limit: PxPct) -> Self {
        self.outside_distance_threshold = Threshold::Limit(limit);
        self
    }

    /// Set outside distance threshold to unlimited.
    pub fn with_unlimited_outside_distance(mut self) -> Self {
        self.outside_distance_threshold = Threshold::Unlimited;
        self
    }

    /// Ignore outside distance threshold.
    pub fn with_outside_distance_ignored(mut self) -> Self {
        self.outside_distance_threshold = Threshold::Ignore;
        self
    }

    /// Record a pointer down event.
    ///
    /// # Arguments
    /// * `pointer_id` - Unique identifier for this pointer (defaults to 1 if None)
    /// * `path` - The path that was pressed
    /// * `world_pos` - Position of the press in world coordinates
    /// * `button` - Which button was pressed
    /// * `timestamp_ms` - When the press occurred
    pub fn on_down(
        &mut self,
        pointer_id: Option<PointerId>,
        path: Vec<K>,
        world_pos: Point,
        button: Option<Button>,
        timestamp_ms: u64,
    ) {
        let pointer_id =
            pointer_id.unwrap_or(NonZeroU64::new(1).expect("1 is a valid non zero u64"));
        let button = button.unwrap_or(1);
        let press = Press {
            path,
            world_down_pos: world_pos,
            when_ms: timestamp_ms,
            button,
            filtered_nodes: Vec::new(),
        };
        self.last_press = Some(press.clone());
        self.presses.insert(pointer_id, press);
    }

    /// Process a pointer up event and determine if a click should be recognized.
    ///
    /// **Key difference from `on_move`:** This method **recognizes clicks** by building a final path
    /// of nodes that qualify for click events, while `on_move` **filters out nodes** that exceed
    /// movement thresholds but doesn't generate clicks.
    ///
    /// Path processing logic (same as `on_move`):
    /// 1. Button must match the original press (unique to `on_up`)
    /// 2. For each node in the press path:
    ///    - If node is in both press path AND current path: include it (ignore all thresholds)
    ///    - If node is only in press path: include it only if distance/time thresholds are met AND outside distance limits are respected
    ///    - Skip nodes that were already filtered out during movement (via `on_move`)
    /// 3. **Return:** Click path if any nodes qualify, or None with original path
    ///
    /// # Arguments
    /// * `pointer_id` - Unique identifier for this pointer (defaults to 1 if None)
    /// * `current_path` - The current path under the pointer (for intersection logic)
    /// * `world_pos` - Position of the release in world coordinates  
    /// * `button` - Which button was released
    /// * `timestamp_ms` - When the release occurred
    /// * `rect_lookup` - Function to get world bounds for a node
    ///
    /// # Returns
    /// `ClickResult::Click(path)` if a click is recognized, `ClickResult::None(original_path)` otherwise
    pub fn on_up<F>(
        &mut self,
        pointer_id: Option<PointerId>,
        current_path: Option<&[K]>,
        world_pos: Point,
        button: Option<Button>,
        timestamp_ms: u64,
        rect_lookup: F,
    ) -> ClickResult<K>
    where
        K: PartialEq,
        F: Fn(&K) -> Option<Rect>,
    {
        let pointer_id =
            pointer_id.unwrap_or(NonZeroU64::new(1).expect("1 is a valid non zero u64"));
        let button = button.unwrap_or(1);

        let press = match self.presses.remove(&pointer_id) {
            Some(press) => press,
            None => return ClickResult::None(Vec::new()), // No press recorded
        };

        // Check if button matches
        if press.button != button {
            return ClickResult::None(press.path);
        }

        // Check thresholds using new enum logic
        let distance = world_pos.distance(press.world_down_pos);
        let time_elapsed = timestamp_ms.saturating_sub(press.when_ms);

        let distance_contributes = match &self.distance_threshold {
            Threshold::Unlimited => true, // Always passes
            Threshold::Ignore => false,   // Don't consider distance
            Threshold::Limit(threshold) => distance <= *threshold,
        };

        let time_contributes = match &self.time_threshold {
            Threshold::Unlimited => true, // Always passes
            Threshold::Ignore => false,   // Don't consider time
            Threshold::Limit(threshold) => time_elapsed <= *threshold,
        };

        // Special case: if both thresholds are ignored, fall back to strict path intersection
        let thresholds_met = match (&self.distance_threshold, &self.time_threshold) {
            (Threshold::Ignore, Threshold::Ignore) => false, // Neither matters
            _ => distance_contributes || time_contributes,
        };

        // Build result path based on filtering logic
        let mut result_path = Vec::new();

        for node in &press.path {
            let in_current_path = current_path
                .map(|path| path.contains(node))
                .unwrap_or(false);

            // If node is in current path, always include it
            if in_current_path {
                result_path.push(node.clone());
                continue;
            }

            // Node is only in press path - check if it should be included
            // Skip nodes that were already filtered out during movement
            if press.filtered_nodes.contains(node) {
                continue;
            }

            // Check if thresholds are met
            if !thresholds_met {
                continue;
            }

            // Check outside distance threshold for nodes not in current path
            let outside_distance_ok = check_outside_distance(
                world_pos,
                &self.outside_distance_threshold,
                &rect_lookup,
                node,
                false, // for_filtering = false
            );

            if outside_distance_ok {
                result_path.push(node.clone());
            }
        }

        if result_path.is_empty() {
            ClickResult::None(press.path)
        } else {
            ClickResult::Click(result_path)
        }
    }

    /// Process a pointer move event and filter out nodes based on movement thresholds.
    ///
    /// **Key difference from `on_up`:** This method **filters out nodes** that exceed movement
    /// thresholds, permanently removing them from consideration, while `on_up` **recognizes clicks**
    /// by building a final path of nodes that qualify for click events.
    ///
    /// Path processing logic (same as `on_up`):
    /// - For each node in the press path:
    ///   - If node is in both press path AND current path: keep it (ignore all thresholds)
    ///   - If node is only in press path: filter it out if outside distance OR time thresholds are exceeded
    /// - Nodes filtered out here will be skipped by subsequent `on_up` calls
    /// - **Return:** Count of newly filtered nodes (not a click path)
    ///
    /// # Arguments
    /// * `pointer_id` - Unique identifier for this pointer (defaults to 1 if None)
    /// * `current_path` - Current hit path under the pointer
    /// * `world_pos` - Current position of the pointer in world coordinates
    /// * `timestamp_ms` - Current timestamp
    /// * `rect_lookup` - Function to get world bounds for a node
    ///
    /// # Returns
    /// Number of nodes that were newly filtered out
    pub fn on_move<F>(
        &mut self,
        pointer_id: Option<PointerId>,
        current_path: Option<&[K]>,
        world_pos: Point,
        timestamp_ms: u64,
        rect_lookup: F,
    ) -> usize
    where
        K: PartialEq,
        F: Fn(&K) -> Option<Rect>,
    {
        let pointer_id =
            pointer_id.unwrap_or(NonZeroU64::new(1).expect("1 is a valid non zero u64"));

        let Some(press) = self.presses.get_mut(&pointer_id) else {
            return 0;
        };

        let mut newly_filtered = 0;
        let time_elapsed = timestamp_ms.saturating_sub(press.when_ms);

        // Check time threshold first - applies globally
        let time_contributes = match &self.time_threshold {
            Threshold::Unlimited => true,
            Threshold::Ignore => false,
            Threshold::Limit(threshold) => time_elapsed <= *threshold,
        };

        for node in &press.path {
            // Skip if already filtered
            if press.filtered_nodes.contains(node) {
                continue;
            }

            // If node is in current path, keep it (no filtering)
            let in_current_path = current_path
                .map(|path| path.contains(node))
                .unwrap_or(false);

            if in_current_path {
                continue;
            }

            // Node is not in current path - check if it should be filtered
            let outside_distance_exceeded = check_outside_distance(
                world_pos,
                &self.outside_distance_threshold,
                &rect_lookup,
                node,
                true, // for_filtering = true
            );

            // Time threshold only matters if it's not ignored
            let time_filter = match &self.time_threshold {
                Threshold::Ignore => false, // Don't filter based on time if ignored
                _ => !time_contributes,
            };

            let should_filter = outside_distance_exceeded || time_filter;

            if should_filter && !press.filtered_nodes.contains(node) {
                press.filtered_nodes.push(node.clone());
                newly_filtered += 1;
            }
        }

        newly_filtered
    }

    /// Cancel the press for a given pointer (e.g., due to capture loss).
    ///
    /// # Arguments
    /// * `pointer_id` - Unique identifier for the pointer to cancel (defaults to 1 if None)
    ///
    /// # Returns
    /// `true` if a press was canceled, `false` if no press was active
    pub fn cancel(&mut self, pointer_id: impl Into<Option<PointerId>>) -> bool {
        let pointer_id = pointer_id
            .into()
            .unwrap_or(NonZeroU64::new(1).expect("1 is a valid non zero u64"));
        self.presses.remove(&pointer_id).is_some()
    }

    /// Clear all active presses.
    pub fn clear(&mut self) {
        self.presses.clear();
    }

    /// Check if a press is currently active for the given pointer.
    pub fn is_pressed(&self, pointer_id: impl Into<Option<PointerId>>) -> bool {
        let pointer_id = pointer_id
            .into()
            .unwrap_or(NonZeroU64::new(1).expect("1 is a valid non zero u64"));
        self.presses.contains_key(&pointer_id)
    }

    /// Get the path for an active press, if any.
    pub fn pressed_path(&self, pointer_id: impl Into<Option<PointerId>>) -> Option<&Vec<K>> {
        let pointer_id = pointer_id
            .into()
            .unwrap_or(NonZeroU64::new(1).expect("1 is a valid non zero u64"));
        self.presses.get(&pointer_id).map(|p| &p.path)
    }

    /// Check if any currently pressed path matches the given predicate.
    ///
    /// # Arguments
    /// * `predicate` - A closure that takes a reference to a path and returns true if it matches
    ///
    /// # Returns
    /// `true` if any currently pressed path matches the predicate, `false` otherwise
    ///
    /// # Example
    /// ```
    /// # use understory_responder::click::ClickState;
    /// let mut state: ClickState<u32> = ClickState::new();
    /// state.on_down(None, vec![1, 42], kurbo::Point::new(0.0, 0.0), Some(0), 1000);
    ///
    /// // Check if path containing 42 is being clicked
    /// assert!(state.is_clicking(|path| path.contains(&42)));
    /// assert!(!state.is_clicking(|path| path.contains(&100)));
    /// ```
    pub fn is_clicking<F>(&self, predicate: F) -> bool
    where
        F: Fn(&Vec<K>) -> bool,
    {
        self.presses.values().any(|press| predicate(&press.path))
    }

    /// Get the number of active presses.
    pub fn active_count(&self) -> usize {
        self.presses.len()
    }
}

impl<K: Clone> Default for ClickState<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn generic_click_recognized_within_distance_threshold() {
        let mut state: ClickState<u32> = ClickState::new();
        let path = vec![1, 42];

        // Press down using default pointer ID
        state.on_down(None, path.clone(), Point::new(10.0, 20.0), Some(0), 1000);
        assert!(state.is_pressed(None));
        assert_eq!(state.pressed_path(None), Some(&path));

        // Release within distance threshold - should get original path
        let result = state.on_up(None, None, Point::new(12.0, 22.0), Some(0), 1050, |_| None);
        match result {
            ClickResult::Click(click_path) => {
                assert_eq!(click_path, path);
            }
            ClickResult::None(_) => panic!("Expected click"),
        }
        assert!(!state.is_pressed(None));
    }

    #[test]
    fn generic_click_beyond_both_thresholds() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(
            Threshold::Limit(3.0),
            Threshold::Limit(50),
            Threshold::Ignore,
        );
        let path = vec![1, 42];

        // Press down
        state.on_down(None, path, Point::new(0.0, 0.0), Some(0), 1000);

        // Release outside both distance threshold (10.0 > 3.0) and time threshold (100ms > 50ms)
        let result = state.on_up(None, None, Point::new(10.0, 0.0), Some(0), 1100, |_| None);
        assert!(matches!(result, ClickResult::None(_)));
    }

    #[test]
    fn generic_click_within_distance_threshold_despite_time() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(
            Threshold::Limit(10.0),
            Threshold::Limit(100),
            Threshold::Ignore,
        );
        let path = vec![1, 42];

        // Press down
        state.on_down(None, path.clone(), Point::new(0.0, 0.0), Some(0), 1000);

        // Release late (exceeds time threshold) but within distance threshold
        // Should still get original path because distance is within threshold
        let result = state.on_up(None, None, Point::new(1.0, 1.0), Some(0), 1200, |_| None);
        match result {
            ClickResult::Click(click_path) => {
                assert_eq!(click_path, path);
            }
            ClickResult::None(_) => panic!("Expected click due to distance within threshold"),
        }
    }

    #[test]
    fn generic_click_canceled_by_wrong_button() {
        let mut state: ClickState<u32> = ClickState::new();
        let path = vec![1, 42];

        // Press with button 0
        state.on_down(None, path, Point::new(0.0, 0.0), Some(0), 1000);

        // Release with button 1
        let result = state.on_up(None, None, Point::new(1.0, 1.1), Some(1), 1050, |_| None);
        assert!(matches!(result, ClickResult::None(_)));
    }

    #[test]
    fn generic_within_time_threshold_despite_distance() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(
            Threshold::Limit(3.0),
            Threshold::Limit(200),
            Threshold::Ignore,
        );
        let path = vec![1, 42];

        // Press down
        state.on_down(None, path.clone(), Point::new(0.0, 0.0), Some(0), 1000);

        // Release far (exceeds distance threshold) but within time threshold
        // Should still get original path because time is within threshold
        let result = state.on_up(None, None, Point::new(10.0, 0.0), Some(0), 1100, |_| None);
        match result {
            ClickResult::Click(click_path) => {
                assert_eq!(click_path, path);
            }
            ClickResult::None(_) => panic!("Expected click due to time within threshold"),
        }
    }

    #[test]
    fn is_clicking_with_predicate() {
        let mut state: ClickState<u32> = ClickState::new();

        // Initially nothing is being clicked
        assert!(!state.is_clicking(|path| path.contains(&42)));
        assert!(!state.is_clicking(|path| path.iter().any(|&x| x > 0)));

        // Press down on path containing 42
        state.on_down(None, vec![1, 42], Point::new(0.0, 0.0), Some(0), 1000);

        // Check if path containing 42 is being clicked
        assert!(state.is_clicking(|path| path.contains(&42)));
        assert!(!state.is_clicking(|path| path.contains(&100)));
        assert!(state.is_clicking(|path| path.iter().any(|&x| x > 40)));
        assert!(state.is_clicking(|path| path.iter().any(|&x| x % 2 == 0))); // even number

        // Press down on another pointer with path containing 100
        let pointer2 = NonZeroU64::new(2).unwrap();
        state.on_down(
            Some(pointer2),
            vec![5, 100],
            Point::new(10.0, 10.0),
            Some(0),
            1001,
        );

        // Both paths should be detected
        assert!(state.is_clicking(|path| path.contains(&42)));
        assert!(state.is_clicking(|path| path.contains(&100)));
        assert!(state.is_clicking(|path| path.iter().any(|&x| x > 90)));

        // Release first path
        state.on_up(None, None, Point::new(1.0, 1.0), Some(0), 1050, |_| None);

        // Only second path should remain
        assert!(!state.is_clicking(|path| path.contains(&42)));
        assert!(state.is_clicking(|path| path.contains(&100)));

        // Release second path
        state.on_up(
            Some(pointer2),
            None,
            Point::new(11.0, 11.0),
            Some(0),
            1051,
            |_| None,
        );

        // Nothing should be clicked
        assert!(!state.is_clicking(|path| path.contains(&42)));
        assert!(!state.is_clicking(|path| path.contains(&100)));
        assert!(!state.is_clicking(|_| true));
    }

    #[test]
    fn path_filtering_logic() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(
            Threshold::Limit(3.0),
            Threshold::Ignore,
            Threshold::Ignore,
        );

        // Press down on path [1, 2, 3]
        let press_path = vec![1, 2, 3];
        state.on_down(
            None,
            press_path.clone(),
            Point::new(0.0, 0.0),
            Some(0),
            1000,
        );

        // Release far away (exceeds distance) with current path [2, 4] (only 2 is shared)
        // No time threshold, so time is not considered met
        let current_path = vec![2, 4];
        let result = state.on_up(
            None,
            Some(&current_path),
            Point::new(10.0, 0.0),
            Some(0),
            1050,
            |_| None,
        );

        match result {
            ClickResult::Click(filtered_path) => {
                // Should only contain node 2 (shared between press and current)
                // Nodes 1 and 3 are filtered out because they're not in current path AND thresholds not met
                assert_eq!(filtered_path, vec![2]);
            }
            ClickResult::None(_) => panic!("Expected filtered path with shared node"),
        }
    }

    #[test]
    fn path_filtering_with_thresholds_met() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(
            Threshold::Limit(10.0),
            Threshold::Limit(100),
            Threshold::Ignore,
        );

        // Press down on path [1, 2, 3]
        let press_path = vec![1, 2, 3];
        state.on_down(
            None,
            press_path.clone(),
            Point::new(0.0, 0.0),
            Some(0),
            1000,
        );

        // Release within distance and within time with current path [2, 4]
        let current_path = vec![2, 4];
        let result = state.on_up(
            None,
            Some(&current_path),
            Point::new(2.0, 0.0),
            Some(0),
            1050,
            |_| None,
        ); // 50ms < 100ms

        match result {
            ClickResult::Click(filtered_path) => {
                // Should contain all nodes from press path because distance threshold is met
                assert_eq!(filtered_path, vec![1, 2, 3]);
            }
            ClickResult::None(_) => panic!("Expected full path due to distance within threshold"),
        }
    }

    #[test]
    fn on_move_with_path_intersection_no_filtering() {
        let mut state: ClickState<u32> = ClickState::new();
        let press_path = vec![1, 2, 3];

        // Press down
        state.on_down(None, press_path, Point::new(0.0, 0.0), Some(0), 1000);
        assert!(state.is_pressed(None));

        // Move with current path that intersects with press path
        let current_path = vec![2, 4]; // Contains node 2 from press path
        let filtered_count = state.on_move(
            None,
            Some(&current_path),
            Point::new(100.0, 100.0),
            1500,
            |_| None, // No rect lookup needed for intersection logic
        );

        // Should not filter any nodes because of path intersection
        assert_eq!(filtered_count, 0);
        assert!(state.is_pressed(None));
    }

    #[test]
    fn on_move_filters_nodes_outside_rect_bounds() {
        use kurbo::Rect;

        let mut state: ClickState<u32> =
            ClickState::new().with_outside_distance_limit(PxPct::Px(10.0));
        let press_path = vec![1, 2, 3];

        // Press down
        state.on_down(None, press_path, Point::new(50.0, 50.0), Some(0), 1000);

        // Move with no current path intersection but provide rects
        let current_path = vec![4, 5]; // No intersection
        let filtered_count = state.on_move(
            None,
            Some(&current_path),
            Point::new(200.0, 200.0), // Far from all rects
            1050,
            |&node| {
                match node {
                    1 => Some(Rect::new(0.0, 0.0, 20.0, 20.0)), // Far from pointer
                    2 => Some(Rect::new(190.0, 190.0, 210.0, 210.0)), // Near pointer
                    3 => Some(Rect::new(100.0, 100.0, 120.0, 120.0)), // Far from pointer
                    _ => None,
                }
            },
        );

        // Should filter nodes 1 and 3 (far from pointer), but not 2 (near pointer)
        assert_eq!(filtered_count, 2);
        assert!(state.is_pressed(None));

        // Check which nodes were filtered
        let press = state.presses.values().next().unwrap();
        assert!(press.filtered_nodes.contains(&1));
        assert!(!press.filtered_nodes.contains(&2));
        assert!(press.filtered_nodes.contains(&3));
    }

    #[test]
    fn on_move_percentage_threshold() {
        use kurbo::Rect;

        let mut state: ClickState<u32> =
            ClickState::new().with_outside_distance_limit(PxPct::Pct(0.5)); // 50% of rect size
        let press_path = vec![1, 2];

        // Press down
        state.on_down(None, press_path, Point::new(0.0, 0.0), Some(0), 1000);

        let current_path = vec![3]; // No intersection
        let filtered_count = state.on_move(
            None,
            Some(&current_path),
            Point::new(50.0, 0.0),
            1050,
            |&node| {
                match node {
                    1 => Some(Rect::new(0.0, 0.0, 20.0, 20.0)), // Size 20x20, 50% = 10px
                    2 => Some(Rect::new(0.0, 0.0, 100.0, 100.0)), // Size 100x100, 50% = 50px
                    _ => None,
                }
            },
        );

        // Node 1: distance = 30, threshold = 10 -> filtered
        // Node 2: distance = 50, threshold = 50 -> not filtered (exactly at threshold)
        assert_eq!(filtered_count, 1);

        let press = state.presses.values().next().unwrap();
        assert!(press.filtered_nodes.contains(&1));
        assert!(!press.filtered_nodes.contains(&2));
    }

    #[test]
    fn distance_outside_rect_inside_rect() {
        use kurbo::Rect;

        let rect = Rect::new(10.0, 20.0, 30.0, 40.0);

        // Point inside rect
        assert_eq!(distance_outside_rect(Point::new(20.0, 30.0), rect), 0.0);

        // Point on edge
        assert_eq!(distance_outside_rect(Point::new(10.0, 30.0), rect), 0.0);
        assert_eq!(distance_outside_rect(Point::new(30.0, 30.0), rect), 0.0);
        assert_eq!(distance_outside_rect(Point::new(20.0, 20.0), rect), 0.0);
        assert_eq!(distance_outside_rect(Point::new(20.0, 40.0), rect), 0.0);

        // Point at corner
        assert_eq!(distance_outside_rect(Point::new(10.0, 20.0), rect), 0.0);
        assert_eq!(distance_outside_rect(Point::new(30.0, 40.0), rect), 0.0);
    }

    #[test]
    fn distance_outside_rect_outside_rect() {
        let rect = Rect::new(10.0, 20.0, 30.0, 40.0);

        // Point directly left/right/up/down - same for both Manhattan and Euclidean
        assert_eq!(distance_outside_rect(Point::new(5.0, 30.0), rect), 5.0);
        assert_eq!(distance_outside_rect(Point::new(35.0, 30.0), rect), 5.0);
        assert_eq!(distance_outside_rect(Point::new(20.0, 15.0), rect), 5.0);
        assert_eq!(distance_outside_rect(Point::new(20.0, 45.0), rect), 5.0);

        // Diagonal cases - now using Euclidean distance
        let distance = distance_outside_rect(Point::new(5.0, 15.0), rect);
        let expected = (5.0_f64.powi(2) + 5.0_f64.powi(2)).sqrt(); // sqrt(50) ≈ 7.071
        assert!((distance - expected).abs() < 1e-10);

        let distance = distance_outside_rect(Point::new(35.0, 45.0), rect);
        let expected = (5.0_f64.powi(2) + 5.0_f64.powi(2)).sqrt();
        assert!((distance - expected).abs() < 1e-10);
    }

    #[test]
    fn pxpct_to_pixels() {
        // Px variant
        assert_eq!(PxPct::Px(10.0).to_pixels(100.0), 10.0);
        assert_eq!(PxPct::Px(5.5).to_pixels(50.0), 5.5);

        // Pct variant
        assert_eq!(PxPct::Pct(0.5).to_pixels(100.0), 50.0);
        assert_eq!(PxPct::Pct(0.25).to_pixels(80.0), 20.0);
        assert_eq!(PxPct::Pct(1.0).to_pixels(50.0), 50.0);
        assert_eq!(PxPct::Pct(0.0).to_pixels(100.0), 0.0);

        // Pct clamping
        assert_eq!(PxPct::Pct(1.5).to_pixels(100.0), 100.0); // Clamped to 1.0
        assert_eq!(PxPct::Pct(-0.5).to_pixels(100.0), 0.0); // Clamped to 0.0
    }

    #[test]
    fn on_up_respects_outside_distance_threshold() {
        use kurbo::Rect;

        // Set up click state with outside distance limit of 10 pixels
        // Use unlimited thresholds so only outside distance matters
        let mut state: ClickState<u32> =
            ClickState::unlimited().with_outside_distance_limit(PxPct::Px(10.0));
        let press_path = vec![1, 2, 3];

        // Mock rect lookup that returns bounds for our nodes
        let rect_lookup = |node: &u32| match *node {
            1 => Some(Rect::new(0.0, 0.0, 50.0, 50.0)), // Node 1: 50x50 rect at origin
            2 => Some(Rect::new(100.0, 0.0, 150.0, 50.0)), // Node 2: 50x50 rect at x=100
            3 => Some(Rect::new(200.0, 0.0, 250.0, 50.0)), // Node 3: 50x50 rect at x=200
            _ => None,
        };

        // Press down at the center of node 1
        state.on_down(
            None,
            press_path.clone(),
            Point::new(25.0, 25.0),
            Some(0),
            1000,
        );

        // Pointer up far outside all nodes (distance > 10 from any node)
        // This should filter out all nodes since they're all outside the distance threshold
        let result = state.on_up(
            None,
            None,                     // No current path - all nodes are only in press path
            Point::new(300.0, 100.0), // Far from all nodes
            Some(0),
            1050,
            rect_lookup,
        );

        // Should result in no click since all nodes exceed outside distance threshold
        match result {
            ClickResult::None(original_path) => {
                assert_eq!(original_path, press_path);
            }
            ClickResult::Click(_) => {
                panic!("Expected no click due to outside distance threshold");
            }
        }

        // Test case where pointer is close to node 1 (within 10 pixels)
        // Use unlimited distance threshold so only outside distance matters
        let mut state2: ClickState<u32> =
            ClickState::unlimited().with_outside_distance_limit(PxPct::Px(10.0));
        state2.on_down(
            None,
            press_path.clone(),
            Point::new(25.0, 25.0),
            Some(0),
            2000,
        );

        let result2 = state2.on_up(
            None,
            None,
            Point::new(30.0, 30.0), // Close to node 1's rect (5px outside edge)
            Some(0),
            2050,
            rect_lookup,
        );

        // Should result in click with only node 1 (others are too far)
        match result2 {
            ClickResult::Click(filtered_path) => {
                assert_eq!(filtered_path, vec![1]); // Only node 1 is within outside distance
            }
            ClickResult::None(_) => {
                panic!("Expected click with node 1");
            }
        }
    }
}
