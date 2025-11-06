// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Public `Index` API and generic implementation over a pluggable backend.

use alloc::vec::Vec;
use core::fmt::Debug;

use crate::backend::Backend;
use crate::damage::Damage;
use crate::types::Aabb2D;

/// Generational handle for entries.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key(u32, u32);

impl Key {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Index keys are intentionally 32-bit; higher bits are truncated by design."
    )]
    const fn new(idx: usize, generation: u32) -> Self {
        Self(idx as u32, generation)
    }

    const fn idx(self) -> usize {
        self.0 as usize
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Mark {
    Added,
    Updated,
    Removed,
}

#[derive(Clone, Debug)]
struct Entry<T, P> {
    generation: u32,
    aabb: Aabb2D<T>,
    payload: P,
    mark: Option<Mark>,
    prev_aabb: Option<Aabb2D<T>>, // for moved damage
}

/// A generic AABB index parameterized by a spatial backend.
#[derive(Debug)]
pub struct IndexGeneric<T: Copy + PartialOrd + Debug, P: Copy + Debug, B: Backend<T>> {
    entries: Vec<Option<Entry<T, P>>>,
    free_list: Vec<usize>,
    backend: B,
}

impl<T, P, B> IndexGeneric<T, P, B>
where
    T: Copy + PartialOrd + Debug,
    P: Copy + Debug,
    B: Backend<T> + Default,
{
    /// Create an empty index using the backend's default constructor.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            free_list: Vec::new(),
            backend: B::default(),
        }
    }
}

impl<T, P, B> IndexGeneric<T, P, B>
where
    T: Copy + PartialOrd + Debug,
    P: Copy + Debug,
    B: Backend<T>,
{
    /// Reserve space for at least `n` entries.
    pub fn reserve(&mut self, n: usize) {
        self.entries.reserve(n);
    }

    /// Insert a new AABB with payload. Returns a stable handle `Key`.
    pub fn insert(&mut self, aabb: Aabb2D<T>, payload: P) -> Key {
        let (idx, generation) = if let Some(idx) = self.free_list.pop() {
            let generation = self.entries[idx]
                .as_ref()
                .map(|e| e.generation)
                .unwrap_or(0)
                + 1;
            self.entries[idx] = Some(Entry {
                generation,
                aabb,
                payload,
                mark: Some(Mark::Added),
                prev_aabb: None,
            });
            (idx, generation)
        } else {
            let generation = 1_u32;
            self.entries.push(Some(Entry {
                generation,
                aabb,
                payload,
                mark: Some(Mark::Added),
                prev_aabb: None,
            }));
            (self.entries.len() - 1, generation)
        };
        Key::new(idx, generation)
    }

    /// Update an existing AABB.
    pub fn update(&mut self, key: Key, aabb: Aabb2D<T>) {
        if let Some(e) = self.entry_mut(key) {
            if e.mark.is_none() {
                e.prev_aabb = Some(e.aabb);
            }
            e.aabb = aabb;
            e.mark = Some(match e.mark {
                Some(Mark::Added) => Mark::Added,
                _ => Mark::Updated,
            });
        }
    }

    /// Remove an existing AABB.
    pub fn remove(&mut self, key: Key) {
        if let Some(e) = self.entry_mut(key) {
            if matches!(e.mark, Some(Mark::Added)) {
                self.entries[key.idx()] = None;
                self.free_list.push(key.idx());
            } else {
                e.mark = Some(Mark::Removed);
            }
        }
    }

    /// Clear the index (without reporting damage).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.free_list.clear();
        self.backend.clear();
    }

    /// Apply pending changes and compute batched damage. Also synchronizes backend state.
    pub fn commit(&mut self) -> Damage<T> {
        let mut dmg = Damage::default();
        for i in 0..self.entries.len() {
            let Some(entry) = self.entries[i].as_mut() else {
                continue;
            };
            match entry.mark.take() {
                Some(Mark::Added) => {
                    self.backend.insert(i, entry.aabb);
                    dmg.added.push(entry.aabb);
                }
                Some(Mark::Removed) => {
                    self.backend.remove(i);
                    dmg.removed.push(entry.aabb);
                    let generation = entry.generation;
                    self.entries[i] = None;
                    self.free_list.push(i);
                    let _ = generation;
                }
                Some(Mark::Updated) => {
                    self.backend.update(i, entry.aabb);
                    if let Some(prev) = entry.prev_aabb.take()
                        && prev != entry.aabb
                    {
                        dmg.moved.push((prev, entry.aabb));
                    }
                }
                None => {}
            }
        }
        dmg
    }

    /// Query for entries whose AABB contains the point.
    pub fn query_point(&self, x: T, y: T) -> impl Iterator<Item = (Key, P)> + '_ {
        let mut out = Vec::new();
        self.visit_point(x, y, |k, p| out.push((k, p)));
        out.into_iter()
    }

    /// Visit entries whose AABB contains the point (does not allocate result storage).
    ///
    /// Calls `f(key, payload)` for each match. The order is backend-dependent.
    pub fn visit_point<F: FnMut(Key, P)>(&self, x: T, y: T, mut f: F) {
        self.backend.visit_point(x, y, |i| {
            if let Some(Some(e)) = self.entries.get(i) {
                f(Key::new(i, e.generation), e.payload);
            }
        });
    }

    /// Query for entries whose AABB intersects the given rectangle.
    pub fn query_rect(&self, rect: Aabb2D<T>) -> impl Iterator<Item = (Key, P)> + '_ {
        let mut out = Vec::new();
        self.visit_rect(rect, |k, p| out.push((k, p)));
        out.into_iter()
    }

    /// Visit entries whose AABB intersects the given rectangle (does not allocate result storage).
    ///
    /// Calls `f(key, payload)` for each match. The order is backend-dependent.
    pub fn visit_rect<F: FnMut(Key, P)>(&self, rect: Aabb2D<T>, mut f: F) {
        self.backend.visit_rect(rect, |i| {
            if let Some(Some(e)) = self.entries.get(i) {
                f(Key::new(i, e.generation), e.payload);
            }
        });
    }

    fn entry_mut(&mut self, key: Key) -> Option<&mut Entry<T, P>> {
        let e = self.entries.get_mut(key.idx())?.as_mut()?;
        if e.generation != key.1 {
            return None;
        }
        Some(e)
    }
}

