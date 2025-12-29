// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Uniform grid backend for 2D AABBs.
//!
//! This backend buckets AABBs into fixed-size grid cells and answers queries
//! by touching only the cells overlapping the query primitive. It is intended
//! for workloads with:
//! - moderately uniform spatial density (e.g., viewports, UI hit-testing),
//! - dynamic updates, and
//! - query rectangles that are small compared to the full world extent.

use alloc::vec::Vec;
use core::fmt::Debug;

use hashbrown::{HashMap, HashSet};
use smallvec::SmallVec;

use crate::backend::Backend;
use crate::types::{Aabb2D, Scalar};

/// Scalar types supported by the grid backend.
///
/// This is kept separate from [`Scalar`] so that the grid implementation can
/// use type-specific logic (e.g., Euclidean division for integers).
pub trait GridScalar: Scalar {
    /// Map a scalar coordinate to a grid coordinate along one axis.
    ///
    /// The mapping is based on an origin and uniform cell size. Implementations
    /// are expected to be monotonic in `value` for fixed `origin` and
    /// `cell_size`.
    fn cell_coord(value: Self, origin: Self, cell_size: Self) -> i32;
}

impl GridScalar for f32 {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Grid cell indices are intentionally i32; out-of-range values are saturated."
    )]
    #[inline]
    fn cell_coord(value: Self, origin: Self, cell_size: Self) -> i32 {
        debug_assert!(
            cell_size > 0.0,
            "grid cell_size must be strictly positive (f32)"
        );
        let t = (value - origin) / cell_size;
        let coord = t as i32;

        // Round towards -∞ (the cast above has already truncated).
        if t < 0.0 && (coord as Self) > t {
            coord.saturating_sub(1)
        } else {
            coord
        }
    }
}

impl GridScalar for f64 {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Grid cell indices are intentionally i32; out-of-range values are saturated."
    )]
    #[inline]
    fn cell_coord(value: Self, origin: Self, cell_size: Self) -> i32 {
        debug_assert!(
            cell_size > 0.0,
            "grid cell_size must be strictly positive (f64)"
        );
        let t = (value - origin) / cell_size;
        let coord = t as i32;

        // Round towards -∞ (the cast above has already truncated).
        if t < 0.0 && (coord as Self) > t {
            coord.saturating_sub(1)
        } else {
            coord
        }
    }
}

impl GridScalar for i64 {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Grid cell indices are intentionally i32; out-of-range values are saturated."
    )]
    #[inline]
    fn cell_coord(value: Self, origin: Self, cell_size: Self) -> i32 {
        debug_assert!(
            cell_size > 0,
            "grid cell_size must be strictly positive (i64)"
        );
        let rel = value - origin;
        // Euclidean division rounds toward -∞, which matches floor for all
        // integer values.
        let coord = rel.div_euclid(cell_size);

        // Saturate values out of `i32` range.
        if coord >= Self::from(i32::MAX) {
            i32::MAX
        } else if coord <= Self::from(i32::MIN) {
            i32::MIN
        } else {
            coord as i32
        }
    }
}

/// Uniform grid backend with fixed cell size.
pub struct Grid<T: GridScalar> {
    cell_size: T,
    origin_x: T,
    origin_y: T,
    cells: HashMap<(i32, i32), Cell>,
    slots: Vec<Option<SlotEntry<T>>>,
}

#[derive(Clone, Debug)]
struct SlotEntry<T: GridScalar> {
    aabb: Aabb2D<T>,
    // Cells currently containing this AABB.
    cells: SmallVec<[(i32, i32); 4]>,
}

#[derive(Default)]
struct Cell {
    slots: SmallVec<[usize; 8]>,
}

impl<T: GridScalar> Debug for Grid<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let total_slots = self.slots.len();
        let live_slots = self.slots.iter().filter(|s| s.is_some()).count();
        let num_cells = self.cells.len();
        f.debug_struct("Grid")
            .field("cell_size", &self.cell_size)
            .field("origin_x", &self.origin_x)
            .field("origin_y", &self.origin_y)
            .field("total_slots", &total_slots)
            .field("live_slots", &live_slots)
            .field("cells", &num_cells)
            .finish_non_exhaustive()
    }
}

