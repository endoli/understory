// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Click event generation helper for transformed elements.
//!
//! This module provides utilities for correctly emitting click events when elements
//! may transform or move during pointer interaction. It preserves user intent by
//! applying configurable spatial and temporal tolerance to determine when clicks
//! should be recognized despite apparent target changes.
//!
//! ## Primary Use Case
//!
//! **Transform-Aware Click Recognition**: When elements animate, resize, or reposition
//! between pointer down and up events, this module determines whether the user's
//! original intent should still generate a click event.
//!
//! Common scenarios in dynamic UIs:
//! - Button animations on press (size/position changes)
//! - Layout shifts during interaction
//! - Element transformations from CSS transitions
//! - Drag-and-drop feedback transforms
//! - Hover state changes that affect positioning
//!
//! ## Usage
//!
//! Basic click detection:
//! ```
//! use understory_event_state::click::{ClickState, ClickResult};
//! use kurbo::Point;
//!
//! let mut state: ClickState<u32> = ClickState::new();
//!
//! // Press down on element 42
//! state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
//!
//! // Same target generates click regardless of movement
//! let result = state.on_up(None, None, &42, Point::new(100.0, 200.0), 2000);
//! assert!(matches!(result, ClickResult::Click(42)));
//! ```
//!
//! Transform-tolerant click detection with movement tracking:
//! ```
//! # use understory_event_state::click::{ClickState, ClickResult};
//! # use kurbo::Point;
//! // Configure spatial tolerance of 10px and temporal tolerance of 500ms
//! let mut state: ClickState<u32> = ClickState::with_thresholds(Some(10.0), Some(500));
//!
//! // Press down on element 42
//! state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
//!
//! // Track movement during interaction (optional - helps detect dragging vs clicking)
//! let exceeded = state.on_move(None, Point::new(15.0, 25.0)); // 7px moved
//! assert!(exceeded.is_none()); // Still within 10px threshold
//!
//! // Element transforms, up occurs on different element but within tolerance
//! let result = state.on_up(None, None, &99, Point::new(18.0, 26.0), 1200); // 10px total, 200ms elapsed
//! match result {
//!     ClickResult::Click(target) => {
//!         assert_eq!(target, 42); // Click on original target despite transform
//!     }
//!     ClickResult::Suppressed(_) => {}
//! }
//! ```
//!
//! ## Click Generation Rules
//!
//! 1. **Same Target**: If down and up targets match, click is always generated
//! 2. **Different Targets with Thresholds**: Click generated if all conditions met:
//!    - No excessive movement: Distance threshold was not exceeded during `on_move` calls
//!    - Spatial: `pointer_distance <= total_pointer_moved_threshold` (if configured)
//!    - Temporal: `elapsed_time <= time_threshold` (if configured)
//! 3. **Different Targets without Thresholds**: No click generated
//! 4. **Movement Exceeded**: If `on_move` recorded threshold violation, no click for different targets
//! 5. **Button Mismatch**: No click generated
//! 6. **No Active Press**: No click generated
//!
//! ## Threshold Configuration
//!
//! **Note**: Thresholds only apply when down and up targets differ. Same-target clicks always succeed.
//!
//! - **`Some(value)`**: Threshold before rejecting user intent as a click for different targets
//! - **`None`**: No rejection threshold applied for different-target clicks (unlimited tolerance)
//! - **Both `None`**: Only same-target clicks generate events
//! - **AND Logic**: When both thresholds are configured, both must pass to preserve click intent
//!
//! ## Multi-Pointer Support
//!
//! Each pointer is tracked independently:
//! ```
//! # use understory_event_state::click::{ClickState, ClickResult, PointerId};
//! # use core::num::NonZeroU64;
//! # use kurbo::Point;
//! let mut state: ClickState<u32> = ClickState::new();
//!
//! let pointer1 = NonZeroU64::new(1).unwrap();
//! let pointer2 = NonZeroU64::new(2).unwrap();
//!
//! state.on_down(Some(pointer1), None, 42, Point::new(10.0, 20.0), 1000);
//! state.on_down(Some(pointer2), None, 99, Point::new(50.0, 60.0), 1010);
//!
//! // Each pointer can generate clicks independently
//! let result1 = state.on_up(Some(pointer1), None, &42, Point::new(12.0, 22.0), 1050);
//! let result2 = state.on_up(Some(pointer2), None, &99, Point::new(52.0, 62.0), 1080);
//! ```

