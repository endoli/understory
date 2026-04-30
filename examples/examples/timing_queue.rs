// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Timer queue basics.
//!
//! This example shows the host-side shape: schedule timers with host-provided
//! monotonic ticks, ask the queue for the next wakeup, pop expired timer
//! records, and dispatch them to retained owners.
//!
//! `ElementId` stands in for the retained owner handle a UI runtime already
//! has. The queue does not know what an element is; it only stores the handle so
//! the host can notify that owner when a timer expires.
//!
//! Run:
//! - `cargo run -p understory_examples --example timing_queue`

use core::num::NonZeroU64;

use understory_timing::{TimerQueue, TimerRepeat};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct ElementId(u32);

fn main() {
    let input = ElementId(1);
    let spinner = ElementId(2);
    let button = ElementId(3);

    let mut timers = TimerQueue::new();
    let blink = timers.schedule(
        input,
        1_000,
        500,
        TimerRepeat::coalescing(NonZeroU64::new(500).expect("interval is non-zero")),
    );
    let spin = timers.schedule_repeating(
        spinner,
        1_000,
        100,
        NonZeroU64::new(100).expect("interval is non-zero"),
    );
    let tooltip = timers.schedule_once(button, 1_000, 750);

    println!("next wakeup: {:?}", timers.next_deadline());

    for now in [1_100, 1_500, 1_750] {
        println!("== tick {now} ==");
        let mut fired = 0;
        while let Some(timer) = timers.pop_expired(now) {
            let kind = if timer.id() == blink {
                "cursor blink"
            } else if timer.id() == spin {
                "spinner"
            } else if timer.id() == tooltip {
                "tooltip"
            } else {
                "unknown"
            };
            println!(
                "  {kind} for {:?} fired at deadline {} next={:?}",
                timer.target(),
                timer.deadline(),
                timer.next_deadline()
            );
            fired += 1;

            if timer.should_rearm() {
                timers.rearm(timer);
            }
        }
        println!("  fired={fired} next={:?}", timers.next_deadline());
    }

    let removed = timers.retain_pending(|timer| *timer.target() != input);
    println!("removed timers for {:?}: {removed}", input);
}
