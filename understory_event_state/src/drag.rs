// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Drag state helper: compute movement deltas and total offsets from position changes.
//!
//! ## Usage
//!
//! 1) Start a drag operation by calling [`DragState::start`] with the initial position.
//! 2) On each move event, call [`DragState::update`] to get the movement delta since the last update.
//! 3) Optionally call [`DragState::total_offset`] to get the cumulative offset from the start position.
//! 4) End the drag operation with [`DragState::end`] to reset state.
//!
//! ## Minimal example
//!
//! ```
//! use kurbo::Point;
//! use understory_event_state::drag::DragState;
//!
//! let mut drag = DragState::default();
//!
//! // Start dragging at (10, 20)
//! drag.start(Point::new(10.0, 20.0));
//! assert!(drag.is_dragging());
//!
//! // Move to (15, 25) - delta is (5, 5)
//! let delta = drag.update(Point::new(15.0, 25.0)).unwrap();
//! assert_eq!(delta.x, 5.0);
//! assert_eq!(delta.y, 5.0);
//!
//! // Total offset from start is also (5, 5)
//! let total = drag.total_offset(Point::new(15.0, 25.0)).unwrap();
//! assert_eq!(total.x, 5.0);
//! assert_eq!(total.y, 5.0);
//! ```

use kurbo::{Point, Vec2};

/// Tracks drag state for move event processing
#[derive(Debug, Clone, Default, Copy)]
pub struct DragState {
    /// Start position of the drag operation
    pub start_pos: Option<Point>,
    /// Last recorded pointer position during drag
    pub last_pos: Option<Point>,
}

impl DragState {
    /// Start tracking a new drag operation from the given position.
    pub fn start(&mut self, pos: Point) {
        self.start_pos = Some(pos);
        self.last_pos = Some(pos);
    }

    /// Update the drag state with a new position, returning the movement delta since last update.
    pub fn update(&mut self, pos: Point) -> Option<Vec2> {
        if self.start_pos.is_some() {
            if let Some(last_pos) = self.last_pos {
                let delta = pos - last_pos;
                self.last_pos = Some(pos);
                Some(delta)
            } else {
                self.last_pos = Some(pos);
                None
            }
        } else {
            None
        }
    }

    /// Get total offset from drag start position.
    pub fn total_offset(&self, current_pos: Point) -> Option<Vec2> {
        if self.start_pos.is_some() {
            self.start_pos.map(|start_pos| current_pos - start_pos)
        } else {
            None
        }
    }

    /// End the current drag operation and reset state.
    pub fn end(&mut self) {
        self.start_pos = None;
        self.last_pos = None;
    }

