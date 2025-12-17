<div align="center">

# Understory Event State

**Common event state managers for UIs**

[![Latest published version.](https://img.shields.io/crates/v/understory_event_state.svg)](https://crates.io/crates/understory_event_state)
[![Documentation build status.](https://img.shields.io/docsrs/understory_event_state.svg)](https://docs.rs/understory_event_state)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license) \
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_event_state
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs may be evaluated here. -->

<!-- cargo-rdme start -->

Understory Event State: Common event state managers for UI interactions.

This crate provides small, focused state machines for common UI interactions that require stateful
tracking across multiple events. Each module handles a specific interaction pattern:

- [`hover`]: Track enter/leave transitions as the pointer moves across UI elements
- [`focus`]: Manage keyboard focus state and focus transitions
- [`click`]: Transform-aware click recognition with spatial/temporal tolerance
- [`drag`]: Track drag operations with movement deltas and total offsets

### Design Philosophy

Each state manager is designed to be:

- **Minimal and focused**: Each handles one specific interaction pattern
- **Stateful but simple**: Track just enough state to compute transitions
- **Integration-friendly**: Work with any event routing or spatial query system
- **Generic**: Accept application-specific node/widget ID types

The crate does not assume any particular UI framework, event system, or scene graph structure.
Instead, these managers accept pre-computed information (like root→target paths from
`understory_responder` or raw pointer positions) and produce transition events or state queries that
applications can interpret.

### Usage Patterns

#### Hover Tracking

Use [`hover::HoverState`] to compute enter/leave transitions when the pointer moves between UI
elements:

```rust
use understory_event_state::hover::{HoverState, HoverEvent};

let mut hover = HoverState::new();

// Pointer enters a nested element: [root, parent, child]
let events = hover.update_path(&[1, 2, 3]);
assert_eq!(events, vec![
    HoverEvent::Enter(1),
    HoverEvent::Enter(2),
    HoverEvent::Enter(3)
]);

// Pointer moves to sibling: [root, parent, sibling]
let events = hover.update_path(&[1, 2, 4]);
assert_eq!(events, vec![
    HoverEvent::Leave(3),   // Leave child
    HoverEvent::Enter(4)    // Enter sibling
]);
```

#### Focus Management

Use [`focus::FocusState`] to track which element has keyboard focus:

```rust
use understory_event_state::focus::{FocusState, FocusEvent};

let mut focus = FocusState::new();

// Focus an element
let event = focus.set_focus(Some(42));
assert_eq!(event, Some(FocusEvent::Gained(42)));

// Change focus to another element
let event = focus.set_focus(Some(100));
assert_eq!(event, Some(FocusEvent::Changed { lost: 42, gained: 100 }));
```

#### Transform-Aware Click Recognition

Use [`click::ClickState`] to recognize clicks even when elements transform during interaction:

```rust
use kurbo::Point;
use understory_event_state::click::{ClickState, ClickResult};

let mut clicks = ClickState::new();

// Press down on element 42
clicks.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);

// Element transforms, pointer up occurs on different element but within tolerance
let result = clicks.on_up(None, None, &99, Point::new(13.0, 23.0), 1050);
assert_eq!(result, ClickResult::Click(42)); // Still generates click on original target
```

#### Drag Operations

Use [`drag::DragState`] to track pointer drag operations:

```rust
use kurbo::Point;
use understory_event_state::drag::DragState;

let mut drag = DragState::default();

// Start drag at (10, 10)
drag.start(Point::new(10.0, 10.0));

// Move pointer, get delta since last position
let delta = drag.update(Point::new(15.0, 12.0)).unwrap();
// delta is (5.0, 2.0)

// Get total offset from start
let total = drag.total_offset(Point::new(15.0, 12.0)).unwrap();
// total is (5.0, 2.0)
```

### Integration with Understory

These state managers integrate naturally with other Understory crates:

- Use `understory_responder` to route events and produce root→target paths
- Feed those paths into [`hover::HoverState`] for enter/leave transitions
- Use `understory_box_tree` hit testing to determine click/drag targets
- Combine with `understory_selection` to handle selection interactions

Each manager is designed to be a focused building block that handles one interaction pattern well,
allowing applications to compose them as needed for their specific UI requirements.

### Features

- `click`: Enable transform-aware click recognition (requires `kurbo` dependency)
- `drag`: Enable drag state tracking (requires `kurbo` dependency)

This crate is `no_std` compatible (with `alloc`) for all modules.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This crate has been verified to compile with **Rust 1.88** and later.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE] or <http://www.apache.org/licenses/LICENSE-2.0>), or
- MIT license ([LICENSE-MIT] or <http://opensource.org/licenses/MIT>),

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

## Contribution

Contributions are welcome by pull request. The [Rust code of conduct] applies. Please feel free to
add your name to the [AUTHORS] file in any substantive pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be licensed as above, without any
additional terms or conditions.

[Rust Code of Conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: ../AUTHORS
[LICENSE-APACHE]: LICENSE-APACHE
[LICENSE-MIT]: LICENSE-MIT