use alloc::collections::BTreeMap;
use core::num::NonZeroU64;
use kurbo::Point;

/// Pointer identifier for tracking multiple concurrent presses.
pub type PointerId = NonZeroU64;

/// Mouse button identifier.
pub type Button = u8;

/// Click event generation state machine for transform-aware UIs.
///
/// Primary purpose: track element clicking state and preserve user click intent when elements
/// transform during interaction. Tracks active pointer presses and determines when click events
/// should be generated despite apparent target changes caused by element transformation.
///
/// The state machine maintains independent tracking for multiple concurrent pointers,
/// applying spatial and temporal tolerance to determine when pointer up events
/// should generate click events on their original target elements rather than
/// the apparent target under the final pointer position.
#[derive(Clone, Debug)]
pub struct ClickState<K> {
    /// Active presses per pointer
    presses: BTreeMap<PointerId, Press<K>>,
    /// Distance threshold before rejecting user intent as a click when targets differ
    pub total_pointer_moved_threshold: Option<f64>,
    /// Time threshold before rejecting user intent as a click when targets differ (milliseconds)
    pub time_threshold: Option<u64>,
    /// The last click that was registered.
    last_click: Option<Press<K>>,
}

/// State for an active pointer press.
#[derive(Clone, Debug)]
pub struct Press<K> {
    /// Target element where press occurred
    pub target: K,
    /// Pointer position at press time
    pub down_position: Point,
    /// Timestamp when press occurred
    pub down_time: u64,
    /// Button that was pressed
    pub button: Button,
    /// True if distance threshold was exceeded during movement
    pub distance_exceeded: bool,
}

/// Result of click event processing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClickResult<K> {
    /// Click event should be generated on the specified target
    Click(K),
    /// Click event was suppressed, contains the originally hit target if there is one associated with the pointer
    Suppressed(Option<K>),
}

impl<K: PartialEq + Clone> ClickState<K> {
    /// Create a new click state with default thresholds.
    ///
    /// Default configuration uses a 5-pixel spatial tolerance and 100ms temporal tolerance
    /// to filter out unintended clicks. If the pointer moves more than 5 pixels or takes
    /// longer than 100ms between different targets, user intent is rejected as a click.
    pub fn new() -> Self {
        Self {
            presses: BTreeMap::new(),
            total_pointer_moved_threshold: Some(5.0), // 5-pixel tolerance before rejecting click intent
            time_threshold: Some(100), // 100ms tolerance before rejecting click intent
            last_click: None,
        }
    }

    /// Create a new click state with custom thresholds.
    ///
    /// # Arguments
    /// * `total_pointer_moved_threshold` - Distance threshold before rejecting click intent when targets differ, or None for unlimited
    /// * `time_threshold` - Time threshold in milliseconds before rejecting click intent when targets differ, or None for unlimited
    pub fn with_thresholds(
        total_pointer_moved_threshold: Option<f64>,
        time_threshold: Option<u64>,
    ) -> Self {
        Self {
            presses: BTreeMap::new(),
            total_pointer_moved_threshold,
            time_threshold,
            last_click: None,
        }
    }

    /// Record a pointer down event.
    ///
    /// # Arguments
    /// * `pointer_id` - Unique pointer identifier, defaults to 1 if None
    /// * `button` - Button that was pressed
    /// * `target` - Target element where press occurred
    /// * `position` - Pointer position at press time
    /// * `timestamp` - Event timestamp in milliseconds
    pub fn on_down(
        &mut self,
        pointer_id: Option<PointerId>,
        button: Option<Button>,
        target: K,
        position: Point,
        timestamp: u64,
    ) {
        let pointer_id = pointer_id.unwrap_or(NonZeroU64::new(1).expect("1 is valid non-zero"));
        let button = button.unwrap_or(1);
        let press = Press {
            target,
            down_position: position,
            down_time: timestamp,
            button,
            distance_exceeded: false,
        };
        self.presses.insert(pointer_id, press);
    }