impl<T: GridScalar> Grid<T> {
    /// Create a new grid backend with the given cell size and origin at (0, 0).
    pub fn new(cell_size: T) -> Self {
        debug_assert!(cell_size > T::zero(), "cell_size must be strictly positive");
        Self {
            cell_size,
            origin_x: T::zero(),
            origin_y: T::zero(),
            cells: HashMap::new(),
            slots: Vec::new(),
        }
    }

    /// Create a new grid backend with the given cell size and origin.
    pub fn with_origin(cell_size: T, origin_x: T, origin_y: T) -> Self {
        debug_assert!(cell_size > T::zero(), "cell_size must be strictly positive");
        Self {
            cell_size,
            origin_x,
            origin_y,
            cells: HashMap::new(),
            slots: Vec::new(),
        }
    }

    fn ensure_slot(&mut self, slot: usize) {
        if self.slots.len() <= slot {
            self.slots.resize_with(slot + 1, || None);
        }
    }

    fn slot_entry(&self, slot: usize) -> &SlotEntry<T> {
        self.slots
            .get(slot)
            .expect("grid invariant violated: cell references out-of-bounds slot")
            .as_ref()
            .expect("grid invariant violated: cell references vacant slot")
    }

    fn remove_from_cells(&mut self, slot: usize, cells: &[(i32, i32)]) {
        for &(ix, iy) in cells {
            let cell = self
                .cells
                .get_mut(&(ix, iy))
                .expect("grid invariant violated: missing cell while removing slot");

            let pos = cell
                .slots
                .iter()
                .position(|&s| s == slot)
                .expect("grid invariant violated: slot not found in expected cell");
            cell.slots.swap_remove(pos);

            if cell.slots.is_empty() {
                // Dropping empty cells keeps the map compact for sparse grids.
                self.cells.remove(&(ix, iy));
            }
        }
    }

    fn cell_range(&self, min: T, max: T, origin: T) -> (i32, i32) {
        let c0 = T::cell_coord(min, origin, self.cell_size);
        let c1 = T::cell_coord(max, origin, self.cell_size);
        if c0 <= c1 { (c0, c1) } else { (c1, c0) }
    }

    fn covered_cells(&self, aabb: &Aabb2D<T>) -> SmallVec<[(i32, i32); 4]> {
        let (ix0, ix1) = self.cell_range(aabb.min_x, aabb.max_x, self.origin_x);
        let (iy0, iy1) = self.cell_range(aabb.min_y, aabb.max_y, self.origin_y);
        let mut out: SmallVec<[(i32, i32); 4]> = SmallVec::new();
        for ix in ix0..=ix1 {
            for iy in iy0..=iy1 {
                out.push((ix, iy));
            }
        }
        out
    }
}

impl<T: GridScalar> Backend<T> for Grid<T> {
    fn insert(&mut self, slot: usize, aabb: Aabb2D<T>) {
        self.ensure_slot(slot);

        // If this slot was previously used, clean up its old cell memberships.
        if let Some(old) = self.slots[slot].take() {
            self.remove_from_cells(slot, &old.cells);
        }

        let cells = self.covered_cells(&aabb);
        for &(ix, iy) in &cells {
            self.cells.entry((ix, iy)).or_default().slots.push(slot);
        }
        self.slots[slot] = Some(SlotEntry { aabb, cells });
    }

    fn update(&mut self, slot: usize, aabb: Aabb2D<T>) {
        // Take the current entry out to avoid aliasing `self` while mutating
        // grid cells.
        let current = if let Some(slot_ref) = self.slots.get_mut(slot) {
            slot_ref.take()
        } else {
            None
        };

        let Some(mut entry) = current else {
            // If the slot does not exist, treat this as an insert.
            self.insert(slot, aabb);
            return;
        };

        // If the AABB is unchanged, restore the entry and skip work.
        if entry.aabb == aabb {
            self.slots[slot] = Some(entry);
            return;
        }

        // Remove from old cells.
        self.remove_from_cells(slot, &entry.cells);

        // Insert into new cells.
        let cells = self.covered_cells(&aabb);
        for &(ix, iy) in &cells {
            self.cells.entry((ix, iy)).or_default().slots.push(slot);
        }
        entry.aabb = aabb;
        entry.cells = cells;
        self.slots[slot] = Some(entry);
    }

