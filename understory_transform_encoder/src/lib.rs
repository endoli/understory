// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transform encoder for mapping user input events to viewport transformations.
//!
//! This crate provides a **strongly typed, dimension-aware transform system** that maps user
//! input events (drag, scroll, pinch, rotate, keyboard) to viewport transformations with
//! compile-time guarantees and zero runtime failures.
//!
//! ## High-Level Usage: Transform Encoder
//!
//! The primary interface is [`TransformEncoder`], which manages behavior configuration and
//! drag state to process input events and apply transforms to mutable target references:
//!
//! ```rust,no_run
//! use understory_transform_encoder::{TransformEncoder, Behaviors};
//! # #[cfg(feature = "view2d_adapter")]
//! use understory_view2d::Viewport2D;
//! use ui_events::{keyboard::{Key, NamedKey}, pointer::PointerEvent};
//! use kurbo::{Rect, Vec2};
//!
//! # #[cfg(feature = "view2d_adapter")]
//! # {
//! // Create a 2D viewport as the transform target
//! let mut viewport = Viewport2D::new(Rect::new(0.0, 0.0, 800.0, 600.0));
//!
//! // Configure input behaviors
//! let behaviors = Behaviors::new()
//!     .drag(|m, _| m.is_empty(), |d| d.xy().pan(Vec2::new(1.0, 1.0)))
//!     .scroll_y(|m, _| m.is_empty(), |s| s.rotate(0.01))
//!     .pinch(|_, _| true, |p| p.uniform().scale(0.1));
//!
//! // Create encoder and process events
//! let mut encoder = TransformEncoder::new(behaviors);
//! // encoder.encode(&pointer_event, &mut viewport);  // Process pointer events
//! // encoder.encode_keyboard(&key_event, &mut viewport);  // Process keyboard events
//! # }
//! ```
//!
//! ## Behavior Configuration: Input Event Processing
//!
//! The [`Behaviors`] type configures how input events map to transforms using a fluent API:
//!
//! ```rust,no_run
//! use understory_transform_encoder::Behaviors;
//! use ui_events::keyboard::{Key, NamedKey};
//! use kurbo::Vec2;
//!
//! let behaviors = Behaviors::new()
//!     // Pointer input: drag without modifiers → exact pan (no sensitivity, absolute positioning)
//!     .drag_exact(|m, _| m.is_empty())
//!     
//!     // Pointer input: drag with Shift → pan with 1:1 sensitivity
//!     .drag(|m, _| m.shift(), |d| d.xy().pan(Vec2::new(1.0, 1.0)))
//!     
//!     // Pointer input: drag with Ctrl → extract X axis → rotate
//!     .drag(|m, _| m.ctrl(), |d| d.x().rotate(0.01))
//!     
//!     // Pointer input: vertical scroll → rotate
//!     .scroll_y(|m, _| m.is_empty(), |s| s.rotate(0.01))
//!     
//!     // Pointer input: horizontal scroll → uniform scale from 1D
//!     .scroll_x(|m, _| m.is_empty(), |s| s.uniform().scale(0.1))
//!     
//!     // Gesture input: pinch → uniform scale
//!     .pinch(|_, _| true, |p| p.uniform().scale(0.5))
//!     
//!     // Gesture input: rotate → rotate transform
//!     .rotate(|_, _| true, |r| r.rotate(1.0))
//!     
//!     // Keyboard input: arrow keys → pan with fixed deltas
//!     .key(Key::Named(NamedKey::ArrowLeft), |_, state| state.is_down(),
//!          |k| k.delta().pan(Vec2::new(-10.0, 0.0)))
//!     .key(Key::Named(NamedKey::ArrowRight), |_, state| state.is_down(),
//!          |k| k.delta().pan(Vec2::new(10.0, 0.0)))
//!     
//!     // Keyboard input: Enter/Escape → scale with fixed factors
//!     .key(Key::Named(NamedKey::Enter), |_, state| state.is_down(),
//!          |k| k.factor().scale_about_pointer(1.1))
//!     .key(Key::Named(NamedKey::Escape), |_, state| state.is_down(),
//!          |k| k.factor().scale_about_pointer(0.9));
//! ```
//!
//! ## Input-to-Output Mappings: Dimensional Transformations
//!
//! The system provides builders for mapping between input dimensions and output transform types:
//!
//! ### 2D Input Sources (Drag, Scroll)
//! - [`InputValue2DBuilder::xy()`] - Keep original axis order: `(x, y) → (x, y)`
//! - [`InputValue2DBuilder::yx()`] - Swap X and Y axes: `(x, y) → (y, x)`
//! - [`InputValue2DBuilder::x()`] - Extract X component only: `(x, y) → x`
//! - [`InputValue2DBuilder::y()`] - Extract Y component only: `(x, y) → y`
//! - [`InputValue2DBuilder::magnitude()`] - Extract vector length signed by dominant axis
//! - [`InputValue2DBuilder::dominant()`] - Extract whichever component has larger magnitude
//!
//! ### 1D Input Sources (Scroll axes, Pinch, Rotate gestures)
//! - [`InputValue1DBuilder::to_x()`] - Emit to X axis only: `value → (value, 0)`
//! - [`InputValue1DBuilder::to_y()`] - Emit to Y axis only: `value → (0, value)`
//! - [`InputValue1DBuilder::uniform()`] - Emit to both axes: `value → (value, value)`
//! - [`InputValue1DBuilder::rotate()`] - Direct 1D→1D rotation: `value → rotate(value)`
//!
//! ### All Supported Mapping Combinations
//!
//! | Input → Output | Examples |
//! |----------------|----------|
//! | **2D → 2D**    | Drag → Pan, Drag → Non-uniform Scale |
//! | **2D → 1D**    | Drag X → Rotate, Drag Magnitude → Rotate |
//! | **1D → 2D**    | Scroll → Uniform Scale, Pinch → Pan Both Axes |
//! | **1D → 1D**    | Rotate Gesture → Rotate Transform |
//!
//! ## Transform Actions: Low-Level Operations
//!
//! At the lowest level, the system uses [`TransformAction`] types to represent transform
//! operations before backend-specific application:
//!
//! - [`TransformAction::Pan`] - Translation operations with [`PanAction`] variants
//! - [`TransformAction::Scale`] - Scaling operations with [`ScaleAction`] variants  
//! - [`TransformAction::Rotate`] - Rotation operations with [`RotateAction`] variants
//!
//! Each action type supports multiple operation modes:
//! - **Fixed values**: Absolute positioning (`To` variants)
//! - **Relative deltas**: Additive changes (`By`, `DeltaBy` variants)
//! - **Multiplicative factors**: Scaling operations (`By` variants for scale/rotate)
//!
//! ## Transform Targets: Backend Implementations
//!
//! The [`TransformTarget`] trait allows different backends to handle transforms appropriately:
//!
//! - **[`Viewport1D`]** - 1D timeline/axis manipulation, maps 2D operations to 1D equivalents
//! - **[`Viewport2D`]** - 2D canvas/map interaction with uniform scaling  
//! - **[`kurbo::Affine`]** - Raw affine transforms supporting non-uniform scaling and rotation
//!
//! ### Direct Transform Usage
//!
//! ```rust
//! use understory_transform_encoder::{TransformAction, TransformTarget, PanAction, ScaleAction};
//! # #[cfg(feature = "view2d_adapter")]
//! use understory_view2d::Viewport2D;
//! use kurbo::{Rect, Vec2, Point};
//!
//! # #[cfg(feature = "view2d_adapter")]
//! # {
//! let mut viewport = Viewport2D::new(Rect::new(0.0, 0.0, 800.0, 600.0));
//!
//! // Apply transform actions directly
//! viewport.apply(TransformAction::Pan(PanAction::By(Vec2::new(10.0, 5.0))));
//! viewport.apply(TransformAction::Scale(ScaleAction::ByAbout {
//!     scale: Vec2::new(1.5, 1.5),
//!     anchor: Point::new(400.0, 300.0)
//! }));
//! # }
//! ```
//!
//! This crate is `no_std`.

#![no_std]

extern crate alloc;

use kurbo::{Affine, Point, Vec2};

use alloc::{rc::Rc, vec::Vec};
use ui_events::{
    keyboard::{Key, KeyState, KeyboardEvent, Modifiers},
    pointer::*,
};

/// Intent that describes a transform operation without specifying how to apply it.
///
/// Transform actions represent what the user wants to do (pan, scale, rotate) without
/// being tied to a specific backend. The structure maps to input types:
///
/// ## Input Type Mapping:
///
/// - **Fixed inputs (clicks, keyboard)**: Use concrete values (`PanAction::To`, `ScaleAction::To`, etc.)
/// - **Delta inputs (clicks, keyboard)**: Use additive operations (`PanAction::By`, `ScaleAction::DeltaBy`, `RotateAction::DeltaBy`)
/// - **Factor inputs (clicks, keyboard)**: Use multiplicative operations (`ScaleAction::By`, `RotateAction::By`)
/// - **Sensitivity inputs (pointer events)**: All operations use *`Action::By` variants
///
/// ## Valid Input Combinations:
///
/// - **Pan**: Fixed ✓, Delta ✓, Sensitivity ✓ | Factor ❌ (no multiplicative translation)
/// - **Scale**: Fixed ✓, Delta ✓, Factor ✓, Sensitivity ✓
/// - **Rotate**: Fixed ✓, Delta ✓, Factor ✓, Sensitivity ✓
#[derive(Debug, Clone, PartialEq)]
pub enum TransformAction {
    /// Translation operations
    Pan(PanAction),
    /// Scaling operations
    Scale(ScaleAction),
    /// Rotation operations
    Rotate(RotateAction),
}

/// Translation operations.
///
/// Note: Factor inputs should NOT be used with pan operations since multiplicative
/// translation doesn't make semantic sense.
#[derive(Debug, Clone, PartialEq)]
pub enum PanAction {
    /// Translate by offset (Fixed/Delta/Sensitivity → relative movement).
    /// - Fixed: Move by exact `Vec2` amount
    /// - Delta: Add to current movement
    /// - Sensitivity: Continuous pointer-based movement
    By(Vec2),
    /// Set absolute position (Fixed only → absolute positioning)
    /// Only makes sense with Fixed inputs for absolute positioning
    To(Vec2),
}

/// Scaling operations
///
/// **Backend Compatibility Note**: Scale operations behave differently across backends:
/// - **`Viewport2D`**: Only supports uniform scaling; non-uniform scales are averaged
/// - **`Affine`**: Supports full non-uniform scaling  
/// - **`Viewport1D`**: Uses dominant axis for 2D scale operations
#[derive(Debug, Clone, PartialEq)]
pub enum ScaleAction {
    /// Scale by multiplicative factors around backend's default center (Factor/Sensitivity → relative)
    /// - Factor: Multiply scale (scale *= factor)  
    /// - Sensitivity: Continuous pointer-based scaling
    By {
        /// Scale factors for X and Y axes
        scale: Vec2,
    },
    /// Scale by multiplicative factors around custom anchor (Factor/Sensitivity → relative)
    ByAbout {
        /// Scale factors for X and Y axes
        scale: Vec2,
        /// Fixed point to scale around in view coordinates
        anchor: Point,
    },
    /// Scale by additive deltas around backend's default center (Delta → relative)
    /// - Delta: Add to scale (scale += delta)
    DeltaBy {
        /// Scale deltas for X and Y axes
        scale: Vec2,
    },
    /// Scale by additive deltas around custom anchor (Delta → relative)
    DeltaByAbout {
        /// Scale deltas for X and Y axes
        scale: Vec2,
        /// Fixed point to scale around in view coordinates
        anchor: Point,
    },
    /// Set absolute scale around backend's default center (Fixed only → absolute)
    To {
        /// Absolute scale factors for X and Y axes (1.0 = normal size)
        scale: Vec2,
    },
    /// Set absolute scale around custom anchor (Fixed only → absolute)
    ToAbout {
        /// Absolute scale factors for X and Y axes (1.0 = normal size)
        scale: Vec2,
        /// Fixed point to scale around in view coordinates
        anchor: Point,
    },
}

