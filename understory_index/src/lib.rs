// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// After you edit the crate's doc comment, run this command, then check README.md for any missing links
// cargo rdme --workspace-project=understory_index --heading-base-level=0

//! Understory Index: a generic 2D AABB index (boundary index).
//!
//! Understory Index is a reusable building block for spatial queries.
//!
//! - Insert, update, and remove axis-aligned bounding boxes (AABBs) with user payloads.
//! - Query by point or intersecting rectangle.
//! - Batch updates with [`Index::commit`] and receive coarse damage (added/removed/moved boxes).
//!
//! It is generic over the scalar type `T` and does not depend on any geometry crate.
//! Higher layers (like a scene or region tree) can compute world-space AABBs and feed them here.
//!
//! Backends are pluggable via a simple trait so you can swap the spatial strategy without API churn.
//! The default backend is a flat vector (linear scan).
//! R-tree and BVH backends are generic over the scalar and use widened accumulator types (f32→f64, f64→f64, i64→i128) for SAH-like splits.
//!
//! # Example
//!
//! ```rust
//! use understory_index::{Index, Aabb2D};
//!
//! // Create an index and add two boxes.
//! let mut idx: Index<i64, u32> = Index::new();
//! let k1 = idx.insert(Aabb2D::new(0, 0, 10, 10), 1);
//! let k2 = idx.insert(Aabb2D::new(5, 5, 15, 15), 2);
//! let _damage0 = idx.commit();
//!
//! // Move the first box and commit a damage set.
//! idx.update(k1, Aabb2D::new(20, 0, 30, 10));
//! let damage = idx.commit();
//! assert!(!damage.is_empty());
//!
//! // Query a point inside the second box.
//! let hits: Vec<_> = idx.query_point(6, 6).collect();
//! assert_eq!(hits.len(), 1);
//! assert_eq!(hits[0].1, 2);
//! ```
//!
//! You can opt into a different backend when you need better query/update balance:
//!
//! ```rust
//! use understory_index::{Index, Aabb2D};
//!
//! // Use an R-tree (f64) for indexing.
//! let mut idx = Index::<f64, u32>::with_rtree();
//! let _k = idx.insert(Aabb2D::new(0.0, 0.0, 100.0, 100.0), 1);
//! let _ = idx.commit();
//!
//! // Query a point.
//! let hits: Vec<_> = idx.query_point(10.0, 10.0).collect();
//! assert_eq!(hits.len(), 1);
//! ```
//!
//! ## Choosing a backend
//!
//! - `FlatVec` (default): simplest and smallest, linear scans. Good for very small sets
//!   or when inserts/updates vastly outnumber queries.
//! - `RTreeF32`/`RTreeF64`/`RTreeI64`: R-tree with SAH-like splits and widened metrics; good
//!   general-purpose index when distribution is irregular and updates are frequent.
//!   See the [`backends`] docs for a brief SAH overview.
//! - `BvhF32`/`BvhF64`/`BvhI64`: binary hierarchy with SAH-like splits; excels when bulk-build
//!   and query performance matter; updates are supported but may be costlier than R-tree.
//!
//! ### Float semantics
//!
//! This crate assumes no NaNs for floating-point coordinates. Debug builds may assert.
//! SAH metrics use widened accumulators to reduce precision pitfalls.

#![no_std]

extern crate alloc;

mod backend;
pub mod backends;
mod damage;
mod index;
mod types;
pub(crate) mod util;

pub use backend::Backend;
pub use damage::Damage;
pub use index::{Index, IndexGeneric, Key};
pub use types::{Aabb2D, Scalar, ScalarAcc};

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn insert_update_commit_and_query() {
        let mut idx: Index<i64, u32> = Index::new();
        let k1 = idx.insert(Aabb2D::new(0, 0, 10, 10), 1);
        let _ = idx.commit();
        idx.update(k1, Aabb2D::new(5, 5, 15, 15));
        let dmg = idx.commit();
        assert!(!dmg.is_empty());

        let hits: Vec<_> = idx.query_point(6, 6).collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1, 1);
    }

    #[test]
    fn added_then_removed_before_commit_is_ignored() {
        let mut idx: Index<i64, u32> = Index::new();
        let k = idx.insert(Aabb2D::new(0, 0, 10, 10), 1);
        idx.remove(k);
        let dmg = idx.commit();
        assert!(dmg.is_empty());
        assert_eq!(idx.query_point(1, 1).count(), 0);
    }

    #[test]
    fn removed_after_commit_reports_removed() {
        let mut idx: Index<i64, u32> = Index::new();
        let k = idx.insert(Aabb2D::new(0, 0, 10, 10), 1);
        let _ = idx.commit();
        idx.remove(k);
        let dmg = idx.commit();
        assert_eq!(dmg.removed.len(), 1);
        assert_eq!(dmg.added.len(), 0);
    }

    #[test]
    fn moved_reports_pair() {
        let mut idx: Index<i64, u32> = Index::new();
        let k = idx.insert(Aabb2D::new(0, 0, 10, 10), 1);
        let _ = idx.commit();
        idx.update(k, Aabb2D::new(5, 5, 15, 15));
        let dmg = idx.commit();
        assert_eq!(dmg.moved.len(), 1);
        let (a, b) = dmg.moved[0];
        assert_eq!(a, Aabb2D::new(0, 0, 10, 10));
        assert_eq!(b, Aabb2D::new(5, 5, 15, 15));
    }
}
