<div align="center">

# Understory Timing

**Host-agnostic timer queue primitives**

[![Latest published version.](https://img.shields.io/crates/v/understory_timing.svg)](https://crates.io/crates/understory_timing)
[![Documentation build status.](https://img.shields.io/docsrs/understory_timing.svg)](https://docs.rs/understory_timing)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/forest-rs/understory/ci.yml?logo=github&label=CI)](https://github.com/forest-rs/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_timing --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Timing: host-agnostic timer queue primitives.

This crate provides a small, deterministic core for ordering timers by
monotonic deadlines. It is intended for UI toolkits, event loops, and other
host runtimes that want timer bookkeeping without taking on clocks, threads,
async reactors, callbacks, or platform wakeups.

The core concepts are:

- [`TimerInstant`] and [`TimerDuration`]: host-provided integer ticks. Most
  hosts use nanoseconds, but the queue treats them as opaque monotonic
  labels.
- [`TimerQueue`]: a deadline-ordered queue of pending timers.
- [`TimerId`]: the queue-assigned id used for queue-local cancellation and
  delivery recognition.
- [`TimerRepeat`]: the policy for calculating a repeating timer's next
  deadline.
- [`ExpiredTimer`]: an owned record returned to the host once a timer is due.

This crate deliberately does **not** know about wall-clock time, sleeping,
wakeup registration, async tasks, widgets, rendering, or redraw policy. Host
runtimes are responsible for:

- converting their clock into [`TimerInstant`] and [`TimerDuration`] values,
- calling [`TimerQueue::schedule`] / [`TimerQueue::schedule_once`] for
  relative delays, and [`TimerQueue::schedule_at`] /
  [`TimerQueue::schedule_once_at`] for host-computed absolute deadlines,
- using [`TimerQueue::next_deadline`] to arm the host wakeup mechanism,
- storing callback or action state in or alongside the timer target payload,
- calling [`TimerQueue::pop_expired`] with the current monotonic time when
  the host wakes,
- dispatching each owned expired record to the owner identified by
  [`ExpiredTimer::target`],
- calling [`TimerQueue::rearm`] after dispatch if a repeating timer should
  keep running,
- using [`TimerQueue::retain_pending`] when an owner is removed and all of
  its pending timers should be purged.

The queue is backed by a sorted [`alloc::collections::VecDeque`]. It favors a
small dependency-free core and explicit ordering rules over high-volume timer
scheduling machinery. It is a good fit for the modest timer counts common in
UI toolkits and small runtime loops; hosts with very large timer sets can
layer a heap or timing wheel above a different scheduling core.

## Invariants

- Pending timers are stored in deadline order.
- Timers with equal deadlines fire in scheduling order.
- Timer ids are stable for the timer record, including after expiration.
- Cancellation is idempotent, applies to pending timers, and reports whether
  a pending timer was removed.
- Expired timers are removed before they are returned to the host.
- Relative deadline arithmetic saturates at [`u64::MAX`].

## Cancellation and delivery

[`TimerQueue::cancel`] removes pending timers only. Once a timer has been
returned by [`TimerQueue::pop_expired`], it is no longer pending; hosts that
batch expired timers before dispatch may still deliver that already-drained
record. Owners should compare the delivered [`TimerId`] with their current
stored id or token and ignore stale deliveries.

Repeating timers make rearm explicit for the same reason. To cancel a
repeating timer that has already expired, drop the [`ExpiredTimer`] instead
of passing it to [`TimerQueue::rearm`].

## Target payloads

Timer targets are host-defined owner handles. Use ids such as element,
widget, task, connection, or request ids when possible. Expired timers own
their target handle so the queue is not borrowed while the host dispatches
the timer.

## Minimal example

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

## External timer tokens

Host runtimes that already expose their own timer token type can store that
token in the target payload. The queue's [`TimerId`] remains useful for
diagnostics or queue-local cancellation, while the host token remains the
value returned from higher-level APIs:

```rust
use understory_timing::TimerQueue;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct AppTimerToken(u64);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct TimerTarget {
    token: AppTimerToken,
    window: u32,
}

let mut timers = TimerQueue::new();
let delivered = AppTimerToken(7);
let cancelled = AppTimerToken(8);

timers.schedule_once(
    TimerTarget {
        token: delivered,
        window: 1,
    },
    1_000,
    250,
);
timers.schedule_once(
    TimerTarget {
        token: cancelled,
        window: 1,
    },
    1_000,
    500,
);

let removed = timers.retain_pending(|timer| timer.target().token != cancelled);
assert_eq!(removed, 1);

let expired = timers.pop_expired(1_250).expect("timer is due");
let target = expired.into_target();

assert_eq!(target.token, delivered);
assert_eq!(target.window, 1);
```

<!-- cargo-rdme end -->

[`alloc::collections::VecDeque`]: https://doc.rust-lang.org/alloc/collections/struct.VecDeque.html
[`ExpiredTimer`]: https://docs.rs/understory_timing/latest/understory_timing/struct.ExpiredTimer.html
[`ExpiredTimer::target`]: https://docs.rs/understory_timing/latest/understory_timing/struct.ExpiredTimer.html#method.target
[`TimerDuration`]: https://docs.rs/understory_timing/latest/understory_timing/type.TimerDuration.html
[`TimerId`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerId.html
[`TimerInstant`]: https://docs.rs/understory_timing/latest/understory_timing/type.TimerInstant.html
[`TimerQueue`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerQueue.html
[`TimerQueue::cancel`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerQueue.html#method.cancel
[`TimerQueue::next_deadline`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerQueue.html#method.next_deadline
[`TimerQueue::pop_expired`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerQueue.html#method.pop_expired
[`TimerQueue::rearm`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerQueue.html#method.rearm
[`TimerQueue::retain_pending`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerQueue.html#method.retain_pending
[`TimerQueue::schedule`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerQueue.html#method.schedule
[`TimerQueue::schedule_at`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerQueue.html#method.schedule_at
[`TimerQueue::schedule_once`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerQueue.html#method.schedule_once
[`TimerQueue::schedule_once_at`]: https://docs.rs/understory_timing/latest/understory_timing/struct.TimerQueue.html#method.schedule_once_at
[`TimerRepeat`]: https://docs.rs/understory_timing/latest/understory_timing/enum.TimerRepeat.html
[`u64::MAX`]: https://doc.rust-lang.org/core/primitive.u64.html#associatedconstant.MAX

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE] or <http://www.apache.org/licenses/LICENSE-2.0>), or
- MIT license ([LICENSE-MIT] or <http://opensource.org/licenses/MIT>),

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Contribution

Contributions are welcome by pull request. The [Rust code of conduct] applies.
Please feel free to add your name to the [AUTHORS] file in any substantive pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[LICENSE-APACHE]: https://github.com/forest-rs/understory/blob/main/LICENSE-APACHE
[LICENSE-MIT]: https://github.com/forest-rs/understory/blob/main/LICENSE-MIT
[Rust code of conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: https://github.com/forest-rs/understory/blob/main/AUTHORS
