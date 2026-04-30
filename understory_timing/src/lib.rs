// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_timing --heading-base-level=0

#![no_std]

//! Understory Timing: host-agnostic timer queue primitives.
//!
//! This crate owns timer identity, ordering, cancellation, expiration, and
//! repeat policy calculation. It explicitly does **not** own clocks, platform
//! wakeups, event loops, widgets, rendering, or redraw policy.
//!
//! The queue uses host-provided monotonic integer ticks. Most UI runtimes will
//! use nanoseconds, but this crate treats the values as opaque monotonic labels.
//! Callers decide how to convert from their platform clock into
//! [`TimerInstant`] and [`TimerDuration`].
//!
//! A host runtime typically keeps one [`TimerQueue`] next to its retained UI or
//! task state:
//!
//! - call [`TimerQueue::schedule`] when a widget, controller, or task asks for a
//!   delayed wakeup,
//! - pass the returned [`TimerId`] back to that owner if it needs to cancel a
//!   pending timer or recognize the timer during later delivery,
//! - use [`TimerQueue::next_deadline`] to configure the platform/event-loop
//!   wakeup,
//! - call [`TimerQueue::pop_expired`] with the current monotonic time when the
//!   host wakes,
//! - dispatch each owned expired record to the owner identified by
//!   [`ExpiredTimer::target`]; because the record is owned, the host may also
//!   schedule or cancel timers during dispatch,
//! - call [`TimerQueue::rearm`] after dispatch if a repeating timer should keep
//!   running,
//! - use [`TimerQueue::retain_pending`] when an owner is removed and all of its
//!   pending timers should be purged.
//!
//! ## Invariants
//!
//! - Pending timers are stored in deadline order.
//! - Timers with equal deadlines fire in scheduling order.
//! - Timer ids are stable for the timer record, including after expiration.
//! - Cancellation is idempotent, applies to pending timers, and reports whether
//!   a pending timer was removed.
//! - Expired timers are removed before they are returned to the host.
//! - Deadline arithmetic saturates at [`u64::MAX`].
//!
//! ## Cancellation and delivery
//!
//! [`TimerQueue::cancel`] removes pending timers only. Once a timer has been
//! returned by [`TimerQueue::pop_expired`], it is no longer pending; hosts that
//! batch expired timers before dispatch may still deliver that already-drained
//! record. Owners should compare the delivered [`TimerId`] with their current
//! stored id or token and ignore stale deliveries.
//!
//! Repeating timers make rearm explicit for the same reason. To cancel a
//! repeating timer that has already expired, drop the [`ExpiredTimer`] instead
//! of passing it to [`TimerQueue::rearm`].
//!
//! ## Target payloads
//!
//! Timer targets are host-defined owner handles. Use ids such as element,
//! widget, or task ids when possible. Expired timers own their target handle so
//! the queue is not borrowed while the host dispatches the timer.
//!
//! The queue is backed by a sorted [`alloc::collections::VecDeque`] and is
//! intended for the modest timer counts typical of UI runtimes. It keeps the
//! dependency surface small and the ordering rules explicit.
//!
//! ## Minimal example
//!
//! ```rust
//! use understory_timing::TimerQueue;
//!
//! let mut timers = TimerQueue::new();
//! let button = 42_u32;
//! let id = timers.schedule_once(button, 1_000, 250);
//!
//! assert_eq!(timers.next_deadline(), Some(1_250));
//!
//! let timer = timers.pop_expired(1_250).expect("timer is due");
//!
//! assert_eq!(timer.id(), id);
//! assert_eq!(*timer.target(), button);
//! assert!(timers.is_empty());
//! ```

extern crate alloc;

use alloc::collections::VecDeque;
use core::num::NonZeroU64;

/// Host-provided monotonic timestamp.
///
/// The unit is chosen by the host. Nanoseconds are a common choice, but the
/// queue only requires that later times compare greater than earlier times.
pub type TimerInstant = u64;

/// Host-provided monotonic duration.
///
/// The unit must match [`TimerInstant`].
pub type TimerDuration = u64;

