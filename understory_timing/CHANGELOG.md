<!-- Instructions

This changelog follows the patterns described here: <https://keepachangelog.com/en/>.

Subheadings to categorize changes are `added, changed, deprecated, removed, fixed, security`.

-->

# Changelog

The latest published Understory Timing release is [0.1.2](#012-2026-05-17) which was released on 2026-05-17.
You can find its changes [documented below](#012-2026-05-17).

## [Unreleased]

## [0.1.2][] (2026-05-17)

This release has an [MSRV][] of 1.88.

### Added

- Added `TimerQueue::schedule_once_at` for scheduling one-shot timers with host-computed absolute deadlines. ([#162][] by [@waywardmonkeys][])

## [0.1.1][] (2026-05-17)

This release has an [MSRV][] of 1.88.

### Added

- Added `TimerQueue::schedule_at` for scheduling timers with host-computed absolute deadlines. ([#158][] by [@waywardmonkeys][])

## [0.1.0][] (2026-05-14)

This release has an [MSRV][] of 1.88.

This is the initial release.

[@waywardmonkeys]: https://github.com/waywardmonkeys

[#158]: https://github.com/forest-rs/understory/pull/158
[#162]: https://github.com/forest-rs/understory/pull/162

[Unreleased]: https://github.com/forest-rs/understory/compare/understory_timing-v0.1.2...HEAD
[0.1.2]: https://github.com/forest-rs/understory/compare/understory_timing-v0.1.1...understory_timing-v0.1.2
[0.1.1]: https://github.com/forest-rs/understory/compare/understory_timing-v0.1.0...understory_timing-v0.1.1
[0.1.0]: https://github.com/forest-rs/understory/releases/tag/understory_timing-v0.1.0

[MSRV]: README.md#minimum-supported-rust-version-msrv