    /// Process a pointer up event and determine if a click should be generated.
    ///
    /// Evaluates whether the pointer up event should generate a click based on:
    /// 1. Target matching between down and up events
    /// 2. Spatial tolerance (if configured)
    /// 3. Temporal tolerance (if configured)
    /// 4. Button consistency
    ///
    /// # Arguments
    /// * `pointer_id` - Pointer identifier, defaults to 1 if None
    /// * `button` - Button that was released, defaults to 1 if None
    /// * `current_target` - Target element where release occurred
    /// * `position` - Pointer position at release time
    /// * `timestamp` - Event timestamp in milliseconds
    ///
    /// # Returns
    /// `ClickResult::Click(original_target)` if click should be generated on the original
    /// press target, `ClickResult::Suppressed(None)` otherwise
    pub fn on_up(
        &mut self,
        pointer_id: Option<PointerId>,
        button: Option<Button>,
        current_target: &K,
        position: Point,
        timestamp: u64,
    ) -> ClickResult<K> {
        let pointer_id = pointer_id.unwrap_or(NonZeroU64::new(1).expect("1 is valid non-zero"));
        let button = button.unwrap_or(1);

        let press = match self.presses.remove(&pointer_id) {
            Some(press) => press,
            None => return ClickResult::Suppressed(None), // No active press
        };

        // Button must match
        if press.button != button {
            return ClickResult::Suppressed(Some(press.target));
        }

        // Fast path: same target
        if press.target == *current_target {
            // Store last successful click.
            self.last_click = Some(press.clone());
            return ClickResult::Click(press.target);
        }

        // Different targets - check if any thresholds are configured
        if self.total_pointer_moved_threshold.is_none() && self.time_threshold.is_none() {
            // No thresholds configured - only same targets generate clicks
            return ClickResult::Suppressed(Some(press.target));
        }

        // If distance was exceeded during movement, no click for different targets
        if press.distance_exceeded {
            return ClickResult::Suppressed(Some(press.target));
        }

        // Check thresholds at release time
        let distance_moved = press.down_position.distance(position);
        let time_elapsed = timestamp.saturating_sub(press.down_time);

        let distance_ok = self
            .total_pointer_moved_threshold
            .is_none_or(|threshold| distance_moved <= threshold);

        let time_ok = self
            .time_threshold
            .is_none_or(|threshold| time_elapsed <= threshold);

        if distance_ok && time_ok {
            // Store last successful click.
            self.last_click = Some(press.clone());
            ClickResult::Click(press.target)
        } else {
            ClickResult::Suppressed(Some(press.target))
        }
    }

    /// Process a pointer move event and track distance threshold violations.
    ///
    /// Records when the pointer has moved beyond the rejection threshold during an active press.
    /// This affects subsequent click generation in `on_up` for cases where down and up targets differ.
    /// Same-target clicks are never affected by movement tracking.
    ///
    /// # Arguments
    /// * `pointer_id` - Pointer identifier, defaults to 1 if None
    /// * `position` - Current pointer position
    ///
    /// # Returns
    /// `Some(target)` if rejection threshold was exceeded and newly recorded, `None` otherwise
    pub fn on_move(&mut self, pointer_id: Option<PointerId>, position: Point) -> Option<K> {
        let pointer_id = pointer_id.unwrap_or(NonZeroU64::new(1).expect("1 is valid non-zero"));

        let Some(press) = self.presses.get_mut(&pointer_id) else {
            return None; // No active press
        };

        // Only check distance threshold if we haven't already recorded a violation
        let mut newly_exceeded = false;
        if !press.distance_exceeded {
            // Check if spatial threshold is configured and exceeded
            if let Some(threshold) = self.total_pointer_moved_threshold {
                let distance_moved = press.down_position.distance(position);
                if distance_moved > threshold {
                    press.distance_exceeded = true;
                    newly_exceeded = true;
                }
            }
        }

        if newly_exceeded {
            Some(press.target.clone())
        } else {
            None
        }
    }