/// Rotation operations
///
/// **Backend Compatibility Note**: Rotation support varies by backend:
/// - **`Viewport1D`**: No rotation support (operations are ignored)
/// - **`Viewport2D`**: No rotation support (operations are ignored)  
/// - **`Affine`**: Full rotation support around arbitrary anchor points
#[derive(Debug, Clone, PartialEq)]
pub enum RotateAction {
    /// Rotate by multiplicative factor around backend's default center (Factor/Sensitivity → relative)
    /// - Factor: Multiply rotation (rotation *= factor)
    /// - Sensitivity: Continuous pointer-based rotation
    By {
        /// Rotation angle in radians
        radians: f64,
    },
    /// Rotate by multiplicative factor around custom anchor (Factor/Sensitivity → relative)
    ByAbout {
        /// Rotation angle in radians
        radians: f64,
        /// Fixed point to rotate around in view coordinates
        anchor: Point,
    },
    /// Rotate by additive delta around backend's default center (Delta → relative)
    /// - Delta: Add to rotation (rotation += delta)
    DeltaBy {
        /// Rotation angle in radians
        radians: f64,
    },
    /// Rotate by additive delta around custom anchor (Delta → relative)
    DeltaByAbout {
        /// Rotation angle in radians
        radians: f64,
        /// Fixed point to rotate around in view coordinates
        anchor: Point,
    },
    /// Set absolute rotation around backend's default center (Fixed only → absolute)
    To {
        /// Absolute rotation angle in radians
        radians: f64,
    },
    /// Set absolute rotation around custom anchor (Fixed only → absolute)
    ToAbout {
        /// Absolute rotation angle in radians
        radians: f64,
        /// Fixed point to rotate around in view coordinates
        anchor: Point,
    },
}

/// Target that can apply transform deltas according to its own capabilities.
///
/// This trait allows different backends (`Viewport1D`, `Viewport2D`, Affine) to handle
/// the same transform intents in ways appropriate for their coordinate systems and
/// capabilities.
///
/// ## Backend-Specific Transform Interpretation
///
/// **Important**: The same [`TransformAction`] may behave differently depending on the target:
///
/// - **[`Viewport1D`]**: Maps 2D operations to 1D equivalents; uses dominant axis for scale factors
/// - **[`Viewport2D`]**: Supports uniform scaling only; uses dominant axis for scale factors. No rotation support
/// - **[`Affine`]**: Supports full non-uniform scaling, rotation, and complex transforms
///
/// **Recommendation**: Prefer uniform scaling when using targets that don't support non-uniform scaling.
///
/// For example, `ScaleAction::ByAbout { scale: Vec2::new(2.0, 1.0), anchor }`:
/// - **Viewport1D/2D**: Becomes uniform scale by factor `2.0` (dominant axis)
/// - **Affine**: Becomes non-uniform scale stretching X by 2x, Y unchanged
///
/// The trait also provides context-specific sizing information for proper scroll
/// delta resolution, allowing each backend to specify appropriate line and page
/// sizes for its coordinate system.
pub trait TransformTarget {
    /// Apply a transform delta to this target.
    fn apply(&mut self, delta: TransformAction);

    /// Get the line size for scroll line deltas in view coordinates.
    ///
    /// This is used to convert `ScrollDelta::LineDelta` into pixel amounts.
    /// A typical implementation might return the height of one line of text.
    fn line_size(&self) -> Vec2;

    /// Get the page size for scroll page deltas in view coordinates.
    ///
    /// This is used to convert `ScrollDelta::PageDelta` into pixel amounts.
    /// A typical implementation might return the visible viewport dimensions.
    fn page_size(&self) -> Vec2;

    /// Get the current translation/pan offset in view coordinates.
    ///
    /// This is used for absolute positioning during drag operations to avoid
    /// floating point error accumulation. Returns the current pan/translation
    /// that would be passed to `PanAction::To`.
    fn current_translation(&self) -> Vec2;
}

#[cfg(feature = "view2d_adapter")]
impl TransformTarget for understory_view2d::Viewport1D {
    /// Apply transform action to 1D viewport.
    ///
    /// **1D Viewport Transform Behavior**:
    /// - **Pan**: Uses magnitude of 2D vector or dominant axis component
    /// - **Scale**: Uses dominant scale factor from Vec2 components
    /// - **Rotate**: Ignored (1D viewports don't rotate)
    fn apply(&mut self, action: TransformAction) {
        match action {
            TransformAction::Pan(pan_action) => match pan_action {
                PanAction::By(v) => {
                    // For 1D viewport, use the magnitude of the pan vector
                    let pan_amount = (v.x.powi(2) + v.y.powi(2)).sqrt();
                    // Apply the sign from the larger component
                    let signed_amount = if v.x.abs() >= v.y.abs() {
                        pan_amount.copysign(v.x)
                    } else {
                        pan_amount.copysign(v.y)
                    };
                    self.pan_by_view(signed_amount);
                }
                PanAction::To(translation) => {
                    // For 1D viewport, use the dominant coordinate as new translation
                    let new_translation = if translation.x.abs() >= translation.y.abs() {
                        translation.x
                    } else {
                        translation.y
                    };
                    self.set_pan_by_view(new_translation);
                }
            },
            TransformAction::Scale(scale_action) => match scale_action {
                ScaleAction::By { scale } => {
                    // For 1D viewport, use the dominant scale factor
                    let factor = if scale.x.abs() >= scale.y.abs() {
                        scale.x
                    } else {
                        scale.y
                    };
                    let new_zoom = self.zoom() * factor;
                    self.set_zoom(new_zoom);
                }
                ScaleAction::ByAbout { scale, anchor } => {
                    // For 1D viewport, use the dominant scale factor and anchor coordinate
                    let factor = if scale.x.abs() >= scale.y.abs() {
                        scale.x
                    } else {
                        scale.y
                    };
                    let anchor_coord = if anchor.x.abs() >= anchor.y.abs() {
                        anchor.x
                    } else {
                        anchor.y
                    };
                    self.zoom_about_view_point(anchor_coord, factor);
                }
                ScaleAction::DeltaBy { scale } => {
                    // For 1D viewport, use the dominant scale delta and add to current zoom
                    let delta = if scale.x.abs() >= scale.y.abs() {
                        scale.x
                    } else {
                        scale.y
                    };
                    let new_zoom = self.zoom() + delta;
                    self.set_zoom(new_zoom);
                }
                ScaleAction::DeltaByAbout { scale, anchor } => {
                    // For 1D viewport, use the dominant scale delta and anchor coordinate
                    let delta = if scale.x.abs() >= scale.y.abs() {
                        scale.x
                    } else {
                        scale.y
                    };
                    let _anchor_coord = if anchor.x.abs() >= anchor.y.abs() {
                        anchor.x
                    } else {
                        anchor.y
                    };
                    // For additive scaling about a point, we need to adjust center to maintain anchor position
                    let _old_zoom = self.zoom();
                    let new_zoom = _old_zoom + delta;
                    self.set_zoom(new_zoom);
                    // TODO: Adjust center to maintain anchor position for additive scaling
                    // This would require translating the viewport so the anchor point remains fixed
                }
                ScaleAction::To { scale } => {
                    // For 1D viewport, use the dominant scale factor as absolute zoom
                    let zoom = if scale.x.abs() >= scale.y.abs() {
                        scale.x
                    } else {
                        scale.y
                    };
                    self.set_zoom(zoom);
                }
                ScaleAction::ToAbout { scale, anchor } => {
                    // For 1D viewport, set absolute scale about anchor point
                    let _zoom = if scale.x.abs() >= scale.y.abs() {
                        scale.x
                    } else {
                        scale.y
                    };
                    let _anchor_coord = if anchor.x.abs() >= anchor.y.abs() {
                        anchor.x
                    } else {
                        anchor.y
                    };
                    // Calculate what center should be to achieve desired scale about anchor
                    // TODO:!
                    // let old_center = self.center_world();
                    // self.set_zoom(zoom);
                    // let new_center = self.center_world();
                    // // Adjust center to maintain anchor position
                    // let offset = (anchor_coord - old_center) * (1.0 - zoom / self.zoom());
                    // self.set_center_world(new_center + offset);
                }
            },
            TransformAction::Rotate(_) => {
                // 1D doesn't rotate
            }
        }
    }

    fn line_size(&self) -> Vec2 {
        // TODO: Get actual line size from 1D viewport context
        // For now, use a reasonable default for 1D scrolling
        Vec2::new(20.0, 20.0)
    }

    fn page_size(&self) -> Vec2 {
        // For 1D viewport, use the view span as the page size
        let span_size = self.view_span().end - self.view_span().start;
        Vec2::new(span_size, span_size)
    }

    fn current_translation(&self) -> Vec2 {
        // TODO: Get actual pan offset from 1D viewport
        // For now, assume no translation or use available API
        Vec2::ZERO
    }
}

#[cfg(feature = "view2d_adapter")]
impl TransformTarget for understory_view2d::Viewport2D {
    /// Apply transform action to 2D viewport.
    ///
    /// **2D Viewport Transform Behavior**:
    /// - **Pan**: Full 2D translation support
    /// - **Scale**: Uniform scaling only - uses dominant axis from Vec2 components
    /// - **Rotate**: Ignored (2D viewports don't rotate)
    fn apply(&mut self, action: TransformAction) {
        match action {
            TransformAction::Pan(pan_action) => match pan_action {
                PanAction::By(delta) => {
                    self.pan_by_view(delta);
                }
                PanAction::To(translation) => {
                    self.set_pan_by_view(translation);
                }
            },
            TransformAction::Scale(scale_action) => match scale_action {
                ScaleAction::By { scale } => {
                    // For uniform zoom viewport, use dominant scale factor
                    let factor = if scale.x.abs() >= scale.y.abs() {
                        scale.x
                    } else {
                        scale.y
                    };
                    let new_zoom = self.zoom() * factor;
                    self.set_zoom(new_zoom);
                }
                ScaleAction::ByAbout { scale, anchor } => {
                    // For uniform zoom viewport, use dominant scale factor
                    let factor = if scale.x.abs() >= scale.y.abs() {
                        scale.x
                    } else {
                        scale.y
                    };
                    self.zoom_about_view_point(anchor, factor);
                }
                ScaleAction::DeltaBy { scale } => {
                    // For uniform zoom viewport, use dominant scale delta
                    let delta = if scale.x.abs() >= scale.y.abs() {
                        scale.x
                    } else {
                        scale.y
                    };
                    let new_zoom = self.zoom() + delta;
                    self.set_zoom(new_zoom);
                }
                ScaleAction::DeltaByAbout { scale, anchor: _ } => {
                    // For uniform zoom viewport, use dominant scale delta about anchor
                    let delta = if scale.x.abs() >= scale.y.abs() {
                        scale.x
                    } else {
                        scale.y
                    };
                    let _old_zoom = self.zoom();
                    let new_zoom = _old_zoom + delta;
                    self.set_zoom(new_zoom);
                    // TODO: Adjust center to maintain anchor position for additive scaling
                    // This would require translating the viewport so the anchor point remains fixed
                }
                ScaleAction::To { scale } => {
                    // For uniform zoom viewport, use average as absolute zoom
                    let zoom = (scale.x + scale.y) / 2.0;
                    self.set_zoom(zoom);
                }
                ScaleAction::ToAbout {
                    scale: _,
                    anchor: _,
                } => {
                    // For uniform zoom viewport, set absolute scale about anchor
                    // TODO:!
                    // let zoom = (scale.x + scale.y) / 2.0;
                    // let old_center = self.center_world();
                    // self.set_zoom(zoom);
                    // // Adjust center to maintain anchor position
                    // let scale_ratio = zoom / self.zoom();
                    // let offset = (anchor - old_center) * (1.0 - scale_ratio);
                    // self.set_center_world(old_center + offset.to_vec2());
                }
            },
            TransformAction::Rotate(_) => {
                // Viewport2D doesn't rotate
            }
        }
    }

    fn line_size(&self) -> Vec2 {
        // TODO: Get actual line size from 2D viewport context or font metrics
        // For now, use a reasonable default for text line height
        Vec2::new(20.0, 20.0)
    }

    fn page_size(&self) -> Vec2 {
        // For 2D viewport, use the view rect dimensions as the page size
        let view_rect = self.view_rect();
        Vec2::new(view_rect.width(), view_rect.height())
    }

    fn current_translation(&self) -> Vec2 {
        // TODO: Get actual pan offset from 2D viewport
        // For now, assume no translation or use available API
        Vec2::ZERO
    }
}

