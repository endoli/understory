// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Identifier types used throughout the display list.

/// Identifier for an operation in a [`crate::DisplayList`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct OpId(pub u32);

/// Identifier for a stacking group.
///
/// Groups provide a way to associate ops into logical stacking contexts.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct GroupId(pub u32);

/// Identifier for a semantic region (for provenance and a11y mapping).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SemanticRegionId(pub u32);

/// Identifier for a glyph run in a font/text system.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct RunId(pub u32);

/// Identifier for an image resource.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u32);

/// Identifier for a path resource.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathId(pub u32);

/// Identifier for a paint resource (solid, gradient, pattern, etc.).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaintId(pub u32);

/// Identifier for a stroke style resource.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StrokeId(pub u32);

/// Identifier for a clip shape resource.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClipId(pub u32);
