// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Host-agnostic timer queue for animation and delayed actions.
//!
//! Overstory maintains pending timers internally. The host queries
//! [`TimerQueue::next_deadline`] to know when to wake, then calls
//! [`crate::Ui::tick`] to fire expired timers.

use alloc::vec::Vec;

use crate::ElementId;

/// Identifier for a pending timer.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TimerId(u32);

/// One pending timer entry.
#[derive(Clone, Debug)]
pub(crate) struct TimerEntry {
    pub id: TimerId,
    pub element_id: ElementId,
    /// Absolute deadline in host-provided monotonic nanoseconds.
    pub deadline: u64,
    /// Repeat interval in nanoseconds, or `None` for one-shot.
    pub repeat: Option<u64>,
}

/// Queue of pending timers, sorted by deadline.
#[derive(Clone, Debug, Default)]
pub struct TimerQueue {
    entries: Vec<TimerEntry>,
    next_id: u32,
}

impl TimerQueue {
    /// Creates an empty timer queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedules a timer. `now` is the current monotonic time in nanoseconds.
    /// `delay` is the delay from now in nanoseconds. If `repeat` is `Some`,
    /// the timer re-arms after firing.
    ///
    /// Returns a [`TimerId`] for cancellation.
    pub fn request(
        &mut self,
        element_id: ElementId,
        now: u64,
        delay: u64,
        repeat: Option<u64>,
    ) -> TimerId {
        let id = TimerId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        let entry = TimerEntry {
            id,
            element_id,
            deadline: now.saturating_add(delay),
            repeat,
        };
        // Insert sorted by deadline.
        let pos = self
            .entries
            .partition_point(|e| e.deadline <= entry.deadline);
        self.entries.insert(pos, entry);
        id
    }

    /// Cancels a pending timer. No-op if the timer already fired or was
    /// cancelled.
    pub fn cancel(&mut self, id: TimerId) {
        self.entries.retain(|e| e.id != id);
    }

    /// Returns the next deadline in nanoseconds, or `None` if no timers
    /// are pending.
    #[must_use]
    pub fn next_deadline(&self) -> Option<u64> {
        self.entries.first().map(|e| e.deadline)
    }

    /// Returns `true` if there are no pending timers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drains all timers whose deadline is <= `now`.
    ///
    /// Returns a list of (`TimerId`, `ElementId`) for each fired timer.
    /// Repeating timers are automatically re-armed.
    pub(crate) fn drain_expired(&mut self, now: u64) -> Vec<(TimerId, ElementId)> {
        let mut fired = Vec::new();
        let mut rearm = Vec::new();

        // Drain from the front (sorted by deadline).
        while let Some(entry) = self.entries.first() {
            if entry.deadline > now {
                break;
            }
            let entry = self.entries.remove(0);
            fired.push((entry.id, entry.element_id));
            if let Some(interval) = entry.repeat {
                rearm.push(TimerEntry {
                    id: entry.id,
                    element_id: entry.element_id,
                    deadline: now.saturating_add(interval),
                    repeat: entry.repeat,
                });
            }
        }

        // Re-insert repeating timers.
        for entry in rearm {
            let pos = self
                .entries
                .partition_point(|e| e.deadline <= entry.deadline);
            self.entries.insert(pos, entry);
        }

        fired
    }
}