    fn remove(&mut self, slot: usize) {
        if slot >= self.slots.len() {
            return;
        }
        if let Some(entry) = self.slots[slot].take() {
            self.remove_from_cells(slot, &entry.cells);
        }
    }

    fn clear(&mut self) {
        self.cells.clear();
        self.slots.clear();
    }

    fn visit_point<F: FnMut(usize)>(&self, x: T, y: T, mut f: F) {
        let ix = T::cell_coord(x, self.origin_x, self.cell_size);
        let iy = T::cell_coord(y, self.origin_y, self.cell_size);
        if let Some(cell) = self.cells.get(&(ix, iy)) {
            for &slot in &cell.slots {
                let entry = self.slot_entry(slot);
                if entry.aabb.contains_point(x, y) {
                    f(slot);
                }
            }
        }
    }

    fn visit_rect<F: FnMut(usize)>(&self, rect: Aabb2D<T>, mut f: F) {
        let (ix0, ix1) = self.cell_range(rect.min_x, rect.max_x, self.origin_x);
        let (iy0, iy1) = self.cell_range(rect.min_y, rect.max_y, self.origin_y);

        let mut seen: HashSet<usize> = HashSet::new();

        for ix in ix0..=ix1 {
            for iy in iy0..=iy1 {
                if let Some(cell) = self.cells.get(&(ix, iy)) {
                    for &slot in &cell.slots {
                        if !seen.insert(slot) {
                            continue;
                        }
                        let entry = self.slot_entry(slot);
                        if entry.aabb.overlaps(&rect) {
                            f(slot);
                        }
                    }
                }
            }
        }
    }
}

