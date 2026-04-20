// Copyright 2026 the Overstory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Simple retained color values for the Overstory first slice.

/// Packed RGBA color.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color(u32);

impl Color {
    /// Opaque black.
    pub const BLACK: Self = Self::rgba8(0, 0, 0, 255);
    /// Opaque white.
    pub const WHITE: Self = Self::rgba8(255, 255, 255, 255);
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self::rgba8(0, 0, 0, 0);

    /// Create a color from packed RGBA bytes.
    #[must_use]
    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a as u32)
    }

    /// Returns the packed color.
    #[must_use]
    pub const fn to_rgba8(self) -> u32 {
        self.0
    }

    /// Returns the red channel.
    #[must_use]
    pub fn r(self) -> u8 {
        self.0.to_be_bytes()[0]
    }

    /// Returns the green channel.
    #[must_use]
    pub fn g(self) -> u8 {
        self.0.to_be_bytes()[1]
    }

    /// Returns the blue channel.
    #[must_use]
    pub fn b(self) -> u8 {
        self.0.to_be_bytes()[2]
    }

    /// Returns the alpha channel.
    #[must_use]
    pub fn a(self) -> u8 {
        self.0.to_be_bytes()[3]
    }
}