// Debug is derived above; backends implement Debug with concise, partial output.

/// Default index using a flat vector backend.
pub type Index<T, P> = IndexGeneric<T, P, crate::backends::flatvec::FlatVec<T>>;

impl<T: Copy + PartialOrd + Debug, P: Copy + Debug> Default for Index<T, P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Copy + Debug> Index<f64, P> {
    /// Create a BVH-backed index using SAH-like splits.
    pub fn with_bvh() -> IndexGeneric<f64, P, crate::backends::bvh::BVHF64> {
        IndexGeneric {
            entries: Vec::new(),
            free_list: Vec::new(),
            backend: crate::backends::bvh::BVHF64::default(),
        }
    }

    /// Create an R-tree-backed index (f64 coordinates).
    pub fn with_rtree() -> IndexGeneric<f64, P, crate::backends::rtree::RTreeF64<P>> {
        IndexGeneric {
            entries: Vec::new(),
            free_list: Vec::new(),
            backend: crate::backends::rtree::RTreeF64::default(),
        }
    }

    /// Build an R-tree-backed index in bulk from entries.
    pub fn with_rtree_bulk(
        entries: &[(Aabb2D<f64>, P)],
    ) -> IndexGeneric<f64, P, crate::backends::rtree::RTreeF64<P>> {
        let mut idx = IndexGeneric {
            entries: Vec::with_capacity(entries.len()),
            free_list: Vec::new(),
            backend: crate::backends::rtree::RTreeF64::default(),
        };
        let mut pairs: Vec<(usize, Aabb2D<f64>)> = Vec::with_capacity(entries.len());
        for (i, (aabb, payload)) in entries.iter().copied().enumerate() {
            idx.entries.push(Some(Entry {
                generation: 1,
                aabb,
                payload,
                mark: None,
                prev_aabb: None,
            }));
            pairs.push((i, aabb));
        }
        idx.backend = crate::backends::rtree::RTreeF64::bulk_build_default(&pairs);
        idx
    }
}

