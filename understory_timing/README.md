# Understory Timing

Host-agnostic timer queue primitives for UI runtimes.

`understory_timing` owns timer identity, deadline ordering, cancellation,
expiration, and repeat policy calculation. It explicitly does not own clocks,
platform wakeups, event loops, widgets, rendering, or redraw policy.

The queue uses host-provided monotonic integer ticks. Most UI runtimes will use
nanoseconds, but the crate treats the values as opaque monotonic labels.

Hosts usually keep one queue next to retained UI or task state. Schedule a
timer when an owner asks for a delayed wakeup, use `next_deadline` to arm the
platform wakeup, and call `pop_expired` with the current monotonic time when
the host wakes. The expired record's target is the owner handle to notify.

Cancellation removes pending timers only. Once a timer has been returned by
`pop_expired`, it is no longer pending; hosts that batch expired timers before
dispatch may still deliver that already-drained record. Owners should compare
the delivered timer id with their current stored id or token and ignore stale
deliveries.

If a repeating timer should continue running after dispatch, pass it back to
`rearm`. To cancel an expired repeating timer, drop the expired record instead
of rearming it.

```rust
use understory_timing::TimerQueue;

let mut timers = TimerQueue::new();
let button = 42_u32;
let id = timers.schedule_once(button, 1_000, 250);

assert_eq!(timers.next_deadline(), Some(1_250));

let timer = timers.pop_expired(1_250).expect("timer is due");

assert_eq!(timer.id(), id);
assert_eq!(*timer.target(), button);
assert!(timers.is_empty());
```

The crate is always `#![no_std]` and uses `alloc`.
