<div align="center">

# Understory Index

**Generic 2D AABB (boundary) index with pluggable backends**

[![Latest published version.](https://img.shields.io/crates/v/understory_index.svg)](https://crates.io/crates/understory_index)
[![Documentation build status.](https://img.shields.io/docsrs/understory_index.svg)](https://docs.rs/understory_index)
[![Apache 2.0 license.](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](#license)
\
[![GitHub Actions CI status.](https://img.shields.io/github/actions/workflow/status/endoli/understory/ci.yml?logo=github&label=CI)](https://github.com/endoli/understory/actions)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=understory_index
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- cargo-rdme start -->

Understory Index: a generic 2D AABB index (boundary index).

Understory Index is a reusable building block for spatial queries.

- Insert, update, and remove axis-aligned bounding boxes (AABBs) with user payloads.
- Query by point or intersecting rectangle.
- Batch updates with [`Index::commit`] and receive coarse damage (added/removed/moved boxes).

It is generic over the scalar type `T` and does not depend on any geometry crate.
Higher layers (like a scene or region tree) can compute world-space AABBs and feed them here.

Backends are pluggable via a simple trait so you can swap the spatial strategy without API churn.
The default backend is a flat vector (linear scan).
R-tree and BVH backends are generic over the scalar and use widened accumulator types (f32→f64, f64→f64, i64→i128) for SAH-like splits.

# Example

```rust
use understory_index::{Index, Aabb2D};

// Create an index and add two boxes.
let mut idx: Index<i64, u32> = Index::new();
let k1 = idx.insert(Aabb2D::new(0, 0, 10, 10), 1);
let k2 = idx.insert(Aabb2D::new(5, 5, 15, 15), 2);
let _damage0 = idx.commit();

// Move the first box and commit a damage set.
idx.update(k1, Aabb2D::new(20, 0, 30, 10));
let damage = idx.commit();
assert!(!damage.is_empty());

// Query a point inside the second box.
let hits: Vec<_> = idx.query_point(6, 6).collect();
assert_eq!(hits.len(), 1);
assert_eq!(hits[0].1, 2);
```

You can opt into a different backend when you need better query/update balance:

```rust
use understory_index::{Index, Aabb2D};

// Use an R-tree (f64) for indexing.
let mut idx = Index::<f64, u32>::with_rtree();
let _k = idx.insert(Aabb2D::new(0.0, 0.0, 100.0, 100.0), 1);
let _ = idx.commit();

// Query a point.
let hits: Vec<_> = idx.query_point(10.0, 10.0).collect();
assert_eq!(hits.len(), 1);
```

## Choosing a backend

- `FlatVec` (default): simplest and smallest, linear scans. Good for very small sets
  or when inserts/updates vastly outnumber queries.
- `RTreeF32`/`RTreeF64`/`RTreeI64`: R-tree with SAH-like splits and widened metrics; good
  general-purpose index when distribution is irregular and updates are frequent.
  See the [`backends`] docs for a brief SAH overview.
- `BVHF32`/`BVHF64`/`BVHI64`: binary hierarchy with SAH-like splits; excels when bulk-build
  and query performance matter; updates are supported but may be costlier than R-tree.

### Float semantics

This crate assumes no NaNs for floating-point coordinates. Debug builds may assert.
SAH metrics use widened accumulators to reduce precision pitfalls.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This version of Understory Index has been verified to compile with **Rust 1.88** and later.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE] or <http://www.apache.org/licenses/LICENSE-2.0>)

<!-- Needs to be defined here for rustdoc's benefit -->
[LICENSE]: LICENSE