/// Identifier assigned to a scheduled timer.
///
/// A timer id can be used to cancel the timer while it is pending and to
/// recognize the timer after it expires. Host layers often wrap this in their
/// own domain-specific token type before exposing it to widgets or tasks.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimerId(u64);

impl TimerId {
    /// Creates a timer id from a raw integer.
    ///
    /// This is mainly useful for tests and host-side bookkeeping.
    /// [`TimerQueue`] assigns ids automatically when scheduling.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the raw integer id.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Repeat policy for a scheduled timer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TimerRepeat {
    /// Fire once and remove the timer.
    None,
    /// Rearm from the time passed to [`TimerQueue::pop_expired`].
    ///
    /// This coalesces missed intervals and is usually the right policy for UI
    /// animation, cursor blinking, and delayed widget actions.
    FromDrainTime(NonZeroU64),
    /// Rearm from the timer's previous scheduled deadline.
    ///
    /// This preserves cadence. If the host pops late, the next deadline may
    /// still be due immediately, allowing the host to catch up deliberately.
    FromScheduledDeadline(NonZeroU64),
}

impl TimerRepeat {
    /// Creates a non-repeating policy.
    #[must_use]
    pub const fn none() -> Self {
        Self::None
    }

    /// Creates a repeating policy that coalesces missed intervals.
    #[must_use]
    pub const fn coalescing(interval: NonZeroU64) -> Self {
        Self::FromDrainTime(interval)
    }

    /// Creates a repeating policy that preserves scheduled cadence.
    #[must_use]
    pub const fn catch_up(interval: NonZeroU64) -> Self {
        Self::FromScheduledDeadline(interval)
    }

    /// Returns the repeat interval, if this timer repeats.
    #[must_use]
    pub const fn interval(self) -> Option<NonZeroU64> {
        match self {
            Self::None => None,
            Self::FromDrainTime(interval) | Self::FromScheduledDeadline(interval) => Some(interval),
        }
    }

    /// Returns whether this policy repeats.
    #[must_use]
    pub const fn is_repeating(self) -> bool {
        self.interval().is_some()
    }

    fn rearmed_deadline(
        self,
        scheduled_deadline: TimerInstant,
        pop_time: TimerInstant,
    ) -> Option<TimerInstant> {
        match self {
            Self::None => None,
            Self::FromDrainTime(interval) => Some(pop_time.saturating_add(interval.get())),
            Self::FromScheduledDeadline(interval) => {
                Some(scheduled_deadline.saturating_add(interval.get()))
            }
        }
    }
}

/// One pending timer entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingTimer<Target> {
    id: TimerId,
    target: Target,
    deadline: TimerInstant,
    repeat: TimerRepeat,
}

impl<Target> PendingTimer<Target> {
    /// Returns the timer id.
    #[must_use]
    pub const fn id(&self) -> TimerId {
        self.id
    }

    /// Returns the host-owned target associated with this timer.
    #[must_use]
    pub const fn target(&self) -> &Target {
        &self.target
    }

    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> TimerInstant {
        self.deadline
    }

    /// Returns this timer's repeat policy.
    #[must_use]
    pub const fn repeat(&self) -> TimerRepeat {
        self.repeat
    }
}

/// An owned timer record that expired.
///
/// The timer has already been removed from its queue. Hosts may dispatch it,
/// mutate the queue, and then call [`TimerQueue::rearm`] if the timer should
/// keep repeating. Cancelling this timer after it has expired is represented by
/// dropping the expired record instead of rearming it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpiredTimer<Target> {
    id: TimerId,
    target: Target,
    deadline: TimerInstant,
    repeat: TimerRepeat,
    next_deadline: Option<TimerInstant>,
}

impl<Target> ExpiredTimer<Target> {
    /// Returns the timer id.
    #[must_use]
    pub const fn id(&self) -> TimerId {
        self.id
    }

    /// Returns the host-owned target associated with this timer.
    #[must_use]
    pub const fn target(&self) -> &Target {
        &self.target
    }

    /// Returns the host-owned target associated with this timer.
    #[must_use]
    pub fn into_target(self) -> Target {
        self.target
    }

