<!-- Instructions

This changelog follows the patterns described here: <https://keepachangelog.com/en/>.

Subheadings to categorize changes are `added, changed, deprecated, removed, fixed, security`.

-->

# Changelog

The latest published Understory Virtual List release is [0.1.1](#011-2026-05-17) which was released on 2026-05-17.
You can find its changes [documented below](#011-2026-05-17).

## [Unreleased]

### Added

- Added half-open range helpers for materialized and viewport ranges:
  `VisibleStrip::range`, `VirtualList::visible_range`, `VirtualList::viewport_strip`,
  and `VirtualList::viewport_range`. ([#169][] by [@waywardmonkeys][])
- Added `VirtualList::set_len` for resizable models, `ResizableExtentModel` for
  `TailAnchoredExtentModel`, and model query wrappers on `VirtualList` that do
  not invalidate its cached visible strip. ([#170][] by [@waywardmonkeys][])

## [0.1.1][] (2026-05-17)

This release has an [MSRV][] of 1.88.

### Added

- Added `VirtualList::restore_tail_anchor`, a convenience helper for applying a
  tail-anchor state captured before mutating the model. ([#165][] by [@waywardmonkeys][])
- Implemented `ResizableExtentModel` for `SparsePrefixSumExtentModel`. ([#166][] by [@waywardmonkeys][])

### Deprecated

- Deprecated `VirtualList::stick_to_tail_if_anchored`; it checks anchoring after
  the model has changed, which can miss append/update cases that were anchored
  before mutation. ([#165][] by [@waywardmonkeys][])

### Fixed

- Fixed empty visible-strip spacer metadata when the requested range is at or
  beyond the end of the content. ([#165][] by [@waywardmonkeys][])

## [0.1.0][] (2026-05-14)

This release has an [MSRV][] of 1.88.

This is the initial release.

[@waywardmonkeys]: https://github.com/waywardmonkeys

[#165]: https://github.com/forest-rs/understory/pull/165
[#166]: https://github.com/forest-rs/understory/pull/166
[#169]: https://github.com/forest-rs/understory/pull/169
[#170]: https://github.com/forest-rs/understory/pull/170

[Unreleased]: https://github.com/forest-rs/understory/compare/understory_virtual_list-v0.1.1...HEAD
[0.1.1]: https://github.com/forest-rs/understory/compare/understory_virtual_list-v0.1.0...understory_virtual_list-v0.1.1
[0.1.0]: https://github.com/forest-rs/understory/releases/tag/understory_virtual_list-v0.1.0

[MSRV]: README.md#minimum-supported-rust-version-msrv