    /// Cancel all active presses for a pointer.
    ///
    /// # Arguments
    /// * `pointer_id` - Pointer to cancel, defaults to 1 if None
    ///
    /// # Returns
    /// `true` if a press was canceled, `false` if no press was active
    pub fn cancel(&mut self, pointer_id: Option<PointerId>) -> bool {
        let pointer_id = pointer_id.unwrap_or(NonZeroU64::new(1).expect("1 is valid non-zero"));
        self.presses.remove(&pointer_id).is_some()
    }

    /// Check if a pointer has an active press.
    pub fn is_pressed(&self, pointer_id: Option<PointerId>) -> bool {
        let pointer_id = pointer_id.unwrap_or(NonZeroU64::new(1).expect("1 is valid non-zero"));
        self.presses.contains_key(&pointer_id)
    }

    /// Check if there is an active press on the specified target.
    ///
    /// # Arguments
    /// * `query_target` - Target element to check for active press
    ///
    /// # Returns
    /// `true` if there is an active press on the query target, `false` otherwise
    pub fn has_active_press(&self, query_target: &K) -> bool {
        self.presses
            .values()
            .any(|press| press.target == *query_target)
    }

    /// Clear all active presses.
    pub fn clear(&mut self) {
        self.presses.clear();
    }

    /// Get the active press for a specific pointer ID.
    ///
    /// Returns `None` if there is no active press for the given pointer.
    pub fn get_press(&self, pointer_id: PointerId) -> Option<&Press<K>> {
        self.presses.get(&pointer_id)
    }

    /// Get an iterator over all active presses.
    ///
    /// Returns an iterator that yields references to all currently active press states.
    pub fn presses(&self) -> impl Iterator<Item = &Press<K>> {
        self.presses.values()
    }

    /// Get the last press that produced a `ClickResult::Click`.
    ///
    /// Returns a clone of the `Press` that most recently resulted in a click,
    /// or `None` if no click has been generated yet.
    pub fn last_click(&self) -> Option<Press<K>> {
        self.last_click.clone()
    }

    /// Get the target of the last click, if any.
    pub fn last_click_target(&self) -> Option<&K> {
        self.last_click.as_ref().map(|p| &p.target)
    }
}

impl<K: PartialEq + Clone> Default for ClickState<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_target_generates_click() {
        let mut state: ClickState<u32> = ClickState::new();

