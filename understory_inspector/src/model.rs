// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Inspector-specific model traits.

use understory_outline::OutlineModel;

/// Hierarchical model requirements for [`crate::Inspector`].
///
/// This extends [`OutlineModel`] with parent lookup so the inspector can
/// reconcile focus when a visible descendant disappears after collapse or
/// model change.
pub trait InspectorModel: OutlineModel {
    /// Returns the parent-like fallback for `key`, if any.
    ///
    /// When the focused key is no longer visible, the inspector uses this to
    /// find the nearest still-meaningful ancestor before falling back to the
    /// first visible row.
    fn parent_key(&self, key: &Self::Key) -> Option<Self::Key>;
}

impl<M> InspectorModel for &M
where
    M: InspectorModel + ?Sized,
{
    fn parent_key(&self, key: &Self::Key) -> Option<Self::Key> {
        (**self).parent_key(key)
    }
}

impl<M> InspectorModel for &mut M
where
    M: InspectorModel + ?Sized,
{
    fn parent_key(&self, key: &Self::Key) -> Option<Self::Key> {
        (**self).parent_key(key)
    }
}