impl TransformTarget for Affine {
    /// Apply transform action to an `Affine`.
    ///
    /// **Affine Transform Behavior**:
    /// - **Pan**: Full 2D translation support
    /// - **Scale**: Full non-uniform scaling support - preserves separate X/Y scale factors
    /// - **Rotate**: Full rotation support around arbitrary anchor points
    fn apply(&mut self, action: TransformAction) {
        match action {
            TransformAction::Pan(pan_action) => match pan_action {
                PanAction::By(v) => {
                    *self = Self::translate(v) * *self;
                }
                PanAction::To(trans) => {
                    // Set translation component directly
                    *self = self.with_translation(trans);
                }
            },
            TransformAction::Scale(scale_action) => match scale_action {
                ScaleAction::By { scale } => {
                    // Non-uniform scale about origin (no anchor)
                    *self = Self::scale_non_uniform(scale.x, scale.y) * *self;
                }
                ScaleAction::ByAbout { scale, anchor } => {
                    // Non-uniform scale about anchor point
                    *self *= Self::translate(anchor.to_vec2())
                        * Self::scale_non_uniform(scale.x, scale.y)
                        * Self::translate(-anchor.to_vec2());
                }
                ScaleAction::DeltaBy { scale } => {
                    // Non-uniform additive scale about origin (no anchor)
                    // For additive scaling, we add to the current scale components
                    let current_scale = self.determinant().sqrt(); // Approximate current scale
                    let new_scale_x = current_scale + scale.x;
                    let new_scale_y = current_scale + scale.y;
                    *self = Self::scale_non_uniform(
                        new_scale_x / current_scale,
                        new_scale_y / current_scale,
                    ) * *self;
                }
                ScaleAction::DeltaByAbout { scale, anchor } => {
                    // Non-uniform additive scale about anchor point
                    let current_scale = self.determinant().sqrt(); // Approximate current scale
                    let new_scale_x = current_scale + scale.x;
                    let new_scale_y = current_scale + scale.y;
                    *self *= Self::translate(anchor.to_vec2())
                        * Self::scale_non_uniform(
                            new_scale_x / current_scale,
                            new_scale_y / current_scale,
                        )
                        * Self::translate(-anchor.to_vec2());
                }
                ScaleAction::To { scale: _ } => {
                    // Set scale component directly, preserving translation and rotation
                    // TODO:
                }
                ScaleAction::ToAbout {
                    scale: _,
                    anchor: _,
                } => {
                    // Set absolute scale about anchor point
                    // TODO:
                }
            },
            TransformAction::Rotate(rotate_action) => match rotate_action {
                RotateAction::By { radians } => {
                    // Rotate about origin (no anchor)
                    *self = Self::rotate(radians) * *self;
                }
                RotateAction::ByAbout { radians, anchor } => {
                    *self *= Self::rotate_about(radians, anchor);
                }
                RotateAction::DeltaBy { radians } => {
                    // Add rotation delta about origin (no anchor)
                    *self = Self::rotate(radians) * *self;
                }
                RotateAction::DeltaByAbout { radians, anchor } => {
                    // Add rotation delta about anchor point
                    *self *= Self::rotate_about(radians, anchor);
                }
                RotateAction::To { radians: _ } => {
                    // Set rotation component directly, preserving translation and scale
                    // TODO:
                }
                RotateAction::ToAbout {
                    radians: _,
                    anchor: _,
                } => {
                    // Set absolute rotation about anchor point
                    // TODO
                }
            },
        }
    }

    fn line_size(&self) -> Vec2 {
        // TODO: Get line size from context or provide through configuration
        // For affine transforms, we don't have inherent viewport context
        // Default to standard line height
        Vec2::new(20.0, 20.0)
    }

    fn page_size(&self) -> Vec2 {
        // TODO: Get page size from context or provide through configuration
        // For affine transforms, we don't have inherent viewport dimensions
        // Default to common viewport size
        Vec2::new(800.0, 600.0)
    }

    fn current_translation(&self) -> Vec2 {
        // Extract translation component from affine matrix
        self.translation()
    }
}

/// How to extract 1D from 2D input
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extract1D {
    /// Use the X component
    X,
    /// Use the Y component
    Y,
    /// Use the magnitude (sqrt(x² + y²)), signed by dominant axis
    Magnitude,
    /// Use the dominant axis (whichever magnitude is larger)
    Dominant,
    /// Use X if it's the dominant axis by magnitude, otherwise None
    XIfDominant,
    /// Use Y if it's the dominant axis by magnitude, otherwise None
    YIfDominant,
}

/// How to remap 2D to 2D
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Remap2D {
    /// Keep original order: (x, y)
    XY,
    /// Swap axes: (y, x)
    YX,
}

/// How to map a 1D value to 2D output axes
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Emit1Dto2D {
    /// Emit to X only: (v, 0)
    X,
    /// Emit to Y only: (0, v)
    Y,
    /// Emit to both: (v, v)
    Uniform,
}

/// Input value that can be either a fixed value or computed from a function
#[derive(Clone)]
pub enum ValueOrFn<T> {
    /// Fixed value provided directly
    Value(T),
    /// Function that computes the value when needed  
    Function(Rc<dyn Fn() -> T>),
}

/// Sensitivity value that can be either fixed or computed from input
#[derive(Clone)]
pub enum SensitivityOrFn<T, I> {
    /// Fixed sensitivity value
    Value(T),
    /// Function that computes sensitivity based on input
    Function(Rc<dyn Fn(I) -> T>),
}

impl<T: core::fmt::Debug, I> core::fmt::Debug for SensitivityOrFn<T, I> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Value(value) => f.debug_struct("Value").field("value", value).finish(),
            Self::Function(_) => f
                .debug_struct("Function")
                .field("value", &"<function>")
                .finish(),
        }
    }
}

impl<T, I> SensitivityOrFn<T, I> {
    /// Get the value, either directly or by calling the function with input.
    pub fn get(&self, input: I) -> T
    where
        T: Clone,
    {
        match self {
            Self::Value(value) => value.clone(),
            Self::Function(func) => func(input),
        }
    }
}

impl<T, I> From<T> for SensitivityOrFn<T, I> {
    fn from(value: T) -> Self {
        Self::Value(value)
    }
}

impl<T, I> From<Rc<dyn Fn(I) -> T>> for SensitivityOrFn<T, I> {
    fn from(func: Rc<dyn Fn(I) -> T>) -> Self {
        Self::Function(func)
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for ValueOrFn<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Value(value) => f.debug_struct("Value").field("value", value).finish(),
            Self::Function(_) => f
                .debug_struct("Function")
                .field("value", &"<function>")
                .finish(),
        }
    }
}

impl<T> ValueOrFn<T> {
    /// Get the value, either directly or by calling the function.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        match self {
            Self::Value(value) => value.clone(),
            Self::Function(func) => func(),
        }
    }
}

impl From<Vec2> for ValueOrFn<Vec2> {
    fn from(value: Vec2) -> Self {
        Self::Value(value)
    }
}
impl From<f64> for ValueOrFn<f64> {
    fn from(value: f64) -> Self {
        Self::Value(value)
    }
}

impl<T> From<Rc<dyn Fn() -> T>> for ValueOrFn<T> {
    fn from(func: Rc<dyn Fn() -> T>) -> Self {
        Self::Function(func)
    }
}

impl<T, F: Fn() -> T + 'static> From<F> for ValueOrFn<T> {
    fn from(func: F) -> Self {
        Self::Function(Rc::new(func))
    }
}

/// Sensitivity multiplier for continuous input (pointer events)
#[derive(Clone, Debug)]
pub struct Sensitivity(pub SensitivityOrFn<f64, f64>);

/// Fixed delta for discrete input (clicks, keyboard events) - absolute positioning/movement
#[derive(Clone, Debug)]
pub struct Fixed(pub ValueOrFn<f64>);

/// Delta for discrete input (clicks, keyboard events) - additive scaling
#[derive(Clone, Debug)]
pub struct Delta(pub ValueOrFn<f64>);

/// Factor for discrete input (clicks, keyboard events) - multiplicative scaling
#[derive(Clone, Debug)]
pub struct Factor(pub ValueOrFn<f64>);

/// Sensitivity multiplier for continuous 2D input (pointer events)
#[derive(Clone, Debug)]
pub struct Sensitivity2D(pub SensitivityOrFn<Vec2, Vec2>);

/// Fixed delta for discrete 2D input (clicks, keyboard events) - absolute positioning/movement
#[derive(Clone, Debug)]
pub struct Fixed2D(pub ValueOrFn<Vec2>);

/// Delta for discrete 2D input (clicks, keyboard events) - additive scaling
#[derive(Clone, Debug)]
pub struct Delta2D(pub ValueOrFn<Vec2>);

/// Factor for discrete 2D input (clicks, keyboard events) - multiplicative scaling
#[derive(Clone, Debug)]
pub struct Factor2D(pub ValueOrFn<Vec2>);

/// 1D transform action parameterized by value type (sensitivity or fixed)
#[derive(Clone, Debug)]
pub enum TransformAction1D<V> {
    /// Rotate operation with value parameter (uses backend's default center)
    Rotate {
        /// Value parameter (sensitivity for pointer input, fixed radians for keyboard)
        value: V,
    },
    /// Rotate operation with value parameter around a custom anchor point
    RotateAbout {
        /// Value parameter (sensitivity for pointer input, fixed radians for keyboard)
        value: V,
        /// Anchor point in view coordinates
        anchor: ValueOrFn<Point>,
    },
    /// Rotate operation with value parameter around the current pointer location
    RotateAboutPointer {
        /// Value parameter (sensitivity for pointer input, fixed radians for keyboard)
        value: V,
    },
}

/// 2D transform action parameterized by value type (sensitivity or fixed)
#[derive(Clone, Debug)]
pub enum TransformAction2D<V> {
    /// Pan operation with value parameter (This should not be used with a factor value)
    Pan {
        /// Value parameter (sensitivity for pointer input, fixed delta for keyboard)
        value: V,
    },
    /// Scale operation with value parameter (uses backend's default center)
    Scale {
        /// Value parameter (sensitivity for pointer input, fixed factor for keyboard)
        value: V,
    },
    /// Scale operation with value parameter around a custom anchor point
    ScaleAbout {
        /// Value parameter (sensitivity for pointer input, fixed factor for keyboard)
        value: V,
        /// Anchor point in view coordinates
        anchor: ValueOrFn<Point>,
    },
    /// Scale operation with value parameter around the current pointer location
    ScaleAboutPointer {
        /// Value parameter (sensitivity for pointer input, fixed factor for keyboard)
        value: V,
    },
}

// Type aliases for common action types

/// 1D action for pointer input (multiplies input by sensitivity)
pub type InputValueAction1D = TransformAction1D<Sensitivity>;
/// 1D action for keyboard input (uses fixed value directly - absolute movement/positioning)
pub type FixedAction1D = TransformAction1D<Fixed>;
/// 1D action for keyboard input (uses delta - additive scaling)  
pub type DeltaAction1D = TransformAction1D<Delta>;
/// 1D action for keyboard input (uses factor - multiplicative scaling)
pub type FactorAction1D = TransformAction1D<Factor>;
/// 2D action for pointer input (multiplies input by sensitivity)
pub type InputValueAction2D = TransformAction2D<Sensitivity2D>;
/// 2D action for keyboard input (uses fixed value directly - absolute movement/positioning)
pub type FixedAction2D = TransformAction2D<Fixed2D>;
/// 2D action for keyboard input (uses delta - additive scaling)  
pub type DeltaAction2D = TransformAction2D<Delta2D>;
/// 2D action for keyboard input (uses factor - multiplicative scaling)  
pub type FactorAction2D = TransformAction2D<Factor2D>;

/// 1D input → 2D output (pinch → uniform scale, scroll axis → pan both)
#[derive(Clone, Debug)]
pub struct Map1Dto2D {
    /// How to emit the 1D input to 2D axes
    pub emit: Emit1Dto2D,
    /// The 2D transform action to apply
    pub action: InputValueAction2D,
}

/// 2D input → 1D output (drag → rotate)
#[derive(Clone, Debug)]
pub struct Map2Dto1D {
    /// How to extract 1D value from 2D input
    pub extract: Extract1D,
    /// The 1D transform action to apply
    pub action: InputValueAction1D,
}

/// 2D input → 2D output (drag → pan)
#[derive(Clone, Debug)]
pub struct Map2Dto2D {
    /// How to remap the 2D input axes
    pub remap: Remap2D,
    /// The 2D transform action to apply
    pub action: InputValueAction2D,
}

/// Actions triggered by 1D input (pinch, rotate gesture)
#[derive(Clone, Debug)]
pub enum InputValue1DToTransform {
    /// Map 1D input to 1D output (e.g., rotate gesture → rotate transform)
    Action1D(InputValueAction1D),
    /// Map 1D input to 2D output (e.g., pinch → uniform scale)
    To2D(Map1Dto2D),
}

/// Actions triggered by 2D input (drag, scroll)
#[derive(Clone, Debug)]
pub enum InputValue2DToTransform {
    /// Map 2D input to 1D output (e.g., drag X axis → rotate)
    To1D(Map2Dto1D),
    /// Map 2D input to 2D output (e.g., drag → pan)
    To2D(Map2Dto2D),
}

impl Extract1D {
    /// Extract a 1D value from a 2D input according to the extraction method.
    pub fn extract(self, input: Vec2) -> Option<f64> {
        let v = input;
        match self {
            Self::X => Some(v.x),
            Self::Y => Some(v.y),
            Self::Magnitude => {
                let mag = v.length();
                let signed = if v.x.abs() >= v.y.abs() {
                    mag.copysign(v.x)
                } else {
                    mag.copysign(v.y)
                };
                Some(signed)
            }
            Self::Dominant => {
                if v.x.abs() >= v.y.abs() {
                    Some(v.x)
                } else {
                    Some(v.y)
                }
            }
            Self::XIfDominant => {
                if v.x.abs() >= v.y.abs() {
                    Some(v.x)
                } else {
                    None
                }
            }
            Self::YIfDominant => {
                if v.y.abs() > v.x.abs() {
                    Some(v.y)
                } else {
                    None
                }
            }
        }
    }
}