/// Grid backend over `f32` coordinates.
pub type GridF32 = Grid<f32>;
/// Grid backend over `f64` coordinates.
pub type GridF64 = Grid<f64>;
/// Grid backend over `i64` coordinates.
pub type GridI64 = Grid<i64>;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn insert_update_remove_roundtrip_f32() {
        let mut grid: GridF32 = GridF32::new(10.0);

        let a = Aabb2D::new(0.0, 0.0, 10.0, 10.0);
        grid.insert(0, a);

        // Point in the AABB should hit slot 0.
        let mut hits = Vec::new();
        grid.visit_point(5.0, 5.0, |s| hits.push(s));
        assert_eq!(hits, vec![0]);

        // Move the AABB; point should follow.
        let b = Aabb2D::new(20.0, 20.0, 30.0, 30.0);
        grid.update(0, b);

        hits.clear();
        grid.visit_point(5.0, 5.0, |s| hits.push(s));
        assert!(hits.is_empty());

        hits.clear();
        grid.visit_point(25.0, 25.0, |s| hits.push(s));
        assert_eq!(hits, vec![0]);

        // Remove and ensure no hits.
        grid.remove(0);
        hits.clear();
        grid.visit_point(25.0, 25.0, |s| hits.push(s));
        assert!(hits.is_empty());
    }

    #[test]
    fn rect_query_deduplicates_slots() {
        let mut grid: GridF32 = GridF32::new(5.0);

        // This AABB spans multiple cells.
        let a = Aabb2D::new(0.0, 0.0, 20.0, 20.0);
        grid.insert(1, a);

        let rect = Aabb2D::new(2.0, 2.0, 18.0, 18.0);
        let mut hits = Vec::new();
        grid.visit_rect(rect, |s| hits.push(s));

        // Slot 1 should be reported exactly once.
        assert_eq!(hits, vec![1]);
    }

    #[test]
    fn update_missing_slot_inserts() {
        let mut grid: GridF32 = GridF32::new(10.0);

        // Updating an unused slot should behave like insert.
        let a = Aabb2D::new(0.0, 0.0, 10.0, 10.0);
        grid.update(5, a);

        let mut hits = Vec::new();
        grid.visit_point(5.0, 5.0, |s| hits.push(s));
        assert_eq!(hits, vec![5]);
    }

    #[test]
    fn basic_f64_and_i64_grids() {
        // f64 grid with negative coordinates.
        let mut g64: GridF64 = GridF64::new(10.0);
        let a = Aabb2D::new(-25.0, -25.0, -5.0, -5.0);
        g64.insert(0, a);
        let mut hits = Vec::new();
        g64.visit_point(-10.0, -10.0, |s| hits.push(s));
        assert_eq!(hits, vec![0]);

        // i64 grid with negative coordinates.
        let mut g_i64: GridI64 = GridI64::new(10);
        let b = Aabb2D::new(-30, -30, -10, -10);
        g_i64.insert(3, b);
        hits.clear();
        g_i64.visit_point(-20, -20, |s| hits.push(s));
        assert_eq!(hits, vec![3]);
    }

    #[test]
    fn cell_coord_saturates() {
        assert_eq!(GridScalar::cell_coord(1e20_f32, 0.0, 1.0), i32::MAX);
        assert_eq!(GridScalar::cell_coord(-1e20_f32, 0.0, 1.0), i32::MIN);
        assert_eq!(GridScalar::cell_coord(1e20_f64, 0.0, 1.0), i32::MAX);
        assert_eq!(GridScalar::cell_coord(-1e20_f64, 0.0, 1.0), i32::MIN);
    }

    #[test]
    fn cell_coord_is_monotonic_f32() {
        // An `f32` cannot represent integers precisely near the edges of the `i32` range. We
        // expect `cell_coord` to be *strictly* monotonic there.
        let value = i32::MIN as f32;
        assert_eq!(
            GridScalar::cell_coord(value.next_down(), 0.0, 1.0),
            i32::MIN
        );
        assert_eq!(GridScalar::cell_coord(value, 0.0, 1.0), i32::MIN);
        assert!(GridScalar::cell_coord(value.next_up(), 0.0, 1.0) > i32::MIN);
        assert!(
            GridScalar::cell_coord(value.next_up().next_up(), 0.0, 1.0)
                > GridScalar::cell_coord(value.next_up(), 0.0, 1.0)
        );

        let value = i32::MAX as f32;
        assert_eq!(GridScalar::cell_coord(value.next_up(), 0.0, 1.0), i32::MAX);
        assert_eq!(GridScalar::cell_coord(value, 0.0, 1.0), i32::MAX);
        assert!(GridScalar::cell_coord(value.next_down(), 0.0, 1.0) < i32::MAX);
        assert!(
            GridScalar::cell_coord(value.next_down().next_down(), 0.0, 1.0)
                < GridScalar::cell_coord(value.next_down(), 0.0, 1.0)
        );

        // But around -1, 0, 1, etc we just expect monotonicity.
        for value in [-1_f32, 0., 1.] {
            assert!(
                GridScalar::cell_coord(value.next_down(), 0.0, 1.0)
                    <= GridScalar::cell_coord(value, 0.0, 1.0)
            );
            assert!(
                GridScalar::cell_coord(value, 0.0, 1.0)
                    <= GridScalar::cell_coord(value.next_up(), 0.0, 1.0)
            );
        }
    }

    #[test]
    fn cell_coord_is_monotonic_f64() {
        // All integers in range of an `i32` can be represented exactly by `f64`. We expect all
        // `cell_coord` to be monotonic in `value`, including at the extremes of what an `i32` can
        // represent.
        for value in [i32::MIN as f64, -1., 0., 1., i32::MAX as f64] {
            assert!(
                GridScalar::cell_coord(value.next_down(), 0.0, 1.0)
                    <= GridScalar::cell_coord(value, 0.0, 1.0)
            );
            assert!(
                GridScalar::cell_coord(value, 0.0, 1.0)
                    <= GridScalar::cell_coord(value.next_up(), 0.0, 1.0)
            );
        }
    }
}
