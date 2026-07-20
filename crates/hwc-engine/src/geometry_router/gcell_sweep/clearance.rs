//! Clearance computation helpers for DRC sweep.

use crate::geometry_router::spatial_index::IndexedSegment;

use super::sweep::segment_bbox;
use super::sweep::SegmentBbox;

/// Compute the actual edge-to-edge clearance between two Manhattan axis-aligned segments.
///
/// Uses perpendicular distance for parallel segments and minimum component
/// distance for crossing segments.
#[inline]
pub fn compute_actual_clearance(seg_a: &IndexedSegment, seg_b: &IndexedSegment) -> i64 {
    let center_a = seg_a.center();
    let center_b = seg_b.center();

    let dx = (center_a.x - center_b.x).abs();
    let dy = (center_a.y - center_b.y).abs();

    let half_a = seg_a.width_nm / 2;
    let half_b = seg_b.width_nm / 2;

    let a_horiz = seg_a.start.y == seg_a.end.y;
    let b_horiz = seg_b.start.y == seg_b.end.y;

    let perp_dist = if a_horiz && b_horiz {
        dy
    } else if !a_horiz && !b_horiz {
        dx
    } else {
        dx.min(dy)
    };

    perp_dist - half_a - half_b
}

/// Compute the approximate overlap area of two segment bounding boxes.
#[inline]
pub fn compute_overlap_area(seg_a: &IndexedSegment, seg_b: &IndexedSegment) -> i64 {
    let a = segment_bbox(seg_a);
    let b = segment_bbox(seg_b);

    let overlap_w = (a.max_x.min(b.max_x) - a.min_x.max(b.min_x)).max(0);
    let overlap_h = (a.max_y.min(b.max_y) - a.min_y.max(b.min_y)).max(0);
    overlap_w * overlap_h
}

/// v0.1.8: Compute the intersection area of two AABBs (axis-aligned bounding boxes).
/// Returns 0 if the boxes don't overlap.
#[inline]
pub fn compute_bbox_intersection_area(a: &SegmentBbox, b: &SegmentBbox) -> i64 {
    let overlap_w = (a.max_x.min(b.max_x) - a.min_x.max(b.min_x)).max(0);
    let overlap_h = (a.max_y.min(b.max_y) - a.min_y.max(b.min_y)).max(0);
    overlap_w * overlap_h
}
