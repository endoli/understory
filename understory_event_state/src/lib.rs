// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_event_state --heading-base-level=0

//! Understory Event State: Common event state managers for UI interactions.
//!
//! This crate provides small, focused state machines for common UI interactions
//! that require stateful tracking across multiple events. Each module handles a
//! specific interaction pattern:
//!
//! - [`hover`]: Track enter/leave transitions as the pointer moves across UI elements
//! - [`focus`]: Manage keyboard focus state and focus transitions
//! - [`click`]: Transform-aware click recognition with spatial/temporal tolerance
//! - [`drag`]: Track drag operations with movement deltas and total offsets
//!
//! ## Design Philosophy
//!
//! Each state manager is designed to be:
//!
//! - **Minimal and focused**: Each handles one specific interaction pattern
//! - **Stateful but simple**: Track just enough state to compute transitions
//! - **Integration-friendly**: Work with any event routing or spatial query system
//! - **Generic**: Accept application-specific node/widget ID types
//!
//! The crate does not assume any particular UI framework, event system, or scene
//! graph structure. Instead, these managers accept pre-computed information (like
//! root→target paths from `understory_responder` or raw pointer positions) and
//! produce transition events or state queries that applications can interpret.
//!
//! ## Usage Patterns
//!
//! ### Hover Tracking
//!
//! Use [`hover::HoverState`] to compute enter/leave transitions when the pointer
//! moves between UI elements:
//!
//! ```rust
//! use understory_event_state::hover::{HoverState, HoverEvent};
//!
//! let mut hover = HoverState::new();
//!
//! // Pointer enters a nested element: [root, parent, child]
//! let events = hover.update_path(&[1, 2, 3]);
//! assert_eq!(events, vec![
//!     HoverEvent::Enter(1),
//!     HoverEvent::Enter(2),
//!     HoverEvent::Enter(3)
//! ]);
//!
//! // Pointer moves to sibling: [root, parent, sibling]
//! let events = hover.update_path(&[1, 2, 4]);
//! assert_eq!(events, vec![
//!     HoverEvent::Leave(3),   // Leave child
//!     HoverEvent::Enter(4)    // Enter sibling
//! ]);
//! ```
//!
//! ### Focus Management
//!
//! Use [`focus::FocusState`] to track keyboard focus transitions:
//!
//! ```rust
//! use understory_event_state::focus::{FocusState, FocusEvent};
//!
//! let mut focus = FocusState::new();
//!
//! // Focus moves to element 42 (with path from root)
//! let events = focus.update_path(&[1, 42]);
//! assert_eq!(events, vec![
//!     FocusEvent::Enter(1),
//!     FocusEvent::Enter(42)
//! ]);
//!
//! // Focus moves to different element 100 (different branch)
//! let events = focus.update_path(&[1, 100]);
//! assert_eq!(events, vec![
//!     FocusEvent::Leave(42),
//!     FocusEvent::Enter(100)
//! ]);
//! ```
//!
//! ### Transform-Aware Click Recognition
//!
//! Use [`click::ClickState`] to recognize clicks even when elements transform during interaction:
//!
//! ```rust
//! # #[cfg(feature = "click")]
//! # fn example() {
//! use kurbo::Point;
//! use understory_event_state::click::{ClickState, ClickResult};
//!
//! let mut clicks = ClickState::new();
//!
//! // Press down on element 42
//! clicks.on_down(None, None, 42, Point::new(10.0, 20.0), 1000);
//!
//! // Element transforms, pointer up occurs on different element but within tolerance
//! let result = clicks.on_up(None, None, &99, Point::new(13.0, 23.0), 1050);
//! assert_eq!(result, ClickResult::Click(42)); // Still generates click on original target
//! # }
//! ```
//!
//! ### Drag Operations
//!
//! Use [`drag::DragState`] to track pointer drag operations:
//!
//! ```rust
//! # #[cfg(feature = "drag")]
//! # fn example() {
//! use kurbo::Point;
//! use understory_event_state::drag::DragState;
//!
//! let mut drag = DragState::default();
//!
//! // Start drag at (10, 10)
//! drag.start(Point::new(10.0, 10.0));
//!
//! // Move pointer, get delta since last position
//! let delta = drag.update(Point::new(15.0, 12.0)).unwrap();
//! // delta is (5.0, 2.0)
//!
//! // Get total offset from start
//! let total = drag.total_offset(Point::new(15.0, 12.0)).unwrap();
//! // total is (5.0, 2.0)
//! # }
//! ```
//!
//! ## Integration with Understory
//!
//! These state managers integrate naturally with other Understory crates:
//!
//! - Use `understory_responder` to route events and produce root→target paths
//! - Feed those paths into [`hover::HoverState`] for enter/leave transitions
//! - Use `understory_box_tree` hit testing to determine click/drag targets
//! - Combine with `understory_selection` to handle selection interactions
//!
//! Each manager is designed to be a focused building block that handles one
//! interaction pattern well, allowing applications to compose them as needed
//! for their specific UI requirements.
//!
//! ## Features
//!
//! - `click`: Enable transform-aware click recognition (requires `kurbo` dependency)
//! - `drag`: Enable drag state tracking (requires `kurbo` dependency)
//!
//! This crate is `no_std` compatible (with `alloc`) for all modules.

#![no_std]

extern crate alloc;

#[cfg(feature = "click")]
pub mod click;

#[cfg(feature = "drag")]
pub mod drag;
pub mod focus;
pub mod hover;
