// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Diffing and resource snapshot utilities for display lists.

use alloc::vec::Vec;

use crate::{DisplayList, Op, OpHeader, OpId};

/// Diff between two display lists.
///
/// This is computed purely from op ids and content; it does not attempt to
/// interpret semantics or geometry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diff {
    /// Ops that are present in the new list but not in the old.
    pub inserted: Vec<OpId>,
    /// Ops that are present in the old list but not in the new.
    pub removed: Vec<OpId>,
    /// Ops whose ids are shared but whose content has changed.
    pub replaced: Vec<OpId>,
    /// Ops whose order within the list has changed.
    pub moved: Vec<OpId>,
}

/// Compute a diff from `old` to `new` using `OpId`s and per-op content.
pub fn diff(old: &DisplayList, new: &DisplayList) -> Diff {
    use alloc::collections::BTreeMap;

    let mut old_map: BTreeMap<OpId, usize> = BTreeMap::new();
    for (i, op) in old.ops.iter().enumerate() {
        old_map.insert(op.header().id, i);
    }

    let mut new_map: BTreeMap<OpId, usize> = BTreeMap::new();
    for (i, op) in new.ops.iter().enumerate() {
        new_map.insert(op.header().id, i);
    }

    let mut inserted = Vec::new();
    let mut removed = Vec::new();
    let mut replaced = Vec::new();
    let mut moved = Vec::new();

    // Inserted + replaced + moved.
    for (new_idx, new_op) in new.ops.iter().enumerate() {
        let id = new_op.header().id;
        match old_map.get(&id) {
            None => inserted.push(id),
            Some(&old_idx) => {
                let old_op = &old.ops[old_idx];
                if !ops_equal_ignoring_id(old_op, new_op) {
                    replaced.push(id);
                } else if old_idx != new_idx {
                    moved.push(id);
                }
            }
        }
    }

    // Removed.
    for old_op in &old.ops {
        let id = old_op.header().id;
        if !new_map.contains_key(&id) {
            removed.push(id);
        }
    }

    Diff {
        inserted,
        removed,
        replaced,
        moved,
    }
}

fn ops_equal_ignoring_id(a: &Op, b: &Op) -> bool {
    use Op::*;
    match (a, b) {
        (
            FillPath {
                header: ha,
                path: pa,
                paint: pna,
            },
            FillPath {
                header: hb,
                path: pb,
                paint: pnb,
            },
        ) => headers_equal_ignoring_id(ha, hb) && pa == pb && pna == pnb,
        (
            StrokePath {
                header: ha,
                path: pa,
                stroke: sa,
                paint: pna,
            },
            StrokePath {
                header: hb,
                path: pb,
                stroke: sb,
                paint: pnb,
            },
        ) => headers_equal_ignoring_id(ha, hb) && pa == pb && sa == sb && pna == pnb,
        (
            GlyphRun {
                header: ha,
                run: ra,
                origin: oa,
                paint: pna,
            },
            GlyphRun {
                header: hb,
                run: rb,
                origin: ob,
                paint: pnb,
            },
        ) => headers_equal_ignoring_id(ha, hb) && ra == rb && oa == ob && pna == pnb,
        (
            Image {
                header: ha,
                image: ia,
                dest: da,
                opacity: oa,
            },
            Image {
                header: hb,
                image: ib,
                dest: db,
                opacity: ob,
            },
        ) => headers_equal_ignoring_id(ha, hb) && ia == ib && da == db && float_eq(*oa, *ob),
        (
            PushClip {
                header: ha,
                clip: ca,
            },
            PushClip {
                header: hb,
                clip: cb,
            },
        ) => headers_equal_ignoring_id(ha, hb) && ca == cb,
        (PopClip { header: ha }, PopClip { header: hb }) => headers_equal_ignoring_id(ha, hb),
        (
            Group {
                header: ha,
                opacity: oa,
                blend: ba,
            },
            Group {
                header: hb,
                opacity: ob,
                blend: bb,
            },
        ) => headers_equal_ignoring_id(ha, hb) && float_eq(*oa, *ob) && ba == bb,
        _ => false,
    }
}

fn headers_equal_ignoring_id(a: &OpHeader, b: &OpHeader) -> bool {
    a.group == b.group && a.z == b.z && a.semantic == b.semantic && a.bounds == b.bounds
}

fn float_eq(a: f32, b: f32) -> bool {
    const EPS: f32 = 1e-6;
    (a - b).abs() <= EPS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisplayListBuilder, GroupId, ImageId, Op, OpHeader, PaintId, PathId};
    use kurbo::Rect;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect::new(x0, y0, x1, y1)
    }

    #[test]
    fn diff_insert_remove_replace_move() {
        let mut b1 = DisplayListBuilder::new(GroupId(1));
        // shared op (same content)
        b1.push_fill_path(0, rect(0.0, 0.0, 10.0, 10.0), PathId(1), PaintId(1), None);
        // will be removed
        b1.push_image(0, rect(10.0, 10.0, 20.0, 20.0), ImageId(1), 1.0, None);
        let list1 = b1.finish();

        let mut b2 = DisplayListBuilder::new(GroupId(1));
        // same fill, ids will be reallocated so we simulate stable ids by copying
        let shared = &list1.ops[0];
        let shared_header = *shared.header();
        b2.next_id = shared_header.id.0 + 1;
        b2.ops.push(Op::FillPath {
            header: shared_header,
            path: PathId(1),
            paint: PaintId(1),
        });

        // replaced image (same id, different dest)
        let old_image = &list1.ops[1];
        let old_header = *old_image.header();
        b2.ops.push(Op::Image {
            header: old_header,
            image: ImageId(1),
            dest: rect(30.0, 30.0, 40.0, 40.0),
            opacity: 1.0,
        });

        // inserted new op
        let inserted_header = OpHeader {
            id: OpId(99),
            group: GroupId(1),
            z: 0,
            semantic: None,
            bounds: rect(0.0, 0.0, 1.0, 1.0),
        };
        b2.ops.push(Op::PopClip {
            header: inserted_header,
        });

        // move: simulate by swapping order of shared and image
        b2.ops.swap(0, 1);

        let list2 = b2.finish();

        let d = diff(&list1, &list2);
        assert!(d.inserted.contains(&OpId(99)));
        assert!(d.removed.is_empty());
        assert!(d.replaced.contains(&old_header.id));
        assert!(d.moved.contains(&shared_header.id));
    }
}
