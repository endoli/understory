// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Backend implementations for different spatial strategies.
//!
//! - `flatvec`: flat vector with linear scans (small, simple).
//! - `rtree`: generic R-tree (`T: Scalar`) with SAH-like split (aliases: `RTreeI64`, `RTreeF32`, `RTreeF64`).
//! - `bvh`: generic BVH (`T: Scalar`) with SAH-like split (aliases: `BvhF32`, `BvhF64`, `BvhI64`).
//!
//! SAH note
//! --------
//! R-tree and BVH use an SAH-like split heuristic.
//! For a split point `k` along a sorted axis we minimize:
//!
//! `cost(k) = area(LB_k) * k + area(RB_k) * (n - k)`
//!
//! where `LB_k` and `RB_k` are the bounding boxes of the first `k` and remaining `n - k` items.
//! We evaluate all `k` in O(n) per axis using prefix/suffix bounding boxes, and pick the lowest cost.
//! Accumulators are widened (`f32`→`f64`, `f64`→`f64`, `i64`→`i128`) for robust comparisons.
//! Bulk builders use an STR-like pass to seed packed leaves and parents.

pub(crate) mod bvh;
pub(crate) mod flatvec;
pub(crate) mod rtree;

pub use bvh::{Bvh, BvhF32, BvhF64, BvhI64};
pub use flatvec::FlatVec;
pub use rtree::{RTree, RTreeF32, RTreeF64, RTreeI64};