    /// Returns the deadline that caused this timer to fire.
    #[must_use]
    pub const fn deadline(&self) -> TimerInstant {
        self.deadline
    }

    /// Returns this timer's repeat policy.
    #[must_use]
    pub const fn repeat(&self) -> TimerRepeat {
        self.repeat
    }

    /// Returns the next deadline if the timer can be rearmed.
    #[must_use]
    pub const fn next_deadline(&self) -> Option<TimerInstant> {
        self.next_deadline
    }

    /// Returns whether this timer has a repeat deadline.
    #[must_use]
    pub const fn should_rearm(&self) -> bool {
        self.next_deadline.is_some()
    }
}

/// Deadline-ordered queue of pending timers.
#[derive(Clone, Debug)]
pub struct TimerQueue<Target> {
    entries: VecDeque<PendingTimer<Target>>,
    next_id: u64,
}

impl<Target> Default for TimerQueue<Target> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Target> TimerQueue<Target> {
    /// Creates an empty timer queue.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            next_id: 0,
        }
    }

    /// Returns pending timers in firing order.
    ///
    /// This iterator is for inspection and host-side diagnostics. The queue
    /// owns the ordering; hosts usually only need [`TimerQueue::next_deadline`]
    /// and [`TimerQueue::pop_expired`] during normal operation.
    #[must_use]
    pub fn pending_timers(
        &self,
    ) -> impl ExactSizeIterator<Item = &PendingTimer<Target>> + DoubleEndedIterator + '_ {
        self.entries.iter()
    }

    /// Returns the number of pending timers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if no timers are pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Removes all pending timers.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Keeps only pending timers that satisfy `keep`.
    ///
    /// Returns the number of timers removed. This is useful when a host removes
    /// a widget, element, or task and wants to purge all timers that target it
    /// without storing every [`TimerId`] separately.
    pub fn retain_pending(&mut self, mut keep: impl FnMut(&PendingTimer<Target>) -> bool) -> usize {
        let old_len = self.entries.len();
        self.entries.retain(|entry| keep(entry));
        old_len - self.entries.len()
    }

    /// Returns the next deadline, or `None` when no timers are pending.
    #[must_use]
    pub fn next_deadline(&self) -> Option<TimerInstant> {
        self.entries.front().map(PendingTimer::deadline)
    }

    /// Schedules a timer with an explicit repeat policy.
    ///
    /// `now` is the current host-provided monotonic time and `delay` is the
    /// duration until first fire. The absolute deadline is computed with
    /// saturating arithmetic.
    pub fn schedule(
        &mut self,
        target: Target,
        now: TimerInstant,
        delay: TimerDuration,
        repeat: TimerRepeat,
    ) -> TimerId {
        let id = TimerId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("timer id space exhausted");
        let entry = PendingTimer {
            id,
            target,
            deadline: now.saturating_add(delay),
            repeat,
        };
        self.insert_entry(entry);
        id
    }

    /// Schedules a one-shot timer.
    pub fn schedule_once(
        &mut self,
        target: Target,
        now: TimerInstant,
        delay: TimerDuration,
    ) -> TimerId {
        self.schedule(target, now, delay, TimerRepeat::None)
    }

    /// Schedules a repeating timer that coalesces missed intervals.
    pub fn schedule_repeating(
        &mut self,
        target: Target,
        now: TimerInstant,
        delay: TimerDuration,
        interval: NonZeroU64,
    ) -> TimerId {
        self.schedule(target, now, delay, TimerRepeat::coalescing(interval))
    }

    /// Cancels a pending timer.
    ///
    /// Returns `true` when a timer was removed and `false` when the id was not
    /// pending. This includes timers that are unknown, already cancelled, or
    /// already returned by [`TimerQueue::pop_expired`].
    pub fn cancel(&mut self, id: TimerId) -> bool {
        self.retain_pending(|entry| entry.id != id) != 0
    }

    /// Removes and returns the next timer due at or before `now`.
    ///
    /// The returned timer is owned and no longer pending, so the host may
    /// freely mutate this queue while dispatching it. If the timer repeats and
    /// should continue running, call [`TimerQueue::rearm`] after dispatch.
    pub fn pop_expired(&mut self, now: TimerInstant) -> Option<ExpiredTimer<Target>> {
        if self.entries.front()?.deadline > now {
            return None;
        }

        let entry = self.entries.pop_front()?;
        Some(ExpiredTimer {
            id: entry.id,
            target: entry.target,
            deadline: entry.deadline,
            repeat: entry.repeat,
            next_deadline: entry.repeat.rearmed_deadline(entry.deadline, now),
        })
    }

    /// Rearms a repeating expired timer.
    ///
    /// Returns `true` when the timer was repeating and was reinserted. Returns
    /// `false` for one-shot timers. Rearming keeps the timer's existing
    /// [`TimerId`].
    ///
    /// To cancel an expired repeating timer, drop it instead of rearming it.
    ///
    /// With [`TimerRepeat::FromScheduledDeadline`], the next deadline may
    /// already be due. Hosts that want each repeating timer to fire at most once
    /// per platform wakeup should pop the current due set first, dispatch those
    /// records, and then rearm the repeating timers afterward.
    pub fn rearm(&mut self, timer: ExpiredTimer<Target>) -> bool {
        let Some(deadline) = timer.next_deadline else {
            return false;
        };

        self.insert_entry(PendingTimer {
            id: timer.id,
            target: timer.target,
            deadline,
            repeat: timer.repeat,
        });
        true
    }

    fn insert_entry(&mut self, entry: PendingTimer<Target>) {
        self.insert_entry_after(entry, 0);
    }

    fn insert_entry_after(&mut self, entry: PendingTimer<Target>, lower_bound: usize) {
        let lower_bound = lower_bound.min(self.entries.len());
        let pos = self.insertion_point_after(entry.deadline, lower_bound);
        self.entries.insert(pos, entry);
    }

    fn insertion_point_after(&self, deadline: TimerInstant, lower_bound: usize) -> usize {
        let mut low = lower_bound.min(self.entries.len());
        let mut high = self.entries.len();

        while low < high {
            let mid = low + (high - low) / 2;
            if self.entries[mid].deadline <= deadline {
                low = mid + 1;
            } else {
                high = mid;
            }
        }

        low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    const fn nz(value: u64) -> NonZeroU64 {
        match NonZeroU64::new(value) {
            Some(value) => value,
            None => panic!("test intervals are non-zero"),
        }
    }

    #[test]
    fn timers_fire_in_deadline_then_schedule_order() {
        let mut timers = TimerQueue::new();
        let late = timers.schedule_once("late", 10, 30);
        let early = timers.schedule_once("early", 10, 5);
        let same = timers.schedule_once("same", 10, 5);

        assert_eq!(timers.next_deadline(), Some(15));

        let mut fired = Vec::new();
        while let Some(timer) = timers.pop_expired(40) {
            fired.push(timer.id());
        }
        assert_eq!(fired, alloc::vec![early, same, late]);
        assert!(timers.is_empty());
    }

    #[test]
    fn cancel_is_idempotent_and_preserves_other_timers() {
        let mut timers = TimerQueue::new();
        let first = timers.schedule_once(1, 0, 10);
        let second = timers.schedule_once(2, 0, 20);

        assert!(timers.cancel(first));
        assert!(!timers.cancel(first));
        let pending = timers.pending_timers().collect::<Vec<_>>();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id(), second);
    }

    #[test]
    fn coalescing_repeat_rearms_from_drain_time() {
        let mut timers = TimerQueue::new();
        let id = timers.schedule("blink", 100, 10, TimerRepeat::coalescing(nz(50)));

        let timer = timers.pop_expired(200).expect("timer is due");
        assert_eq!(timer.id(), id);
        assert_eq!(timer.deadline(), 110);
        assert_eq!(timer.next_deadline(), Some(250));
        assert!(timers.rearm(timer));
        assert_eq!(timers.next_deadline(), Some(250));
    }

    #[test]
    fn catch_up_repeat_rearms_from_scheduled_deadline() {
        let mut timers = TimerQueue::new();
        let id = timers.schedule("cadence", 100, 10, TimerRepeat::catch_up(nz(50)));

        let timer = timers.pop_expired(200).expect("timer is due");
        assert_eq!(timer.id(), id);
        assert_eq!(timer.next_deadline(), Some(160));
        assert!(timers.rearm(timer));
        assert_eq!(timers.next_deadline(), Some(160));
    }

    #[test]
    fn one_shot_timer_does_not_rearm() {
        let mut timers = TimerQueue::new();
        timers.schedule_once("once", 0, 10);

        let timer = timers.pop_expired(10).expect("timer is due");
        assert_eq!(timer.next_deadline(), None);
        assert!(!timers.rearm(timer));
        assert!(timers.is_empty());
    }

    #[test]
    fn retain_pending_removes_matching_targets() {
        let mut timers = TimerQueue::new();
        timers.schedule_once("removed", 0, 10);
        let kept = timers.schedule_once("kept", 0, 20);
        timers.schedule_once("removed", 0, 30);

        let removed = timers.retain_pending(|timer| *timer.target() != "removed");

        assert_eq!(removed, 2);
        let pending = timers.pending_timers().collect::<Vec<_>>();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id(), kept);
    }

    #[test]
    fn expired_timers_can_be_dispatched_before_rearming() {
        let mut timers = TimerQueue::new();
        let repeating = timers.schedule("cadence", 100, 10, TimerRepeat::catch_up(nz(50)));
        let one_shot = timers.schedule_once("once", 100, 20);

        let mut expired = Vec::new();
        while let Some(timer) = timers.pop_expired(200) {
            expired.push(timer);
        }
        let fired = expired.iter().map(ExpiredTimer::id).collect::<Vec<_>>();

        assert_eq!(fired, alloc::vec![repeating, one_shot]);
        assert!(timers.is_empty());
        for timer in expired {
            timers.rearm(timer);
        }
        assert_eq!(timers.next_deadline(), Some(160));
    }

    #[test]
    fn cancel_removes_rearmed_timer_after_drain() {
        let mut timers = TimerQueue::new();
        let id = timers.schedule("blink", 100, 10, TimerRepeat::coalescing(nz(50)));

        let timer = timers.pop_expired(110).expect("timer is due");
        assert_eq!(timer.id(), id);
        assert!(timers.rearm(timer));
        assert_eq!(timers.next_deadline(), Some(160));
        assert!(timers.cancel(id));
        assert!(timers.is_empty());
    }

    #[test]
    fn catch_up_repeats_advance_one_period_per_rearm() {
        let mut timers = TimerQueue::new();
        let id = timers.schedule("cadence", 100, 10, TimerRepeat::catch_up(nz(50)));

        for expected_next in [160, 210] {
            let timer = timers.pop_expired(200).expect("timer is due");
            assert_eq!(timer.id(), id);
            assert_eq!(timer.next_deadline(), Some(expected_next));
            assert!(timers.rearm(timer));
        }

        assert_eq!(timers.next_deadline(), Some(210));
    }

    #[test]
    fn host_can_mutate_queue_while_dispatching_expired_timer() {
        let mut timers = TimerQueue::new();
        let first = timers.schedule_once("first", 0, 10);

        let timer = timers.pop_expired(10).expect("timer is due");
        assert_eq!(timer.id(), first);
        let follow_up = timers.schedule_once("follow-up", 10, 5);

        assert!(!timers.rearm(timer));
        assert_eq!(timers.pop_expired(14), None);
        assert_eq!(
            timers.pop_expired(15).map(|timer| timer.id()),
            Some(follow_up)
        );
    }

    #[test]
    fn non_clone_targets_can_expire() {
        #[derive(Debug, PartialEq, Eq)]
        struct NonClone(u32);

        let mut timers = TimerQueue::new();
        let id = timers.schedule_once(NonClone(7), 0, 10);

        let timer = timers.pop_expired(10).expect("timer is due");
        assert_eq!(timer.id(), id);
        assert_eq!(timer.into_target(), NonClone(7));
    }

    #[test]
    fn deadlines_saturate() {
        let mut timers = TimerQueue::new();
        timers.schedule_once("late", u64::MAX - 5, 10);

        assert_eq!(timers.next_deadline(), Some(u64::MAX));
    }
}
