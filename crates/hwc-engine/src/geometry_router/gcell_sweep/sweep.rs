//! Flat active interval sweep for AABB overlap detection.

use crate::geometry_router::spatial_index::IndexedSegment;

/// Width-inflated bounding box for a segment (i64 coordinates only).
#[derive(Clone, Copy, Debug)]
pub struct SegmentBbox {
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
    pub segment_id: usize,
}

/// Compute the width-inflated bounding box for a segment.
#[inline]
pub fn segment_bbox(seg: &IndexedSegment) -> SegmentBbox {
    let half_w = seg.width_nm / 2;
    SegmentBbox {
        min_x: seg.start.x.min(seg.end.x) - half_w,
        min_y: seg.start.y.min(seg.end.y) - half_w,
        max_x: seg.start.x.max(seg.end.x) + half_w,
        max_y: seg.start.y.max(seg.end.y) + half_w,
        segment_id: seg.segment_id,
    }
}

/// Sweep event type: segment entering or leaving the active set.
#[derive(Clone, Debug)]
enum SweepEvent {
    Start { segment_id: usize, y: i64 },
    End { segment_id: usize, y: i64 },
}

/// Flat active interval sweep — no BST, no pointer chasing.
///
/// Vertical sweep-line along the Y-axis with a flat `Vec<usize>` of active
/// segment indices. When a new segment enters the active set, its X-range
/// is checked against all currently active segments for AABB overlap.
/// Complexity: O(N log N + K) where K = number of overlaps.
pub struct FlatIntervalSweep {
    events: Vec<SweepEvent>,
    active: Vec<usize>,
}

impl FlatIntervalSweep {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            active: Vec::new(),
        }
    }

    /// Run the sweep and return all (segment_id_a, segment_id_b) pairs
    /// whose width-inflated bounding boxes overlap.
    pub fn sweep(&mut self, bboxes: &[SegmentBbox]) -> Vec<(usize, usize)> {
        self.events.clear();
        self.active.clear();

        if bboxes.len() < 2 {
            return Vec::new();
        }

        self.events.reserve(bboxes.len() * 2);
        for bbox in bboxes {
            self.events.push(SweepEvent::Start {
                segment_id: bbox.segment_id,
                y: bbox.min_y,
            });
            self.events.push(SweepEvent::End {
                segment_id: bbox.segment_id,
                y: bbox.max_y,
            });
        }

        self.events.sort_by_key(|e| match e {
            SweepEvent::Start { y, .. } => (*y, 0u8),
            SweepEvent::End { y, .. } => (*y, 1u8),
        });

        let mut overlaps = Vec::new();

        for event in &self.events {
            match event {
                SweepEvent::Start { segment_id, .. } => {
                    let sid = *segment_id;
                    let new_bbox = match bboxes.iter().find(|b| b.segment_id == sid) {
                        Some(b) => b,
                        None => continue,
                    };

                    for &active_id in &self.active {
                        let active_bbox = match bboxes.iter().find(|b| b.segment_id == active_id) {
                            Some(b) => b,
                            None => continue,
                        };

                        if aabb_overlap_2d(new_bbox, active_bbox) {
                            let pair = if sid < active_id {
                                (sid, active_id)
                            } else {
                                (active_id, sid)
                            };
                            overlaps.push(pair);
                        }
                    }

                    self.active.push(sid);
                }
                SweepEvent::End { segment_id, .. } => {
                    self.active.retain(|&i| i != *segment_id);
                }
            }
        }

        overlaps
    }
}

impl Default for FlatIntervalSweep {
    fn default() -> Self {
        Self::new()
    }
}

/// Check 2D AABB overlap (branchless i64 comparisons).
#[inline]
fn aabb_overlap_2d(a: &SegmentBbox, b: &SegmentBbox) -> bool {
    a.min_x < b.max_x && a.max_x > b.min_x && a.min_y < b.max_y && a.max_y > b.min_y
}

/// Find all overlapping segment pairs in a set of segments.
///
/// Sorts by Morton code, builds width-inflated bboxes, runs the flat
/// interval sweep, and returns the overlap pairs.
pub fn find_overlaps(segments: &[IndexedSegment]) -> Vec<(usize, usize)> {
    if segments.len() < 2 {
        return Vec::new();
    }

    let bboxes: Vec<SegmentBbox> = segments.iter().map(segment_bbox).collect();
    let mut sweep = FlatIntervalSweep::new();
    sweep.sweep(&bboxes)
}
