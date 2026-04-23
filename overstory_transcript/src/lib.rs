// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![no_std]

//! Transcript surface integration for Overstory.
//!
//! This crate keeps transcript-specific row composition and sync policy out of
//! app examples while avoiding transcript semantics inside `overstory` itself.
//!
//! The main entry point is [`TranscriptViewController`], which binds a
//! [`understory_transcript::Transcript`] onto one Overstory `ScrollView`
//! element and keeps transcript rows in sync with append-order entry state.

extern crate alloc;

mod view;

pub use view::{
    TranscriptEntryRole, TranscriptRowIds, TranscriptRowPresentation, TranscriptViewController,
    TranscriptViewStyle,
};