        // Press and release on same target
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &42, Point::new(12.0, 22.0), 1050);

        assert_eq!(result, ClickResult::Click(42));
        assert!(!state.is_pressed(None));
    }

    #[test]
    fn different_targets_no_thresholds_no_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(None, None);

        // Press on 42, release on 99
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(10.0, 20.0), 1050);

        assert_eq!(result, ClickResult::Suppressed(Some(42)));
    }

    #[test]
    fn wrong_button_no_click() {
        let mut state: ClickState<u32> = ClickState::new();

        // Press with button 1, release with button 2
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, Some(2), &42, Point::new(10.0, 20.0), 1050);

        assert_eq!(result, ClickResult::Suppressed(Some(42)));
    }

    #[test]
    fn no_active_press_no_click() {
        let mut state: ClickState<u32> = ClickState::new();

        // Release without press
        let result = state.on_up(None, None, &42, Point::new(10.0, 20.0), 1000);

        assert_eq!(result, ClickResult::Suppressed(None));
    }

    #[test]
    fn spatial_tolerance_within_threshold_generates_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), None);

        // Press on 42, release on 99 but within 5.0 distance
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(13.0, 23.0), 1050); // ~4.24 distance

        assert_eq!(result, ClickResult::Click(42));
    }

    #[test]
    fn spatial_tolerance_beyond_threshold_no_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), None);

        // Press on 42, release on 99 beyond 5.0 distance
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(20.0, 30.0), 1050); // ~14.14 distance

        assert_eq!(result, ClickResult::Suppressed(Some(42)));
    }

    #[test]
    fn spatial_tolerance_exact_threshold_generates_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), None);

        // Press on 42, release on 99 exactly at 5.0 distance
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(15.0, 20.0), 1050); // exactly 5.0 distance

        assert_eq!(result, ClickResult::Click(42));
    }

    #[test]
    fn temporal_tolerance_within_threshold_generates_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(None, Some(100));

        // Press on 42, release on 99 within 100ms
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(100.0, 200.0), 1050); // 50ms elapsed

        assert_eq!(result, ClickResult::Click(42));
    }

    #[test]
    fn temporal_tolerance_beyond_threshold_no_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(None, Some(100));

        // Press on 42, release on 99 beyond 100ms
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(100.0, 200.0), 1200); // 200ms elapsed

        assert_eq!(result, ClickResult::Suppressed(Some(42)));
    }

    #[test]
    fn temporal_tolerance_exact_threshold_generates_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(None, Some(100));

        // Press on 42, release on 99 exactly at 100ms
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(100.0, 200.0), 1100); // exactly 100ms elapsed

        assert_eq!(result, ClickResult::Click(42));
    }

    #[test]
    fn combined_thresholds_both_pass_generates_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), Some(100));

        // Press on 42, release on 99 within both thresholds
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(13.0, 23.0), 1050); // ~4.24 distance, 50ms

        assert_eq!(result, ClickResult::Click(42));
    }

    #[test]
    fn combined_thresholds_distance_fails_no_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), Some(100));

        // Press on 42, release on 99 - time ok, distance too far
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(20.0, 30.0), 1050); // ~14.14 distance, 50ms

        assert_eq!(result, ClickResult::Suppressed(Some(42)));
    }

    #[test]
    fn combined_thresholds_time_fails_no_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), Some(100));

        // Press on 42, release on 99 - distance ok, time too long
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(13.0, 23.0), 1200); // ~4.24 distance, 200ms

        assert_eq!(result, ClickResult::Suppressed(Some(42)));
    }

    #[test]
    fn combined_thresholds_both_fail_no_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), Some(100));

        // Press on 42, release on 99 - both thresholds exceeded
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &99, Point::new(20.0, 30.0), 1200); // ~14.14 distance, 200ms

        assert_eq!(result, ClickResult::Suppressed(Some(42)));
    }

    #[test]
    fn multi_pointer_independent_tracking() {
        let mut state: ClickState<u32> = ClickState::new();

        let pointer1 = NonZeroU64::new(1).unwrap();
        let pointer2 = NonZeroU64::new(2).unwrap();

        // Press on different targets with different pointers
        state.on_down(Some(pointer1), None, 42, Point::new(10.0, 20.0), 1000);
        state.on_down(Some(pointer2), None, 99, Point::new(50.0, 60.0), 1010);

        assert!(state.is_pressed(Some(pointer1)));
        assert!(state.is_pressed(Some(pointer2)));

        // Release pointer1
        let result1 = state.on_up(Some(pointer1), None, &42, Point::new(12.0, 22.0), 1050);
        assert_eq!(result1, ClickResult::Click(42));
        assert!(!state.is_pressed(Some(pointer1)));
        assert!(state.is_pressed(Some(pointer2)));

        // Release pointer2
        let result2 = state.on_up(Some(pointer2), None, &99, Point::new(52.0, 62.0), 1080);
        assert_eq!(result2, ClickResult::Click(99));
        assert!(!state.is_pressed(Some(pointer2)));
    }

    #[test]
    fn multi_pointer_no_interference() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), None);

        let pointer1 = NonZeroU64::new(1).unwrap();
        let pointer2 = NonZeroU64::new(2).unwrap();

        // Press on same target with different pointers at different positions
        state.on_down(Some(pointer1), None, 42, Point::new(10.0, 20.0), 1000);
        state.on_down(Some(pointer2), None, 42, Point::new(100.0, 200.0), 1010);

        // Release pointer1 within its threshold
        let result1 = state.on_up(Some(pointer1), None, &99, Point::new(13.0, 23.0), 1050); // ~4.24 from pointer1 down
        assert_eq!(result1, ClickResult::Click(42));

        // Release pointer2 far from its down position (should fail threshold)
        let result2 = state.on_up(Some(pointer2), None, &99, Point::new(13.0, 23.0), 1080); // ~247 from pointer2 down
        assert_eq!(result2, ClickResult::Suppressed(Some(42)));
    }

    #[test]
    fn cancel_pointer_removes_press() {
        let mut state: ClickState<u32> = ClickState::new();

        let pointer1 = NonZeroU64::new(1).unwrap();
        let pointer2 = NonZeroU64::new(2).unwrap();

        // Press with both pointers
        state.on_down(Some(pointer1), None, 42, Point::new(10.0, 20.0), 1000);
        state.on_down(Some(pointer2), None, 99, Point::new(50.0, 60.0), 1010);

        assert!(state.is_pressed(Some(pointer1)));
        assert!(state.is_pressed(Some(pointer2)));

        // Cancel pointer1
        let canceled = state.cancel(Some(pointer1));
        assert!(canceled);
        assert!(!state.is_pressed(Some(pointer1)));
        assert!(state.is_pressed(Some(pointer2)));

        // Try to cancel pointer1 again
        let canceled_again = state.cancel(Some(pointer1));
        assert!(!canceled_again);

        // pointer2 should still work normally
        let result = state.on_up(Some(pointer2), None, &99, Point::new(52.0, 62.0), 1080);
        assert_eq!(result, ClickResult::Click(99));
    }

    #[test]
    fn clear_removes_all_presses() {
        let mut state: ClickState<u32> = ClickState::new();

        let pointer1 = NonZeroU64::new(1).unwrap();
        let pointer2 = NonZeroU64::new(2).unwrap();

        // Press with both pointers
        state.on_down(Some(pointer1), None, 42, Point::new(10.0, 20.0), 1000);
        state.on_down(Some(pointer2), None, 99, Point::new(50.0, 60.0), 1010);

        assert!(state.is_pressed(Some(pointer1)));
        assert!(state.is_pressed(Some(pointer2)));

        // Clear all
        state.clear();

        assert!(!state.is_pressed(Some(pointer1)));
        assert!(!state.is_pressed(Some(pointer2)));

        // No clicks should be generated after clear
        let result = state.on_up(Some(pointer1), None, &42, Point::new(12.0, 22.0), 1080);
        assert_eq!(result, ClickResult::Suppressed(None));
    }

    #[test]
    fn on_move_tracks_distance_exceeded() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), None);

        // Press down
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);

        // Move within threshold - should not mark as exceeded
        let exceeded1 = state.on_move(None, Point::new(13.0, 23.0)); // ~4.24 distance
        assert!(exceeded1.is_none());

        // Move beyond threshold - should mark as exceeded
        let exceeded2 = state.on_move(None, Point::new(20.0, 30.0)); // ~14.14 distance
        assert_eq!(exceeded2, Some(42));

        // Subsequent moves should not report exceeded again
        let exceeded3 = state.on_move(None, Point::new(30.0, 40.0));
        assert!(exceeded3.is_none());
    }

    #[test]
    fn distance_exceeded_during_move_blocks_different_target_click() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), None);

        // Press down
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);

        // Move beyond threshold
        state.on_move(None, Point::new(20.0, 30.0)); // ~14.14 distance, exceeds 5.0

        // Try to release on different target, even within threshold of final position
        let result = state.on_up(None, None, &99, Point::new(22.0, 32.0), 1050); // only 2.83 from final position
        assert_eq!(result, ClickResult::Suppressed(Some(42)));
    }

    #[test]
    fn distance_exceeded_during_move_allows_same_target_click() {
        let mut state: ClickState<u32> = ClickState::new();

        // Press down
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);

        // Move far beyond threshold
        state.on_move(None, Point::new(100.0, 200.0));

        // Same target should still generate click regardless of movement
        let result = state.on_up(None, None, &42, Point::new(200.0, 400.0), 1050);
        assert_eq!(result, ClickResult::Click(42));
    }

    #[test]
    fn on_move_with_no_spatial_threshold_configured() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(None, Some(100));

        // Press down
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);

        // Move any distance - should not be recorded as exceeded since no spatial threshold
        let exceeded = state.on_move(None, Point::new(1000.0, 2000.0));
        assert!(exceeded.is_none());

        // Different target click should still work if time is within threshold
        let result = state.on_up(None, None, &99, Point::new(1000.0, 2000.0), 1050);
        assert_eq!(result, ClickResult::Click(42));
    }

    #[test]
    fn on_move_with_no_active_press() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), None);

        // Move without any press
        let exceeded = state.on_move(None, Point::new(100.0, 200.0));
        assert!(exceeded.is_none());
    }

    #[test]
    fn multi_pointer_independent_move_tracking() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), None);

        let pointer1 = NonZeroU64::new(1).unwrap();
        let pointer2 = NonZeroU64::new(2).unwrap();

        // Press down on same target with different pointers
        state.on_down(Some(pointer1), None, 42, Point::new(10.0, 20.0), 1000);
        state.on_down(Some(pointer2), None, 42, Point::new(100.0, 200.0), 1010);

        // Move pointer1 beyond threshold
        let exceeded1 = state.on_move(Some(pointer1), Point::new(20.0, 30.0)); // ~14.14 distance
        assert_eq!(exceeded1, Some(42));

        // Move pointer2 within threshold
        let exceeded2 = state.on_move(Some(pointer2), Point::new(103.0, 203.0)); // ~4.24 distance
        assert!(exceeded2.is_none());

        // pointer1 should not generate click for different target
        let result1 = state.on_up(Some(pointer1), None, &99, Point::new(22.0, 32.0), 1050);
        assert_eq!(result1, ClickResult::Suppressed(Some(42)));

        // pointer2 should generate click for different target (release close to move position)
        let result2 = state.on_up(Some(pointer2), None, &99, Point::new(103.0, 203.0), 1080); // ~4.24 from down
        assert_eq!(result2, ClickResult::Click(42));
    }

    #[test]
    fn last_press_is_none_before_any_click() {
        let mut state: ClickState<u32> = ClickState::new();

        assert!(state.last_click().is_none());

        // A suppressed click shouldn't set it either.
        let result = state.on_up(None, None, &42, Point::new(0.0, 0.0), 0);
        assert_eq!(result, ClickResult::Suppressed(None));
        assert!(state.last_click().is_none());
    }

    #[test]
    fn last_press_updates_on_same_target_click() {
        let mut state: ClickState<u32> = ClickState::new();

        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result = state.on_up(None, None, &42, Point::new(12.0, 22.0), 1050);

        assert_eq!(result, ClickResult::Click(42));

        let last = state.last_click().expect("last_press should be set");
        assert_eq!(last.target, 42);
        assert_eq!(last.down_position, Point::new(10.0, 20.0));
        assert_eq!(last.down_time, 1000);
    }

    #[test]
    fn last_press_updates_only_on_clicks() {
        let mut state: ClickState<u32> = ClickState::with_thresholds(Some(5.0), Some(100));

        // First, a valid click.
        state.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
        let result1 = state.on_up(None, None, &42, Point::new(12.0, 22.0), 1050);
        assert_eq!(result1, ClickResult::Click(42));
        assert_eq!(state.last_click().unwrap().target, 42);

        // Next, a suppressed click (e.g. too far).
        state.on_down(None, None, 99, Point::new(0.0, 0.0), 2000);
        let result2 = state.on_up(None, None, &100, Point::new(100.0, 100.0), 2100);
        assert_eq!(result2, ClickResult::Suppressed(Some(99)));

        // last_press should still be the previous successful click (42).
        assert_eq!(state.last_click().unwrap().target, 42);
    }

    #[test]
    fn last_press_tracks_last_of_multiple_clicks() {
        let mut state: ClickState<u32> = ClickState::new();

        // First click on 1.
        state.on_down(None, None, 1, Point::new(0.0, 0.0), 0);
        state.on_up(None, None, &1, Point::new(0.0, 0.0), 10);
        assert_eq!(state.last_click().unwrap().target, 1);

        // Second click on 2.
        state.on_down(None, None, 2, Point::new(10.0, 10.0), 100);
        state.on_up(None, None, &2, Point::new(10.0, 10.0), 110);
        assert_eq!(state.last_click().unwrap().target, 2);
    }
}