impl Remap2D {
    /// Remap a 2D input according to the remapping method.
    pub fn remap(self, input: Vec2) -> Vec2 {
        match self {
            Self::XY => input,
            Self::YX => Vec2::new(input.y, input.x),
        }
    }
}

impl Emit1Dto2D {
    /// Emit a 1D value to 2D output according to the emission method.
    pub fn emit(self, input: f64) -> Vec2 {
        let v = input;
        match self {
            Self::X => Vec2::new(v, 0.0),
            Self::Y => Vec2::new(0.0, v),
            Self::Uniform => Vec2::new(v, v),
        }
    }
}

// Sensitivity-based (pointer) actions take input and multiply by sensitivity
impl TransformAction1D<Sensitivity> {
    /// Convert action with 1D input to `TransformAction`.
    pub fn resolve(&self, input: f64, pointer_anchor: Point) -> TransformAction {
        match self {
            Self::Rotate { value } => TransformAction::Rotate(RotateAction::By {
                radians: input * value.0.get(input),
            }),
            Self::RotateAbout { value, anchor } => TransformAction::Rotate(RotateAction::ByAbout {
                radians: input * value.0.get(input),
                anchor: anchor.get(),
            }),
            Self::RotateAboutPointer { value } => TransformAction::Rotate(RotateAction::ByAbout {
                radians: input * value.0.get(input),
                anchor: pointer_anchor,
            }),
        }
    }
}

// Fixed-value (keyboard) actions use the fixed value directly, no input needed
impl TransformAction1D<Fixed> {
    /// Convert action to `TransformAction` using fixed value.
    pub fn resolve(&self, anchor_point: Point) -> TransformAction {
        match self {
            Self::Rotate { value } => TransformAction::Rotate(RotateAction::By {
                radians: value.0.get(),
            }),
            Self::RotateAbout { value, anchor } => TransformAction::Rotate(RotateAction::ByAbout {
                radians: value.0.get(),
                anchor: anchor.get(),
            }),
            Self::RotateAboutPointer { value } => TransformAction::Rotate(RotateAction::ByAbout {
                radians: value.0.get(),
                anchor: anchor_point,
            }),
        }
    }
}

// Factor-based (keyboard) actions use the factor value directly, no input needed
impl TransformAction1D<Factor> {
    /// Convert action to `TransformAction` using factor value.
    pub fn resolve(&self, anchor_point: Point) -> TransformAction {
        match self {
            Self::Rotate { value } => TransformAction::Rotate(RotateAction::By {
                radians: value.0.get(),
            }),
            Self::RotateAbout { value, anchor } => TransformAction::Rotate(RotateAction::ByAbout {
                radians: value.0.get(),
                anchor: anchor.get(),
            }),
            Self::RotateAboutPointer { value } => TransformAction::Rotate(RotateAction::ByAbout {
                radians: value.0.get(),
                anchor: anchor_point,
            }),
        }
    }
}

// Delta-based (keyboard) actions use the delta value directly, no input needed
impl TransformAction1D<Delta> {
    /// Convert action to `TransformAction` using delta value.
    pub fn resolve(&self, anchor_point: Point) -> TransformAction {
        match self {
            Self::Rotate { value } => TransformAction::Rotate(RotateAction::DeltaBy {
                radians: value.0.get(),
            }),
            Self::RotateAbout { value, anchor } => {
                TransformAction::Rotate(RotateAction::DeltaByAbout {
                    radians: value.0.get(),
                    anchor: anchor.get(),
                })
            }
            Self::RotateAboutPointer { value } => {
                TransformAction::Rotate(RotateAction::DeltaByAbout {
                    radians: value.0.get(),
                    anchor: anchor_point,
                })
            }
        }
    }
}

// Sensitivity-based (pointer) actions take input and multiply by sensitivity
impl TransformAction2D<Sensitivity2D> {
    /// Convert action with 2D input to `TransformAction`.
    pub fn resolve(&self, input: Vec2, pointer_anchor: Point) -> TransformAction {
        match self {
            Self::Pan { value } => {
                let sens = value.0.get(input);
                TransformAction::Pan(PanAction::By(Vec2::new(input.x * sens.x, input.y * sens.y)))
            }
            Self::Scale { value } => {
                let sens = value.0.get(input);
                let fx = 1.0 + input.x * sens.x;
                let fy = 1.0 + input.y * sens.y;
                TransformAction::Scale(ScaleAction::By {
                    scale: Vec2::new(fx, fy),
                })
            }
            Self::ScaleAbout { value, anchor } => {
                let sens = value.0.get(input);
                let fx = 1.0 + input.x * sens.x;
                let fy = 1.0 + input.y * sens.y;
                TransformAction::Scale(ScaleAction::ByAbout {
                    scale: Vec2::new(fx, fy),
                    anchor: anchor.get(),
                })
            }
            Self::ScaleAboutPointer { value } => {
                let sens = value.0.get(input);
                let fx = 1.0 + input.x * sens.x;
                let fy = 1.0 + input.y * sens.y;
                TransformAction::Scale(ScaleAction::ByAbout {
                    scale: Vec2::new(fx, fy),
                    anchor: pointer_anchor,
                })
            }
        }
    }
}

// Fixed-value (keyboard) actions use the fixed value directly, no input needed
impl TransformAction2D<Fixed2D> {
    /// Convert action to `TransformAction` using fixed value.
    pub fn resolve(&self, anchor_point: Point) -> TransformAction {
        match self {
            Self::Pan { value } => TransformAction::Pan(PanAction::By(value.0.get())),
            Self::Scale { value } => TransformAction::Scale(ScaleAction::By {
                scale: value.0.get(),
            }),
            Self::ScaleAbout { value, anchor } => TransformAction::Scale(ScaleAction::ByAbout {
                scale: value.0.get(),
                anchor: anchor.get(),
            }),
            Self::ScaleAboutPointer { value } => TransformAction::Scale(ScaleAction::ByAbout {
                scale: value.0.get(),
                anchor: anchor_point,
            }),
        }
    }
}

// Factor-based (keyboard) actions use the factor value directly, no input needed
impl TransformAction2D<Factor2D> {
    /// Convert action to `TransformAction` using factor value.
    pub fn resolve(&self, anchor_point: Point) -> TransformAction {
        match self {
            Self::Pan { value } => {
                // Factor doesn't make sense for pan, treat as fixed delta
                TransformAction::Pan(PanAction::By(value.0.get()))
            }
            Self::Scale { value } => TransformAction::Scale(ScaleAction::By {
                scale: value.0.get(),
            }),
            Self::ScaleAbout { value, anchor } => TransformAction::Scale(ScaleAction::ByAbout {
                scale: value.0.get(),
                anchor: anchor.get(),
            }),
            Self::ScaleAboutPointer { value } => TransformAction::Scale(ScaleAction::ByAbout {
                scale: value.0.get(),
                anchor: anchor_point,
            }),
        }
    }
}

impl TransformAction2D<Delta2D> {
    /// Convert action to `TransformAction` using delta value.
    pub fn resolve(&self, anchor_point: Point) -> TransformAction {
        match self {
            Self::Pan { value } => {
                // Delta for pan works as additive movement
                TransformAction::Pan(PanAction::By(value.0.get()))
            }
            Self::Scale { value } => TransformAction::Scale(ScaleAction::DeltaBy {
                scale: value.0.get(),
            }),
            Self::ScaleAbout { value, anchor } => {
                TransformAction::Scale(ScaleAction::DeltaByAbout {
                    scale: value.0.get(),
                    anchor: anchor.get(),
                })
            }
            Self::ScaleAboutPointer { value } => {
                TransformAction::Scale(ScaleAction::DeltaByAbout {
                    scale: value.0.get(),
                    anchor: anchor_point,
                })
            }
        }
    }
}

impl InputValue1DToTransform {
    /// Convert 1D action to `TransformAction`.
    pub fn resolve(&self, input: f64, pointer_anchor: Point) -> TransformAction {
        match self {
            Self::Action1D(action) => action.resolve(input, pointer_anchor),
            Self::To2D(map) => {
                let input_2d = map.emit.emit(input);
                map.action.resolve(input_2d, pointer_anchor)
            }
        }
    }
}

impl InputValue2DToTransform {
    /// Convert 2D action to `TransformAction`.
    pub fn resolve(&self, input: Vec2, pointer_anchor: Point) -> Option<TransformAction> {
        match self {
            Self::To1D(map) => {
                let input_1d = map.extract.extract(input)?;
                Some(map.action.resolve(input_1d, pointer_anchor))
            }
            Self::To2D(map) => {
                let remapped = map.remap.remap(input);
                Some(map.action.resolve(remapped, pointer_anchor))
            }
        }
    }
}

/// Filter function for user actions that handles both modifier keys and pointer device type
pub type PointerFilter = dyn Fn(Modifiers, PointerType) -> bool;

/// Filter function for keyboard actions that handles modifier keys and key state
pub type UserKeyboardFilter = dyn Fn(Modifiers, KeyState) -> bool;

/// Action types that don't require input values (keyboard actions)
#[derive(Clone, Debug)]
pub enum NoInputAction {
    /// Fixed value action (absolute movement/positioning)
    Fixed2D(FixedAction2D),
    /// Delta action (delta scaling)
    Delta2D(DeltaAction2D),
    /// Factor action (multiplicative scaling)
    Factor2D(FactorAction2D),
    /// Fixed 1D action
    Fixed1D(FixedAction1D),
    /// Delta 1D action  
    Delta1D(DeltaAction1D),
    /// Factor 1D action
    Factor1D(FactorAction1D),
}

impl NoInputAction {
    /// Convert no-input action to transform delta.
    pub fn resolve(&self, anchor_point: Point) -> TransformAction {
        match self {
            Self::Fixed2D(action) => action.resolve(anchor_point),
            Self::Factor2D(action) => action.resolve(anchor_point),
            Self::Delta2D(action) => action.resolve(anchor_point),
            Self::Fixed1D(action) => action.resolve(anchor_point),
            Self::Factor1D(action) => action.resolve(anchor_point),
            Self::Delta1D(action) => action.resolve(anchor_point),
        }
    }
}

/// Keyboard action that can trigger transform operations.
///
/// Keyboard actions represent key presses with filtering conditions and
/// associated transform operations using fixed or factor values.
#[derive(Clone)]
pub struct KeyAction {
    /// The key that triggers this action
    pub key: Key,
    /// Filter function that determines if this action should trigger
    pub filter: Rc<UserKeyboardFilter>,
    /// Transform to apply when key matches filter
    pub action: NoInputAction,
}

impl core::fmt::Debug for KeyAction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KeyAction")
            .field("key", &self.key)
            .field("action", &self.action)
            .field("filter", &"<function>")
            .finish()
    }
}

impl KeyAction {
    /// Try to match this action against a keyboard event, returning a transform delta if matched.
    pub fn try_match(&self, event: &KeyboardEvent, anchor_point: Point) -> Option<TransformAction> {
        // Check if the key matches
        if event.key != self.key {
            return None;
        }

        // Check if the filter matches
        if !(self.filter)(event.modifiers, event.state) {
            return None;
        }

        // Apply the keyboard action (no input needed, uses fixed or factor values)
        Some(self.action.resolve(anchor_point))
    }
}

/// Pointer input actions that can trigger transform operations.
///
/// Pointer actions represent input gestures (drag, scroll, pinch, rotate) with
/// filtering conditions and associated transform operations. Each action type
/// only accepts transforms that are compatible with that input method.
#[derive(Clone)]
pub enum PointerInputAction {
    /// Drag gesture (mouse drag, touch drag, pen drag)
    Drag {
        /// Filter function that determines if this action should trigger
        filter: Rc<PointerFilter>,
        /// Transform to apply when drag matches filter
        action: InputValue2DToTransform,
    },
    /// Exact drag gesture that uses absolute positioning without sensitivity multipliers
    DragExact {
        /// Filter function that determines if this action should trigger
        filter: Rc<PointerFilter>,
    },
    /// Drag gesture with 1D extraction (e.g., drag X axis → rotate)
    DragExtract {
        /// Filter function that determines if this action should trigger
        filter: Rc<PointerFilter>,
        /// How to extract 1D value from 2D drag input
        extract: Extract1D,
        /// Transform to apply when drag matches filter
        action: InputValue1DToTransform,
    },
    /// Scroll/wheel input
    Scroll {
        /// Filter function that determines if this action should trigger
        filter: Rc<PointerFilter>,
        /// Transform to apply when scroll matches filter
        action: InputValue2DToTransform,
    },
    /// Scroll input with 1D extraction (e.g., scroll Y axis → rotate)
    ScrollExtract {
        /// Filter function that determines if this action should trigger
        filter: Rc<PointerFilter>,
        /// How to extract 1D value from 2D scroll input
        extract: Extract1D,
        /// Transform to apply when scroll matches filter
        action: InputValue1DToTransform,
    },
    /// Pinch gesture
    Pinch {
        /// Filter function that determines if this action should trigger
        filter: Rc<PointerFilter>,
        /// Transform to apply when pinch matches filter
        action: InputValue1DToTransform,
    },
    /// Rotate gesture
    Rotate {
        /// Filter function that determines if this action should trigger
        filter: Rc<PointerFilter>,
        /// Transform to apply when rotate matches filter
        action: InputValue1DToTransform,
    },
    /// Click gesture (single pointer down-up)
    Click {
        /// Filter function that determines if this action should trigger
        filter: Rc<PointerFilter>,
        /// Transform to apply when click matches filter
        action: NoInputAction,
    },
    /// Double-click gesture (two clicks in succession)
    DoubleClick {
        /// Filter function that determines if this action should trigger
        filter: Rc<PointerFilter>,
        /// Transform to apply when double-click matches filter
        action: NoInputAction,
    },
}