impl<P: Copy + Debug> Index<i64, P> {
    /// Create an i64 R-tree-backed index using integer SAH splits.
    pub fn with_rtree() -> IndexGeneric<i64, P, crate::backends::rtree::RTreeI64<P>> {
        IndexGeneric {
            entries: Vec::new(),
            free_list: Vec::new(),
            backend: crate::backends::rtree::RTreeI64::default(),
        }
    }

    /// Build an i64 R-tree-backed index in bulk from entries.
    pub fn with_rtree_bulk(
        entries: &[(Aabb2D<i64>, P)],
    ) -> IndexGeneric<i64, P, crate::backends::rtree::RTreeI64<P>> {
        let mut idx = IndexGeneric {
            entries: Vec::with_capacity(entries.len()),
            free_list: Vec::new(),
            backend: crate::backends::rtree::RTreeI64::default(),
        };
        let mut pairs: Vec<(usize, Aabb2D<i64>)> = Vec::with_capacity(entries.len());
        for (i, (aabb, payload)) in entries.iter().copied().enumerate() {
            idx.entries.push(Some(Entry {
                generation: 1,
                aabb,
                payload,
                mark: None,
                prev_aabb: None,
            }));
            pairs.push((i, aabb));
        }
        idx.backend = crate::backends::rtree::RTreeI64::bulk_build_default(&pairs);
        idx
    }
}

impl<P: Copy + Debug> Index<f32, P> {
    /// Create a BVH-backed index (f32 coordinates).
    pub fn with_bvh() -> IndexGeneric<f32, P, crate::backends::bvh::BVHF32> {
        IndexGeneric {
            entries: Vec::new(),
            free_list: Vec::new(),
            backend: crate::backends::bvh::BVHF32::default(),
        }
    }

    /// Create an R-tree-backed index (f32 coordinates).
    pub fn with_rtree() -> IndexGeneric<f32, P, crate::backends::rtree::RTreeF32<P>> {
        IndexGeneric {
            entries: Vec::new(),
            free_list: Vec::new(),
            backend: crate::backends::rtree::RTreeF32::default(),
        }
    }

    /// Build an f32 R-tree-backed index in bulk from entries.
    pub fn with_rtree_bulk(
        entries: &[(Aabb2D<f32>, P)],
    ) -> IndexGeneric<f32, P, crate::backends::rtree::RTreeF32<P>> {
        let mut idx = IndexGeneric {
            entries: Vec::with_capacity(entries.len()),
            free_list: Vec::new(),
            backend: crate::backends::rtree::RTreeF32::default(),
        };
        let mut pairs: Vec<(usize, Aabb2D<f32>)> = Vec::with_capacity(entries.len());
        for (i, (aabb, payload)) in entries.iter().copied().enumerate() {
            idx.entries.push(Some(Entry {
                generation: 1,
                aabb,
                payload,
                mark: None,
                prev_aabb: None,
            }));
            pairs.push((i, aabb));
        }
        idx.backend = crate::backends::rtree::RTreeF32::bulk_build_default(&pairs);
        idx
    }
}

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

    #[test]
    fn visit_point_and_rect_match_query_counts() {
        let mut idx: Index<i64, u32> = Index::new();
        let _k1 = idx.insert(Aabb2D::new(0, 0, 10, 10), 1);
        let _k2 = idx.insert(Aabb2D::new(5, 5, 15, 15), 2);
        let _ = idx.commit();

        let it_count = idx.query_point(6, 6).count();
        let mut visit_count = 0;
        idx.visit_point(6, 6, |_k, _p| visit_count += 1);
        assert_eq!(visit_count, it_count);

        let r = Aabb2D::new(8, 8, 12, 12);
        let it_count_r = idx.query_rect(r).count();
        let mut visit_count_r = 0;
        idx.visit_rect(r, |_k, _p| visit_count_r += 1);
        assert_eq!(visit_count_r, it_count_r);
    }
}