    /// Returns `true` while a drag operation is active
    pub fn is_dragging(&self) -> bool {
        self.start_pos.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_drag_state_is_not_dragging() {
        let drag = DragState::default();
        assert!(drag.start_pos.is_none());
        assert!(drag.start_pos.is_some() == drag.last_pos.is_some());
    }

    #[test]
    fn start_sets_dragging_state() {
        let mut drag = DragState::default();
        let start = Point::new(10.0, 20.0);

        drag.start(start);

        assert_eq!(drag.start_pos, Some(start));
        assert_eq!(drag.start_pos, drag.last_pos);
    }

    #[test]
    fn update_returns_delta_when_dragging() {
        let mut drag = DragState::default();
        drag.start(Point::new(10.0, 20.0));

        let new_pos = Point::new(15.0, 25.0);
        let delta = drag.update(new_pos);

        assert_eq!(delta, Some(Vec2::new(5.0, 5.0)));
        assert_eq!(drag.last_pos, Some(new_pos));
    }

    #[test]
    fn update_returns_none_when_not_dragging() {
        let mut drag = DragState::default();

        let delta = drag.update(Point::new(15.0, 25.0));

        assert_eq!(delta, None);
        assert!(drag.last_pos.is_none());
    }

    #[test]
    fn update_with_no_last_position_returns_none() {
        let mut drag = DragState {
            start_pos: Some(Point::new(10.0, 20.0)),
            last_pos: None,
        };

        let new_pos = Point::new(15.0, 25.0);
        let delta = drag.update(new_pos);

        assert_eq!(delta, None);
        assert_eq!(drag.last_pos, Some(new_pos));
    }

    #[test]
    fn multiple_updates_track_incremental_deltas() {
        let mut drag = DragState::default();
        drag.start(Point::new(0.0, 0.0));

        // First move
        let delta1 = drag.update(Point::new(5.0, 3.0));
        assert_eq!(delta1, Some(Vec2::new(5.0, 3.0)));

        // Second move from new position
        let delta2 = drag.update(Point::new(8.0, 7.0));
        assert_eq!(delta2, Some(Vec2::new(3.0, 4.0)));

        // Third move
        let delta3 = drag.update(Point::new(10.0, 10.0));
        assert_eq!(delta3, Some(Vec2::new(2.0, 3.0)));
    }

    #[test]
    fn total_offset_calculates_from_start() {
        let mut drag = DragState::default();
        let start = Point::new(10.0, 20.0);
        drag.start(start);

        // Move to intermediate position
        drag.update(Point::new(15.0, 25.0));

        // Total offset from start to current position
        let current = Point::new(20.0, 35.0);
        let total = drag.total_offset(current);

        assert_eq!(total, Some(Vec2::new(10.0, 15.0)));
    }

    #[test]
    fn total_offset_returns_none_when_not_dragging() {
        let drag = DragState::default();

        let total = drag.total_offset(Point::new(100.0, 200.0));

        assert_eq!(total, None);
    }

    #[test]
    fn total_offset_with_no_start_position_returns_none() {
        let drag = DragState {
            start_pos: None,
            last_pos: Some(Point::new(10.0, 20.0)),
        };

        let total = drag.total_offset(Point::new(15.0, 25.0));

        assert_eq!(total, None);
    }

    #[test]
    fn end_resets_drag_state() {
        let mut drag = DragState::default();
        drag.start(Point::new(10.0, 20.0));
        drag.update(Point::new(15.0, 25.0));

        drag.end();

        assert!(drag.start_pos.is_none());
        assert!(drag.last_pos.is_none());
    }

    #[test]
    fn end_on_fresh_state_is_safe() {
        let mut drag = DragState::default();

        drag.end();

        assert!(drag.start_pos.is_none());
        assert!(drag.start_pos.is_some() == drag.last_pos.is_some());
    }

    #[test]
    fn negative_movement_deltas() {
        let mut drag = DragState::default();
        drag.start(Point::new(100.0, 100.0));

        // Move to smaller coordinates
        let delta = drag.update(Point::new(90.0, 85.0));

        assert_eq!(delta, Some(Vec2::new(-10.0, -15.0)));
    }

    #[test]
    fn zero_movement_delta() {
        let mut drag = DragState::default();
        let start = Point::new(50.0, 50.0);
        drag.start(start);

        // Update to same position
        let delta = drag.update(start);

        assert_eq!(delta, Some(Vec2::new(0.0, 0.0)));
    }

    #[test]
    fn fractional_coordinates() {
        let mut drag = DragState::default();
        drag.start(Point::new(1.5, 2.7));

        let delta = drag.update(Point::new(3.2, 4.1));

        // Use approximate equality for floating point comparison
        let expected_delta = Vec2::new(1.7, 1.4);
        assert!((delta.unwrap().x - expected_delta.x).abs() < f64::EPSILON * 10.0);
        assert!((delta.unwrap().y - expected_delta.y).abs() < f64::EPSILON * 10.0);

        let total = drag.total_offset(Point::new(3.2, 4.1));
        assert!((total.unwrap().x - expected_delta.x).abs() < f64::EPSILON * 10.0);
        assert!((total.unwrap().y - expected_delta.y).abs() < f64::EPSILON * 10.0);
    }

    #[test]
    fn start_overwrites_previous_drag() {
        let mut drag = DragState::default();

        // First drag session
        drag.start(Point::new(0.0, 0.0));
        drag.update(Point::new(10.0, 10.0));

        // Start new drag session from different position
        let new_start = Point::new(50.0, 60.0);
        drag.start(new_start);

        assert_eq!(drag.start_pos, Some(new_start));
        assert_eq!(drag.start_pos, drag.last_pos);

        // Total offset should be from new start position
        let total = drag.total_offset(Point::new(55.0, 65.0));
        assert_eq!(total, Some(Vec2::new(5.0, 5.0)));
    }

    #[test]
    fn large_coordinate_values() {
        let mut drag = DragState::default();
        drag.start(Point::new(1000000.0, 2000000.0));

        let delta = drag.update(Point::new(1000001.0, 2000002.0));

        assert_eq!(delta, Some(Vec2::new(1.0, 2.0)));
    }
}