impl core::fmt::Debug for PointerInputAction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Drag { action, .. } => f
                .debug_struct("Drag")
                .field("action", action)
                .field("filter", &"<function>")
                .finish(),
            Self::DragExact { .. } => f
                .debug_struct("DragExact")
                .field("filter", &"<function>")
                .finish(),
            Self::DragExtract {
                extract, action, ..
            } => f
                .debug_struct("DragExtract")
                .field("extract", extract)
                .field("action", action)
                .field("filter", &"<function>")
                .finish(),
            Self::Scroll { action, .. } => f
                .debug_struct("Scroll")
                .field("action", action)
                .field("filter", &"<function>")
                .finish(),
            Self::ScrollExtract {
                extract, action, ..
            } => f
                .debug_struct("ScrollExtract")
                .field("extract", extract)
                .field("action", action)
                .field("filter", &"<function>")
                .finish(),
            Self::Pinch { action, .. } => f
                .debug_struct("Pinch")
                .field("action", action)
                .field("filter", &"<function>")
                .finish(),
            Self::Rotate { action, .. } => f
                .debug_struct("Rotate")
                .field("action", action)
                .field("filter", &"<function>")
                .finish(),
            Self::Click { action, .. } => f
                .debug_struct("Click")
                .field("action", action)
                .field("filter", &"<function>")
                .finish(),
            Self::DoubleClick { action, .. } => f
                .debug_struct("DoubleClick")
                .field("action", action)
                .field("filter", &"<function>")
                .finish(),
        }
    }
}

impl PointerInputAction {
    /// Try to match this action against a pointer event, returning a transform delta if matched.
    pub fn try_match<T: TransformTarget>(
        &self,
        event: &PointerEvent,
        drag_delta: Option<Vec2>,
        drag_total_offset: Option<Vec2>,
        target: &T,
        drag_base_translation: Option<Vec2>,
    ) -> Option<TransformAction> {
        match self {
            Self::Drag { filter, action } => {
                let PointerEvent::Move(PointerUpdate {
                    pointer, current, ..
                }) = event
                else {
                    return None;
                };
                if !filter(current.modifiers, pointer.pointer_type) {
                    return None;
                }
                let delta = drag_delta?;
                action.resolve(delta, current.logical_point())
            }

            Self::DragExact { filter } => {
                let PointerEvent::Move(PointerUpdate {
                    pointer, current, ..
                }) = event
                else {
                    return None;
                };
                if !filter(current.modifiers, pointer.pointer_type) {
                    return None;
                }
                let base = drag_base_translation?;
                let total_offset = drag_total_offset?;
                // For exact drag, emit absolute pan using base + total offset
                Some(TransformAction::Pan(PanAction::To(base + total_offset)))
            }

            Self::DragExtract {
                filter,
                extract,
                action,
            } => {
                let PointerEvent::Move(PointerUpdate {
                    pointer, current, ..
                }) = event
                else {
                    return None;
                };
                if !filter(current.modifiers, pointer.pointer_type) {
                    return None;
                }
                let delta = drag_delta?;
                let extracted = extract.extract(delta)?;
                Some(action.resolve(extracted, current.logical_point()))
            }

            Self::Scroll { filter, action } => {
                let PointerEvent::Scroll(scroll_event) = event else {
                    return None;
                };
                if !filter(
                    scroll_event.state.modifiers,
                    scroll_event.pointer.pointer_type,
                ) {
                    return None;
                }
                let scroll_delta = resolve_scroll_delta(scroll_event, target);
                if scroll_delta.x == 0.0 && scroll_delta.y == 0.0 {
                    return None;
                }
                action.resolve(scroll_delta, scroll_event.state.logical_point())
            }

            Self::ScrollExtract {
                filter,
                extract,
                action,
            } => {
                let PointerEvent::Scroll(scroll_event) = event else {
                    return None;
                };
                if !filter(
                    scroll_event.state.modifiers,
                    scroll_event.pointer.pointer_type,
                ) {
                    return None;
                }
                let scroll_delta = resolve_scroll_delta(scroll_event, target);
                if scroll_delta.x == 0.0 && scroll_delta.y == 0.0 {
                    return None;
                }
                let extracted = extract.extract(scroll_delta)?;
                Some(action.resolve(extracted, scroll_event.state.logical_point()))
            }

            Self::Pinch { filter, action } => {
                let PointerEvent::Gesture(gesture_event) = event else {
                    return None;
                };
                let PointerGesture::Pinch(delta) = &gesture_event.gesture else {
                    return None;
                };
                if !filter(
                    gesture_event.state.modifiers,
                    gesture_event.pointer.pointer_type,
                ) {
                    return None;
                }
                Some(action.resolve(f64::from(*delta), gesture_event.state.logical_point()))
            }

            Self::Rotate { filter, action } => {
                let PointerEvent::Gesture(gesture_event) = event else {
                    return None;
                };
                let PointerGesture::Rotate(delta) = &gesture_event.gesture else {
                    return None;
                };
                if !filter(
                    gesture_event.state.modifiers,
                    gesture_event.pointer.pointer_type,
                ) {
                    return None;
                }
                Some(action.resolve(f64::from(*delta), gesture_event.state.logical_point()))
            }

            Self::Click { filter, action } => {
                let PointerEvent::Down(pointer_event) = event else {
                    return None;
                };
                if !filter(
                    pointer_event.state.modifiers,
                    pointer_event.pointer.pointer_type,
                ) {
                    return None;
                }
                Some(action.resolve(pointer_event.state.logical_point()))
            }

            Self::DoubleClick { filter, action } => {
                let PointerEvent::Down(pointer_event) = event else {
                    return None;
                };
                if !filter(
                    pointer_event.state.modifiers,
                    pointer_event.pointer.pointer_type,
                ) {
                    return None;
                }
                // Note: This is a simple implementation. In practice, you might want to
                // track timing and position to determine if it's actually a double-click
                Some(action.resolve(pointer_event.state.logical_point()))
            }
        }
    }
}

/// Builder for 2D input
#[derive(Clone, Copy, Debug)]
pub struct InputValue2DBuilder;

impl InputValue2DBuilder {
    /// 2D → 2D mappings.
    pub fn xy(self) -> Remap2DBuilder {
        Remap2DBuilder { remap: Remap2D::XY }
    }
    /// Swap X and Y axes before applying transform.
    pub fn yx(self) -> Remap2DBuilder {
        Remap2DBuilder { remap: Remap2D::YX }
    }

    /// 2D → 1D extractions.
    pub fn x(self) -> Extract1DBuilder {
        Extract1DBuilder {
            extract: Extract1D::X,
        }
    }
    /// Extract Y component from 2D input.
    pub fn y(self) -> Extract1DBuilder {
        Extract1DBuilder {
            extract: Extract1D::Y,
        }
    }
    /// Extract magnitude (length) of 2D vector, signed by dominant axis.
    pub fn magnitude(self) -> Extract1DBuilder {
        Extract1DBuilder {
            extract: Extract1D::Magnitude,
        }
    }
    /// Extract the dominant (larget magnitude) axis.
    pub fn dominant(self) -> Extract1DBuilder {
        Extract1DBuilder {
            extract: Extract1D::Dominant,
        }
    }
    /// Extract X component only if X is the dominant (larger magnitude) axis, otherwise no action.
    pub fn x_if_dominant(self) -> Extract1DBuilder {
        Extract1DBuilder {
            extract: Extract1D::XIfDominant,
        }
    }
    /// Extract Y component only if Y is the dominant (larger magnitude) axis, otherwise no action.
    pub fn y_if_dominant(self) -> Extract1DBuilder {
        Extract1DBuilder {
            extract: Extract1D::YIfDominant,
        }
    }
}

/// Builder for 2D → 2D output
#[derive(Clone, Copy, Debug)]
pub struct Remap2DBuilder {
    remap: Remap2D,
}

impl Remap2DBuilder {
    /// Create a pan action with X and Y sensitivities.
    pub fn pan(
        self,
        sensitivities: impl Into<SensitivityOrFn<Vec2, Vec2>>,
    ) -> InputValue2DToTransform {
        InputValue2DToTransform::To2D(Map2Dto2D {
            remap: self.remap,
            action: InputValueAction2D::Pan {
                value: Sensitivity2D(sensitivities.into()),
            },
        })
    }

    /// Create a uniform scale action with single sensitivity.
    pub fn scale(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
    ) -> InputValue2DToTransform {
        let sens = sensitivity.into();
        InputValue2DToTransform::To2D(Map2Dto2D {
            remap: self.remap,
            action: InputValueAction2D::Scale {
                value: Sensitivity2D(match sens {
                    SensitivityOrFn::Value(val) => SensitivityOrFn::Value(Vec2::new(val, val)),
                    SensitivityOrFn::Function(f) => {
                        SensitivityOrFn::Function(Rc::new(move |input: Vec2| {
                            let val = f(input.length());
                            Vec2::new(val, val)
                        }))
                    }
                }),
            },
        })
    }

    /// Create a scale action with X and Y sensitivities.
    pub fn scale_non_uniform(
        self,
        sensitivities: impl Into<SensitivityOrFn<Vec2, Vec2>>,
    ) -> InputValue2DToTransform {
        InputValue2DToTransform::To2D(Map2Dto2D {
            remap: self.remap,
            action: InputValueAction2D::Scale {
                value: Sensitivity2D(sensitivities.into()),
            },
        })
    }

