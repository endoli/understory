// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Helpers for integrating 2D precise hit tests with the responder.
//!
//! This adapter is intentionally minimal: it bridges geometry-level
//! [`understory_precise_hit::PreciseHitTest`] implementations with the responder's
//! [`ResolvedHit`] type, without imposing a specific tree or index.

use alloc::vec::Vec;

use kurbo::Point;
use understory_precise_hit::{HitParams, PreciseHitTest};

use crate::types::{DepthKey, Localizer, ResolvedHit};

/// Narrow-phase hit result for a single key.
///
/// The generic `K` is typically your node or widget identifier.
#[derive(Clone, Debug)]
pub struct KeyHit<K> {
    /// The key that was hit (e.g., a `NodeId`).
    pub key: K,
    /// Distance score from the precise hit test.
    pub distance: f64,
}

/// Convert a list of precise key hits into resolver hits for the router.
///
/// This helper is agnostic to any particular tree; it assumes the caller has
/// already performed broad-phase culling and precise tests and simply wants
/// a convenient way to produce `ResolvedHit` values.
pub fn resolved_hits_from_precise<K>(hits: &[KeyHit<K>]) -> Vec<ResolvedHit<K, ()>>
where
    K: Copy,
{
    hits.iter()
        .map(|h| ResolvedHit {
            node: h.key,
            path: None,
            #[allow(
                clippy::cast_possible_truncation,
                reason = "DepthKey uses f32; precision loss is acceptable for hit sorting."
            )]
            depth_key: DepthKey::Distance(h.distance as f32),
            localizer: Localizer::default(),
            meta: (),
        })
        .collect()
}

/// Run precise hit tests for a collection of shaped keys.
///
/// The caller supplies an iterator of `(key, shape)` pairs, where each shape
/// implements [`PreciseHitTest`] in its local coordinate space and the
/// provided `local_point` is already transformed into that space.
pub fn precise_hits_for_point<K, S, I>(
    candidates: I,
    local_point: Point,
    params: HitParams,
) -> Vec<KeyHit<K>>
where
    K: Copy,
    S: PreciseHitTest,
    I: IntoIterator<Item = (K, S)>,
{
    let mut hits = Vec::new();
    for (key, shape) in candidates {
        if let Some(score) = shape.hit_test_local(local_point, &params) {
            hits.push(KeyHit {
                key,
                distance: score.distance,
            });
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use understory_precise_hit::{HitKind, HitScore};

    #[derive(Clone, Copy)]
    struct TestShape(bool);

    impl PreciseHitTest for TestShape {
        fn hit_test_local(&self, _pt: Point, _params: &HitParams) -> Option<HitScore> {
            if self.0 {
                Some(HitScore {
                    distance: 1.0,
                    kind: HitKind::Fill,
                })
            } else {
                None
            }
        }
    }

    #[test]
    fn empty_precise_hits() {
        let hits: Vec<KeyHit<u32>> = precise_hits_for_point::<u32, TestShape, _>(
            [(1, TestShape(false))],
            Point::new(0.0, 0.0),
            HitParams::default(),
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn resolved_hits_distance_mapping() {
        let key_hits = vec![
            KeyHit {
                key: 1_u32,
                distance: 2.0,
            },
            KeyHit {
                key: 2_u32,
                distance: 0.5,
            },
        ];
        let resolved = resolved_hits_from_precise(&key_hits);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].node, 1);
        assert!(matches!(resolved[0].depth_key, DepthKey::Distance(d) if (d - 2.0).abs() < 1e-6));
    }
}

// Integration-style test exercising a full broad → narrow flow with a box tree.
//
// This models the "coarse hit, precise miss" scenario: the box tree's AABB
// contains the point, but the precise geometry test rejects it, so no hits
// are produced for routing.
#[cfg(all(test, feature = "box_tree_adapter"))]
mod integration_tests {
    use super::*;
    use alloc::vec;
    use kurbo::{Circle, Rect};
    use understory_box_tree::{LocalNode, NodeFlags, QueryFilter, Tree};

    #[test]
    fn coarse_hit_rejected_by_precise_test() {
        let mut tree = Tree::new();

        // Single node whose AABB contains the query point.
        let node = tree.insert(
            None,
            LocalNode {
                local_bounds: Rect::new(-10.0, -10.0, 10.0, 10.0),
                flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
                ..Default::default()
            },
        );
        let _damage = tree.commit();

        // Broad phase: point lies inside the node's bounds.
        let pt = Point::new(9.0, 0.0);
        let filter = QueryFilter::new().visible().pickable();
        let candidates: Vec<_> = tree
            .intersect_rect(Rect::from_points(pt, pt), filter)
            .collect();
        assert_eq!(candidates, vec![node]);

        // Narrow phase: circle is strictly inside the rect; the query point is
        // inside the rect AABB but outside the circle, so `hit_test_local` must
        // return `None`.
        let circle = Circle::new((0.0, 0.0), 5.0);
        let local_pt = pt;
        let params = HitParams::default();

        // Direct check on the precise test.
        assert!(
            circle.hit_test_local(local_pt, &params).is_none(),
            "point should be outside the precise geometry"
        );

        // Adapter flow: feed the same candidate through `precise_hits_for_point`.
        let key_hits: Vec<KeyHit<_>> = precise_hits_for_point([(node, circle)], local_pt, params);
        assert!(
            key_hits.is_empty(),
            "coarse hit should be filtered out by precise hit test"
        );

        // Downstream `ResolvedHit` list is also empty.
        let resolved = resolved_hits_from_precise(&key_hits);
        assert!(resolved.is_empty());
    }
}