    /// Create a uniform scale action with single sensitivity around a custom anchor point.
    pub fn scale_about(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> InputValue2DToTransform {
        let sens = sensitivity.into();
        InputValue2DToTransform::To2D(Map2Dto2D {
            remap: self.remap,
            action: InputValueAction2D::ScaleAbout {
                value: Sensitivity2D(match sens {
                    SensitivityOrFn::Value(val) => SensitivityOrFn::Value(Vec2::new(val, val)),
                    SensitivityOrFn::Function(f) => {
                        SensitivityOrFn::Function(Rc::new(move |input: Vec2| {
                            let val = f(input.length());
                            Vec2::new(val, val)
                        }))
                    }
                }),
                anchor: anchor.into(),
            },
        })
    }

    /// Create a scale action with X and Y sensitivities around a custom anchor point.
    pub fn scale_non_uniform_about(
        self,
        sensitivities: impl Into<SensitivityOrFn<Vec2, Vec2>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> InputValue2DToTransform {
        InputValue2DToTransform::To2D(Map2Dto2D {
            remap: self.remap,
            action: InputValueAction2D::ScaleAbout {
                value: Sensitivity2D(sensitivities.into()),
                anchor: anchor.into(),
            },
        })
    }

    /// Create a uniform scale action with single sensitivity around the current pointer location.
    pub fn scale_about_pointer(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
    ) -> InputValue2DToTransform {
        let sens = sensitivity.into();
        InputValue2DToTransform::To2D(Map2Dto2D {
            remap: self.remap,
            action: InputValueAction2D::ScaleAboutPointer {
                value: Sensitivity2D(match sens {
                    SensitivityOrFn::Value(val) => SensitivityOrFn::Value(Vec2::new(val, val)),
                    SensitivityOrFn::Function(f) => {
                        SensitivityOrFn::Function(Rc::new(move |input: Vec2| {
                            let val = f(input.length());
                            Vec2::new(val, val)
                        }))
                    }
                }),
            },
        })
    }

    /// Create a scale action with X and Y sensitivities around the current pointer location.
    pub fn scale_non_uniform_about_pointer(
        self,
        sensitivities: impl Into<SensitivityOrFn<Vec2, Vec2>>,
    ) -> InputValue2DToTransform {
        InputValue2DToTransform::To2D(Map2Dto2D {
            remap: self.remap,
            action: InputValueAction2D::ScaleAboutPointer {
                value: Sensitivity2D(sensitivities.into()),
            },
        })
    }
}

/// Builder for 2D → 1D output
#[derive(Clone, Copy, Debug)]
pub struct Extract1DBuilder {
    extract: Extract1D,
}

impl Extract1DBuilder {
    /// Create a rotate action from extracted 1D value.
    ///
    /// **Backend Compatibility**: Rotation is only supported by Affine; ignored by viewports.
    pub fn rotate(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
    ) -> InputValue2DToTransform {
        InputValue2DToTransform::To1D(Map2Dto1D {
            extract: self.extract,
            action: InputValueAction1D::Rotate {
                value: Sensitivity(sensitivity.into()),
            },
        })
    }

    /// Create a rotate action from extracted 1D value around a custom anchor point.
    pub fn rotate_about(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> InputValue2DToTransform {
        InputValue2DToTransform::To1D(Map2Dto1D {
            extract: self.extract,
            action: InputValueAction1D::RotateAbout {
                value: Sensitivity(sensitivity.into()),
                anchor: anchor.into(),
            },
        })
    }

    /// Create a rotate action from extracted 1D value around the current pointer location.
    pub fn rotate_about_pointer(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
    ) -> InputValue2DToTransform {
        InputValue2DToTransform::To1D(Map2Dto1D {
            extract: self.extract,
            action: InputValueAction1D::RotateAboutPointer {
                value: Sensitivity(sensitivity.into()),
            },
        })
    }
}

/// Builder for 1D input (scroll, pinch, rotate gesture)
#[derive(Clone, Copy, Debug)]
pub struct InputValue1DBuilder;

impl InputValue1DBuilder {
    /// 1D → 2D emissions.
    pub fn to_x(self) -> Emit1DBuilder {
        Emit1DBuilder {
            emit: Emit1Dto2D::X,
        }
    }
    /// Emit 1D value to Y axis only (0, value).
    pub fn to_y(self) -> Emit1DBuilder {
        Emit1DBuilder {
            emit: Emit1Dto2D::Y,
        }
    }
    /// Emit 1D value to both axes uniformly (value, value).
    pub fn uniform(self) -> Emit1DBuilder {
        Emit1DBuilder {
            emit: Emit1Dto2D::Uniform,
        }
    }

    /// 1D → 1D (direct to rotate).
    pub fn rotate(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
    ) -> InputValue1DToTransform {
        InputValue1DToTransform::Action1D(InputValueAction1D::Rotate {
            value: Sensitivity(sensitivity.into()),
        })
    }

    /// 1D → 1D (direct to rotate around a custom anchor point).
    pub fn rotate_about(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> InputValue1DToTransform {
        InputValue1DToTransform::Action1D(InputValueAction1D::RotateAbout {
            value: Sensitivity(sensitivity.into()),
            anchor: anchor.into(),
        })
    }

    /// 1D → 1D (direct to rotate around the current pointer location).
    pub fn rotate_about_pointer(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
    ) -> InputValue1DToTransform {
        InputValue1DToTransform::Action1D(InputValueAction1D::RotateAboutPointer {
            value: Sensitivity(sensitivity.into()),
        })
    }
}

/// Builder for 1D → 2D output
#[derive(Clone, Copy, Debug)]
pub struct Emit1DBuilder {
    emit: Emit1Dto2D,
}

impl Emit1DBuilder {
    /// Create a pan action from 1D input with X and Y sensitivities.
    pub fn pan(
        self,
        sensitivities: impl Into<SensitivityOrFn<Vec2, Vec2>>,
    ) -> InputValue1DToTransform {
        InputValue1DToTransform::To2D(Map1Dto2D {
            emit: self.emit,
            action: InputValueAction2D::Pan {
                value: Sensitivity2D(sensitivities.into()),
            },
        })
    }

    /// Create a uniform scale action from 1D input with single sensitivity.
    pub fn scale(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
    ) -> InputValue1DToTransform {
        let sens = sensitivity.into();
        InputValue1DToTransform::To2D(Map1Dto2D {
            emit: self.emit,
            action: InputValueAction2D::Scale {
                value: Sensitivity2D(match sens {
                    SensitivityOrFn::Value(val) => SensitivityOrFn::Value(Vec2::new(val, val)),
                    SensitivityOrFn::Function(f) => {
                        SensitivityOrFn::Function(Rc::new(move |input: Vec2| {
                            let val = f(input.length());
                            Vec2::new(val, val)
                        }))
                    }
                }),
            },
        })
    }

    /// Create a scale action from 1D input with X and Y sensitivities.
    pub fn scale_non_uniform(
        self,
        sensitivities: impl Into<SensitivityOrFn<Vec2, Vec2>>,
    ) -> InputValue1DToTransform {
        InputValue1DToTransform::To2D(Map1Dto2D {
            emit: self.emit,
            action: InputValueAction2D::Scale {
                value: Sensitivity2D(sensitivities.into()),
            },
        })
    }

    /// Create a uniform scale action from 1D input with single sensitivity around a custom anchor point.
    pub fn scale_about(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> InputValue1DToTransform {
        let sens = sensitivity.into();
        InputValue1DToTransform::To2D(Map1Dto2D {
            emit: self.emit,
            action: InputValueAction2D::ScaleAbout {
                value: Sensitivity2D(match sens {
                    SensitivityOrFn::Value(val) => SensitivityOrFn::Value(Vec2::new(val, val)),
                    SensitivityOrFn::Function(f) => {
                        SensitivityOrFn::Function(Rc::new(move |input: Vec2| {
                            let val = f(input.length());
                            Vec2::new(val, val)
                        }))
                    }
                }),
                anchor: anchor.into(),
            },
        })
    }

    /// Create a scale action from 1D input with X and Y sensitivities around a custom anchor point.
    pub fn scale_non_uniform_about(
        self,
        sensitivities: impl Into<SensitivityOrFn<Vec2, Vec2>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> InputValue1DToTransform {
        InputValue1DToTransform::To2D(Map1Dto2D {
            emit: self.emit,
            action: InputValueAction2D::ScaleAbout {
                value: Sensitivity2D(sensitivities.into()),
                anchor: anchor.into(),
            },
        })
    }

    /// Create a uniform scale action from 1D input with single sensitivity around the current pointer location.
    pub fn scale_about_pointer(
        self,
        sensitivity: impl Into<SensitivityOrFn<f64, f64>>,
    ) -> InputValue1DToTransform {
        let sens = sensitivity.into();
        InputValue1DToTransform::To2D(Map1Dto2D {
            emit: self.emit,
            action: InputValueAction2D::ScaleAboutPointer {
                value: Sensitivity2D(match sens {
                    SensitivityOrFn::Value(val) => SensitivityOrFn::Value(Vec2::new(val, val)),
                    SensitivityOrFn::Function(f) => {
                        SensitivityOrFn::Function(Rc::new(move |input: Vec2| {
                            let val = f(input.length());
                            Vec2::new(val, val)
                        }))
                    }
                }),
            },
        })
    }

    /// Create a scale action from 1D input with X and Y sensitivities around the current pointer location.
    pub fn scale_non_uniform_about_pointer(
        self,
        sensitivities: impl Into<SensitivityOrFn<Vec2, Vec2>>,
    ) -> InputValue1DToTransform {
        InputValue1DToTransform::To2D(Map1Dto2D {
            emit: self.emit,
            action: InputValueAction2D::ScaleAboutPointer {
                value: Sensitivity2D(sensitivities.into()),
            },
        })
    }
}

/// Builder for actions with no input that can produce either fixed or factor actions
#[derive(Clone, Copy, Debug)]
pub struct NoInputActionBuilder;

/// Fixed-value keyboard action builder
#[derive(Clone, Copy, Debug)]
pub struct FixedBuilder;

/// Factor-value keyboard action builder  
#[derive(Clone, Copy, Debug)]
pub struct FactorActionBuilder;

/// Delta-value keyboard action builder  
#[derive(Clone, Copy, Debug)]
pub struct DeltaActionBuilder;

impl NoInputActionBuilder {
    /// Switch to fixed-value builder for absolute positioning/movement.
    pub fn fixed(self) -> FixedBuilder {
        FixedBuilder
    }

    /// Switch to factor-value builder for multiplicative operations.
    pub fn factor(self) -> FactorActionBuilder {
        FactorActionBuilder
    }

    /// Switch to delta-value builder for additive operations.
    pub fn delta(self) -> DeltaActionBuilder {
        DeltaActionBuilder
    }
}

impl FixedBuilder {
    /// Create a pan action with fixed X and Y deltas.
    pub fn pan(self, deltas: impl Into<ValueOrFn<Vec2>>) -> NoInputAction {
        NoInputAction::Fixed2D(FixedAction2D::Pan {
            value: Fixed2D(deltas.into()),
        })
    }

    /// Create a uniform scale action with fixed scale factor (uses backend's default center).
    pub fn scale(self, factor: impl Into<ValueOrFn<f64>>) -> NoInputAction {
        let f = factor.into();
        NoInputAction::Fixed2D(FixedAction2D::Scale {
            value: Fixed2D(match f {
                ValueOrFn::Value(val) => ValueOrFn::Value(Vec2::new(val, val)),
                ValueOrFn::Function(func) => ValueOrFn::Function(Rc::new(move || {
                    let val = func();
                    Vec2::new(val, val)
                })),
            }),
        })
    }

    /// Create a scale action with fixed scale factors (uses backend's default center).
    ///
    /// **Backend Compatibility**: Non-uniform scaling is averaged by `Viewport2D` but fully supported by Affine.
    pub fn scale_non_uniform(self, factors: impl Into<ValueOrFn<Vec2>>) -> NoInputAction {
        NoInputAction::Fixed2D(FixedAction2D::Scale {
            value: Fixed2D(factors.into()),
        })
    }

    /// Create a uniform scale action with fixed scale factor around a custom anchor point.
    pub fn scale_about(
        self,
        factor: impl Into<ValueOrFn<f64>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> NoInputAction {
        let f = factor.into();
        NoInputAction::Fixed2D(FixedAction2D::ScaleAbout {
            value: Fixed2D(ValueOrFn::Function(Rc::new(move || {
                let val = f.get();
                Vec2::new(val, val)
            }))),
            anchor: anchor.into(),
        })
    }

    /// Create a scale action with fixed scale factors around a custom anchor point.
    pub fn scale_non_uniform_about(
        self,
        factors: impl Into<ValueOrFn<Vec2>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> NoInputAction {
        NoInputAction::Fixed2D(FixedAction2D::ScaleAbout {
            value: Fixed2D(factors.into()),
            anchor: anchor.into(),
        })
    }

    /// Create a uniform scale action with fixed scale factor around the last known pointer location.
    pub fn scale_about_pointer(self, factor: impl Into<ValueOrFn<f64>>) -> NoInputAction {
        let f = factor.into();
        NoInputAction::Fixed2D(FixedAction2D::ScaleAboutPointer {
            value: Fixed2D(ValueOrFn::Function(Rc::new(move || {
                let val = f.get();
                Vec2::new(val, val)
            }))),
        })
    }

    /// Create a scale action with fixed scale factors around the last known pointer location.
    pub fn scale_non_uniform_about_pointer(
        self,
        factors: impl Into<ValueOrFn<Vec2>>,
    ) -> NoInputAction {
        NoInputAction::Fixed2D(FixedAction2D::ScaleAboutPointer {
            value: Fixed2D(factors.into()),
        })
    }
}

impl FactorActionBuilder {
    /// Create a uniform scale action with factor scale multiplier around the last known pointer location.
    pub fn scale_about_pointer(self, factor: impl Into<ValueOrFn<f64>>) -> NoInputAction {
        let f = factor.into();
        NoInputAction::Factor2D(FactorAction2D::ScaleAboutPointer {
            value: Factor2D(ValueOrFn::Function(Rc::new(move || {
                let val = f.get();
                Vec2::new(val, val)
            }))),
        })
    }

    /// Create a uniform scale action with factor scale multiplier (uses backend's default center).
    pub fn scale(self, factor: impl Into<ValueOrFn<f64>>) -> NoInputAction {
        let f = factor.into();
        NoInputAction::Factor2D(FactorAction2D::Scale {
            value: Factor2D(ValueOrFn::Function(Rc::new(move || {
                let val = f.get();
                Vec2::new(val, val)
            }))),
        })
    }

    /// Create a uniform scale action with factor scale multiplier around a custom anchor point.
    pub fn scale_about(
        self,
        factor: impl Into<ValueOrFn<f64>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> NoInputAction {
        let f = factor.into();
        NoInputAction::Factor2D(FactorAction2D::ScaleAbout {
            value: Factor2D(ValueOrFn::Function(Rc::new(move || {
                let val = f.get();
                Vec2::new(val, val)
            }))),
            anchor: anchor.into(),
        })
    }

    /// Create a scale action with factor scale multipliers around the last known pointer location.
    pub fn scale_non_uniform_about_pointer(
        self,
        factors: impl Into<ValueOrFn<Vec2>>,
    ) -> NoInputAction {
        NoInputAction::Factor2D(FactorAction2D::ScaleAboutPointer {
            value: Factor2D(factors.into()),
        })
    }

    /// Create a scale action with factor X and Y scale multipliers (uses backend's default center).
    ///
    /// **Backend Compatibility**: Non-uniform scaling is averaged by `Viewport2D` but fully supported by Affine.
    pub fn scale_non_uniform(self, factors: impl Into<ValueOrFn<Vec2>>) -> NoInputAction {
        NoInputAction::Factor2D(FactorAction2D::Scale {
            value: Factor2D(factors.into()),
        })
    }

    /// Create a scale action with factor scale multipliers around a custom anchor point.
    pub fn scale_non_uniform_about(
        self,
        factors: impl Into<ValueOrFn<Vec2>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> NoInputAction {
        NoInputAction::Factor2D(FactorAction2D::ScaleAbout {
            value: Factor2D(factors.into()),
            anchor: anchor.into(),
        })
    }
}

impl DeltaActionBuilder {
    /// Create a pan action with delta X and Y values.
    pub fn pan(self, deltas: impl Into<ValueOrFn<Vec2>>) -> NoInputAction {
        NoInputAction::Delta2D(DeltaAction2D::Pan {
            value: Delta2D(deltas.into()),
        })
    }

    /// Create a uniform scale action with delta scale value (uses backend's default center).
    pub fn scale(self, delta: impl Into<ValueOrFn<f64>>) -> NoInputAction {
        let d = delta.into();
        NoInputAction::Delta2D(DeltaAction2D::Scale {
            value: Delta2D(ValueOrFn::Function(Rc::new(move || {
                let val = d.get();
                Vec2::new(val, val)
            }))),
        })
    }

    /// Create a scale action with delta scale values (uses backend's default center).
    ///
    /// **Backend Compatibility**: Non-uniform scaling is averaged by `Viewport2D` but fully supported by Affine.
    pub fn scale_non_uniform(self, deltas: impl Into<ValueOrFn<Vec2>>) -> NoInputAction {
        NoInputAction::Delta2D(DeltaAction2D::Scale {
            value: Delta2D(deltas.into()),
        })
    }

    /// Create a uniform scale action with delta scale value around a custom anchor point.
    pub fn scale_about(
        self,
        delta: impl Into<ValueOrFn<f64>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> NoInputAction {
        let d = delta.into();
        NoInputAction::Delta2D(DeltaAction2D::ScaleAbout {
            value: Delta2D(ValueOrFn::Function(Rc::new(move || {
                let val = d.get();
                Vec2::new(val, val)
            }))),
            anchor: anchor.into(),
        })
    }

    /// Create a scale action with delta scale values around a custom anchor point.
    pub fn scale_non_uniform_about(
        self,
        deltas: impl Into<ValueOrFn<Vec2>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> NoInputAction {
        NoInputAction::Delta2D(DeltaAction2D::ScaleAbout {
            value: Delta2D(deltas.into()),
            anchor: anchor.into(),
        })
    }

    /// Create a uniform scale action with delta scale value around the last known pointer location.
    pub fn scale_about_pointer(self, delta: impl Into<ValueOrFn<f64>>) -> NoInputAction {
        let d = delta.into();
        NoInputAction::Delta2D(DeltaAction2D::ScaleAboutPointer {
            value: Delta2D(ValueOrFn::Function(Rc::new(move || {
                let val = d.get();
                Vec2::new(val, val)
            }))),
        })
    }

    /// Create a scale action with delta scale values around the last known pointer location.
    pub fn scale_non_uniform_about_pointer(
        self,
        deltas: impl Into<ValueOrFn<Vec2>>,
    ) -> NoInputAction {
        NoInputAction::Delta2D(DeltaAction2D::ScaleAboutPointer {
            value: Delta2D(deltas.into()),
        })
    }

    /// Create a rotate action with delta rotation value (uses backend's default center).
    ///
    /// **Backend Compatibility**: Rotation is only supported by Affine; ignored by viewports.
    pub fn rotate(self, delta: impl Into<ValueOrFn<f64>>) -> NoInputAction {
        NoInputAction::Delta1D(DeltaAction1D::Rotate {
            value: Delta(delta.into()),
        })
    }

    /// Create a rotate action with delta rotation value around a custom anchor point.
    pub fn rotate_about(
        self,
        delta: impl Into<ValueOrFn<f64>>,
        anchor: impl Into<ValueOrFn<Point>>,
    ) -> NoInputAction {
        NoInputAction::Delta1D(DeltaAction1D::RotateAbout {
            value: Delta(delta.into()),
            anchor: anchor.into(),
        })
    }

    /// Create a rotate action with delta rotation value around the last known pointer location.
    ///
    /// **Backend Compatibility**: Rotation is only supported by Affine; ignored by viewports.
    pub fn rotate_about_pointer(self, delta: impl Into<ValueOrFn<f64>>) -> NoInputAction {
        NoInputAction::Delta1D(DeltaAction1D::RotateAboutPointer {
            value: Delta(delta.into()),
        })
    }
}

/// Configuration for how input combinations map to transform actions
#[derive(Clone, Default, Debug)]
pub struct Behaviors {
    /// All pointer input actions with their filtering and transform behaviors
    pub pointer_actions: Vec<PointerInputAction>,
    /// All keyboard actions with their filtering and transform behaviors
    pub key_actions: Vec<KeyAction>,
}

impl Behaviors {
    /// Creates a new empty behavior configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add action for drag events with builder pattern.
    #[must_use]
    pub fn drag<F, B>(mut self, filter: F, build: B) -> Self
    where
        F: Fn(Modifiers, PointerType) -> bool + 'static,
        B: FnOnce(InputValue2DBuilder) -> InputValue2DToTransform,
    {
        self.pointer_actions.push(PointerInputAction::Drag {
            filter: Rc::new(filter),
            action: build(InputValue2DBuilder),
        });
        self
    }

    /// Add action for exact drag events (absolute positioning, no sensitivity multipliers).
    #[must_use]
    pub fn drag_exact<F>(mut self, filter: F) -> Self
    where
        F: Fn(Modifiers, PointerType) -> bool + 'static,
    {
        self.pointer_actions.push(PointerInputAction::DragExact {
            filter: Rc::new(filter),
        });
        self
    }

    /// Add action for scroll events with builder pattern.
    #[must_use]
    pub fn scroll<F, B>(mut self, filter: F, build: B) -> Self
    where
        F: Fn(Modifiers, PointerType) -> bool + 'static,
        B: FnOnce(InputValue2DBuilder) -> InputValue2DToTransform,
    {
        self.pointer_actions.push(PointerInputAction::Scroll {
            filter: Rc::new(filter),
            action: build(InputValue2DBuilder),
        });
        self
    }

    /// Add action for pinch gestures with builder pattern.
    #[must_use]
    pub fn pinch<F, B>(mut self, filter: F, build: B) -> Self
    where
        F: Fn(Modifiers, PointerType) -> bool + 'static,
        B: FnOnce(InputValue1DBuilder) -> InputValue1DToTransform,
    {
        self.pointer_actions.push(PointerInputAction::Pinch {
            filter: Rc::new(filter),
            action: build(InputValue1DBuilder),
        });
        self
    }

    /// Add action for rotate gestures with builder pattern.
    #[must_use]
    pub fn rotate<F, B>(mut self, filter: F, build: B) -> Self
    where
        F: Fn(Modifiers, PointerType) -> bool + 'static,
        B: FnOnce(InputValue1DBuilder) -> InputValue1DToTransform,
    {
        self.pointer_actions.push(PointerInputAction::Rotate {
            filter: Rc::new(filter),
            action: build(InputValue1DBuilder),
        });
        self
    }

    /// Add action for X-axis scroll events (convenience helper).
    #[must_use]
    pub fn scroll_x<F, B>(mut self, filter: F, build: B) -> Self
    where
        F: Fn(Modifiers, PointerType) -> bool + 'static,
        B: FnOnce(InputValue1DBuilder) -> InputValue1DToTransform,
    {
        self.pointer_actions
            .push(PointerInputAction::ScrollExtract {
                filter: Rc::new(filter),
                extract: Extract1D::X,
                action: build(InputValue1DBuilder),
            });
        self
    }

    /// Add action for Y-axis scroll events (convenience helper).
    #[must_use]
    pub fn scroll_y<F, B>(mut self, filter: F, build: B) -> Self
    where
        F: Fn(Modifiers, PointerType) -> bool + 'static,
        B: FnOnce(InputValue1DBuilder) -> InputValue1DToTransform,
    {
        self.pointer_actions
            .push(PointerInputAction::ScrollExtract {
                filter: Rc::new(filter),
                extract: Extract1D::Y,
                action: build(InputValue1DBuilder),
            });
        self
    }

    /// Add action for keyboard key events with builder pattern.
    #[must_use]
    pub fn key<F, B>(mut self, key: Key, filter: F, build: B) -> Self
    where
        F: Fn(Modifiers, KeyState) -> bool + 'static,
        B: FnOnce(NoInputActionBuilder) -> NoInputAction,
    {
        self.key_actions.push(KeyAction {
            key,
            filter: Rc::new(filter),
            action: build(NoInputActionBuilder),
        });
        self
    }

    /// Add action for click events with builder pattern.
    #[must_use]
    pub fn click<F, B>(mut self, filter: F, build: B) -> Self
    where
        F: Fn(Modifiers, PointerType) -> bool + 'static,
        B: FnOnce(NoInputActionBuilder) -> NoInputAction,
    {
        self.pointer_actions.push(PointerInputAction::Click {
            filter: Rc::new(filter),
            action: build(NoInputActionBuilder),
        });
        self
    }

    /// Add action for double-click events with builder pattern.
    #[must_use]
    pub fn double_click<F, B>(mut self, filter: F, build: B) -> Self
    where
        F: Fn(Modifiers, PointerType) -> bool + 'static,
        B: FnOnce(NoInputActionBuilder) -> NoInputAction,
    {
        self.pointer_actions.push(PointerInputAction::DoubleClick {
            filter: Rc::new(filter),
            action: build(NoInputActionBuilder),
        });
        self
    }

    /// Process a pointer event and produce transform deltas.
    pub fn process_pointer<T: TransformTarget>(
        &self,
        event: &PointerEvent,
        drag_state: &mut DragState,
        target: &T,
        drag_base_translation: &mut Option<Vec2>,
    ) -> Vec<TransformAction> {
        match event {
            PointerEvent::Down(e) => {
                drag_state.start(e.state.logical_point());
                *drag_base_translation = Some(target.current_translation());
                return Vec::new();
            }
            PointerEvent::Up(_) => {
                drag_state.end();
                *drag_base_translation = None;
                return Vec::new();
            }
            _ => {}
        }

        let (drag_delta, drag_total_offset) = if let PointerEvent::Move(e) = event {
            let delta = drag_state.update(e.current.logical_point());
            let total = drag_state.total_offset(e.current.logical_point());
            (delta, total)
        } else {
            (None, None)
        };

        self.pointer_actions
            .iter()
            .filter_map(|action| {
                action.try_match(
                    event,
                    drag_delta,
                    drag_total_offset,
                    target,
                    *drag_base_translation,
                )
            })
            .collect()
    }

    /// Process a keyboard event and produce transform deltas.
    pub fn process_keyboard(
        &self,
        event: &KeyboardEvent,
        anchor_point: Point,
    ) -> Vec<TransformAction> {
        self.key_actions
            .iter()
            .filter_map(|action| action.try_match(event, anchor_point))
            .collect()
    }
}

/// Input event processor with behavior configuration.
///
/// This type manages input behavior configuration and drag state without owning
/// the transform target. It processes events and applies transforms to targets
/// passed as mutable references.
#[derive(Debug)]
pub struct TransformEncoder {
    /// Input event processing behavior
    pub behaviors: Behaviors,
    /// Drag state for event processing
    drag_state: DragState,
    /// Last known pointer position for keyboard event anchoring
    last_pointer_position: Point,
    /// Base translation when drag started (for absolute positioning)
    drag_base_translation: Option<Vec2>,
}

impl TransformEncoder {
    /// Create a new encoder with the given behavior configuration.
    pub fn new(behaviors: Behaviors) -> Self {
        Self {
            behaviors,
            drag_state: DragState::default(),
            last_pointer_position: Point::ZERO,
            drag_base_translation: None,
        }
    }

    /// Process a pointer event and apply any resulting transform deltas to the target.
    ///
    /// Returns true if the event was handled and the target was modified.
    pub fn encode<T: TransformTarget>(&mut self, event: &PointerEvent, target: &mut T) -> bool {
        // Update last pointer position from pointer events that have meaningful positions
        match event {
            PointerEvent::Down(e) => {
                self.last_pointer_position = e.state.logical_point();
            }
            PointerEvent::Move(e) => {
                self.last_pointer_position = e.current.logical_point();
            }
            PointerEvent::Up(e) => {
                self.last_pointer_position = e.state.logical_point();
            }
            PointerEvent::Scroll(e) => {
                self.last_pointer_position = e.state.logical_point();
            }
            PointerEvent::Gesture(e) => {
                self.last_pointer_position = e.state.logical_point();
            }
            PointerEvent::Cancel(_) | PointerEvent::Enter(_) | PointerEvent::Leave(_) => {
                // Don't update position for these events
            }
        }

        let deltas = self.behaviors.process_pointer(
            event,
            &mut self.drag_state,
            target,
            &mut self.drag_base_translation,
        );
        let handled = !deltas.is_empty();
        for delta in deltas {
            target.apply(delta);
        }
        handled
    }

    /// Process a keyboard event and apply any resulting transform deltas to the target.
    ///
    /// Returns true if the event was handled and the target was modified.
    pub fn encode_keyboard<T: TransformTarget>(
        &mut self,
        event: &KeyboardEvent,
        target: &mut T,
    ) -> bool {
        let deltas = self
            .behaviors
            .process_keyboard(event, self.last_pointer_position);
        let handled = !deltas.is_empty();
        for delta in deltas {
            target.apply(delta);
        }
        handled
    }

    /// Apply deltas directly to a target (useful for testing or manual control).
    pub fn apply_deltas<T: TransformTarget>(
        &mut self,
        target: &mut T,
        deltas: impl IntoIterator<Item = TransformAction>,
    ) {
        for delta in deltas {
            target.apply(delta);
        }
    }
}

/// Tracks drag state for move event processing
#[derive(Debug, Clone, Default)]
pub struct DragState {
    /// Whether a drag operation is currently active
    pub is_dragging: bool,
    /// Start position of the drag operation
    pub start_pos: Option<Point>,
    /// Last recorded pointer position during drag
    pub last_pos: Option<Point>,
}

impl DragState {
    /// Start tracking a new drag operation from the given position.
    pub fn start(&mut self, pos: Point) {
        self.is_dragging = true;
        self.start_pos = Some(pos);
        self.last_pos = Some(pos);
    }

    /// Update the drag state with a new position, returning the movement delta since last update.
    pub fn update(&mut self, pos: Point) -> Option<Vec2> {
        if self.is_dragging {
            if let Some(last_pos) = self.last_pos {
                let delta = pos - last_pos;
                self.last_pos = Some(pos);
                Some(delta)
            } else {
                self.last_pos = Some(pos);
                None
            }
        } else {
            None
        }
    }

    /// Get total offset from drag start position.
    pub fn total_offset(&self, current_pos: Point) -> Option<Vec2> {
        if self.is_dragging {
            self.start_pos.map(|start_pos| current_pos - start_pos)
        } else {
            None
        }
    }

    /// End the current drag operation and reset state.
    pub fn end(&mut self) {
        self.is_dragging = false;
        self.start_pos = None;
        self.last_pos = None;
    }
}

fn resolve_scroll_delta<T: TransformTarget>(event: &PointerScrollEvent, target: &T) -> Vec2 {
    use ui_events::ScrollDelta;
    match &event.delta {
        ScrollDelta::PixelDelta(pos) => {
            let logical = pos.to_logical(event.state.scale_factor);
            Vec2::new(logical.x, logical.y)
        }
        ScrollDelta::LineDelta(x, y) => {
            // Use target-specific line size
            let line_size = target.line_size();
            Vec2::new(f64::from(*x) * line_size.x, f64::from(*y) * line_size.y)
        }
        ScrollDelta::PageDelta(x, y) => {
            // Use target-specific page size
            let page_size = target.page_size();
            Vec2::new(f64::from(*x) * page_size.x, f64::from(*y) * page_size.y)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Affine, Point, Rect, Vec2};
    use ui_events::pointer::PointerType;
    use understory_view2d::{Viewport1D, Viewport2D};

    #[test]
    fn test_transform_target_viewport1d() {
        let mut viewport = Viewport1D::new(0.0..100.0);

        // Test pan - should use magnitude with sign from dominant axis
        viewport.apply(TransformAction::Pan(PanAction::By(Vec2::new(10.0, 5.0))));

        // Test scale - should scale about viewport center
        viewport.apply(TransformAction::Scale(ScaleAction::By {
            scale: Vec2::new(2.0, 2.0),
        }));

        // Test scale about - should work with anchor point
        viewport.apply(TransformAction::Scale(ScaleAction::ByAbout {
            scale: Vec2::new(1.5, 3.0),
            anchor: Point::new(25.0, 50.0),
        }));

        // Test rotate (should be ignored - 1D doesn't rotate)
        viewport.apply(TransformAction::Rotate(RotateAction::By { radians: 1.0 }));
    }

    #[test]
    fn test_transform_target_viewport2d() {
        let mut viewport = Viewport2D::new(Rect::new(0.0, 0.0, 800.0, 600.0));

        // Test pan
        viewport.apply(TransformAction::Pan(PanAction::By(Vec2::new(10.0, 5.0))));

        // Test scale (uniform viewport uses average of scale factors)
        viewport.apply(TransformAction::Scale(ScaleAction::By {
            scale: Vec2::new(2.0, 2.0),
        }));

        // Test scale about
        viewport.apply(TransformAction::Scale(ScaleAction::ByAbout {
            scale: Vec2::new(1.5, 1.5),
            anchor: Point::new(200.0, 150.0),
        }));

        // Test rotate (should be ignored)
        viewport.apply(TransformAction::Rotate(RotateAction::By { radians: 0.5 }));
    }

    #[test]
    fn test_transform_target_affine() {
        let mut transform = Affine::IDENTITY;

        // Test pan
        transform.apply(TransformAction::Pan(PanAction::By(Vec2::new(10.0, 5.0))));
        let expected_pan = Affine::translate((10.0, 5.0));
        assert_eq!(transform, expected_pan);

        // Reset
        transform = Affine::IDENTITY;

        // Test scale (non-uniform - scales about origin)
        transform.apply(TransformAction::Scale(ScaleAction::By {
            scale: Vec2::new(2.0, 3.0),
        }));
        let expected_scale = Affine::scale_non_uniform(2.0, 3.0);
        assert_eq!(transform, expected_scale);

        // Reset
        transform = Affine::IDENTITY;

        // Test rotate (no anchor - rotates about origin)
        transform.apply(TransformAction::Rotate(RotateAction::By {
            radians: 1.57, // ~90 degrees
        }));
        let expected_rotate = Affine::rotate(1.57);
        assert_eq!(transform, expected_rotate);
    }

    #[test]
    fn test_anchored_transform_actions() {
        // Test anchored scale
        let mut viewport = Viewport2D::new(Rect::new(0.0, 0.0, 800.0, 600.0));
        let anchor = Point::new(400.0, 300.0);
        viewport.apply(TransformAction::Scale(ScaleAction::ByAbout {
            scale: Vec2::new(2.0, 2.0),
            anchor,
        }));

        // Test anchored rotate on Affine
        let mut transform = Affine::IDENTITY;
        transform.apply(TransformAction::Rotate(RotateAction::ByAbout {
            radians: 1.57,
            anchor: Point::new(100.0, 100.0),
        }));
        let expected = Affine::rotate_about(1.57, Point::new(100.0, 100.0));
        assert_eq!(transform, expected);

        // Test non-uniform anchored scale on Affine
        transform = Affine::IDENTITY;
        let scale = Vec2::new(2.0, 3.0);
        let anchor = Point::new(50.0, 50.0);
        transform.apply(TransformAction::Scale(ScaleAction::ByAbout {
            scale,
            anchor,
        }));
        let expected_scale = Affine::translate(anchor.to_vec2())
            * Affine::scale_non_uniform(scale.x, scale.y)
            * Affine::translate(-anchor.to_vec2());
        assert_eq!(transform, expected_scale);
    }

    #[test]
    fn test_behavior_creation() {
        let _behavior = Behaviors::new()
            .drag(|_m, _p| true, |d| d.xy().pan(Vec2::new(1.0, 1.0)))
            .scroll(|_m, _p| true, |s| s.y().rotate(0.01));
    }

    #[test]
    fn test_drag_state() {
        let mut drag_state = DragState::default();

        assert!(!drag_state.is_dragging);
        assert!(drag_state.last_pos.is_none());

        let start_pos = Point::new(100.0, 50.0);
        drag_state.start(start_pos);

        assert!(drag_state.is_dragging);
        assert_eq!(drag_state.last_pos, Some(start_pos));

        let move_pos = Point::new(110.0, 55.0);
        let delta = drag_state.update(move_pos);
        assert_eq!(delta, Some(Vec2::new(10.0, 5.0)));
        assert_eq!(drag_state.last_pos, Some(move_pos));

        drag_state.end();
        assert!(!drag_state.is_dragging);
        assert!(drag_state.last_pos.is_none());
    }

    #[test]
    fn test_action_scale_axis_behavior() {
        let behavior = Behaviors::new()
            .scroll_x(
                |_m, _p| _p == PointerType::Mouse,
                |x| x.uniform().pan(Vec2::new(0.1, 0.1)),
            )
            .scroll_y(|_m, _p| _p == PointerType::Mouse, |a| a.rotate(0.1));

        let _encoder = TransformEncoder::new(behavior);
    }

    #[test]
    fn test_scroll_actions() {
        let behavior = Behaviors::new()
            .scroll_x(
                |_m, _p| _p == PointerType::Mouse,
                |a| a.uniform().pan(Vec2::new(0.1, 0.1)),
            )
            .scroll_y(|_m, _p| _p == PointerType::Mouse, |a| a.rotate(0.02))
            .scroll_x(|_m, _p| _p == PointerType::Touch, |a| a.rotate(0.005));

        assert_eq!(behavior.pointer_actions.len(), 3);
    }

    #[test]
    fn test_keyboard_builder_api() {
        use ui_events::keyboard::{Key, NamedKey};

        let behavior = Behaviors::new()
            .key(
                Key::Named(NamedKey::ArrowLeft),
                |_, state| state.is_down(),
                |k| k.fixed().pan(Vec2::new(-10.0, 0.0)),
            )
            .key(
                Key::Named(NamedKey::ArrowRight),
                |_, state| state.is_down(),
                |k| k.fixed().pan(Vec2::new(10.0, 0.0)),
            )
            .key(
                Key::Named(NamedKey::ArrowUp),
                |_, state| state.is_down(),
                |k| k.fixed().pan(Vec2::new(0.0, -10.0)),
            )
            .key(
                Key::Named(NamedKey::ArrowDown),
                |_, state| state.is_down(),
                |k| k.fixed().pan(Vec2::new(0.0, 10.0)),
            )
            .key(
                Key::Named(NamedKey::Enter),
                |_, state| state.is_down(),
                |k| k.factor().scale_about_pointer(10.),
            )
            .double_click(|_, _| true, |b| b.fixed().pan(|| Vec2::new(0.5, 0.5)))
            .key(
                Key::Named(NamedKey::Escape),
                |_, state| state.is_down(),
                |k| k.factor().scale_about_pointer(0.9),
            );

        assert_eq!(behavior.key_actions.len(), 6);
        assert_eq!(behavior.pointer_actions.len(), 0);

        // Verify the first key action
        let arrow_left = &behavior.key_actions[0];
        assert_eq!(arrow_left.key, Key::Named(NamedKey::ArrowLeft));
        if let NoInputAction::Fixed2D(FixedAction2D::Pan { value }) = &arrow_left.action {
            assert_eq!(value.0.get(), Vec2::new(-10.0, 0.0));
        } else {
            panic!("Expected Pan action");
        }
    }

    #[test]
    fn test_anchored_actions() {
        let behavior = Behaviors::new()
            .drag(
                |_m, _p| _p == PointerType::Mouse,
                |d| d.xy().scale_non_uniform(Vec2::new(0.1, 0.1)),
            )
            .scroll(|_m, _p| _p == PointerType::Mouse, |s| s.x().rotate(0.01))
            .pinch(
                |_, _| true,
                |p| p.uniform().scale_non_uniform(Vec2::new(0.5, 0.5)),
            )
            .rotate(|_, _| true, |r| r.rotate(1.0));

        assert_eq!(behavior.pointer_actions.len(), 4);

        // Verify the actions are properly configured
        if let PointerInputAction::Drag { action, .. } = &behavior.pointer_actions[0]
            && let InputValue2DToTransform::To2D(map) = action
        {
            matches!(map.action, InputValueAction2D::ScaleAbout { .. });
        }
    }
}
