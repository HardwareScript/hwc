//! G-Cell-Local Unified Sweep Verification
//!
//! Complete G-cell-local DRC sweep engine with:
//! - Boundary-halo expansion for ghost segment detection
//! - Morton-ordered segment sorting for cache-friendly access
//! - Flat active-interval sweep (no BST, no pointer chasing)
//! - Unified overlap dispatch (same-net, different-net, no-overlap)
//! - SIMD-style 4-wide batched AABB overlap
//! - Rayon parallelism across G-cells

use crate::geometry::{BoundingBox, Point3D};
use crate::geometry::transform::BoundingBox2D;
use crate::geometry_router::partition::PartitionGrid;
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::geometry_router::route_decomposition::VirtualJunction;

use rayon::prelude::*;

/// A lightweight DRC violation for the sweep engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SweepViolation {
    pub net_a: u32,
    pub net_b: u32,
    pub location: (i64, i64),
    pub violation_type: ViolationType,
}

/// Types of DRC violations detected by the sweep engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViolationType {
    /// Clearance between two different nets is insufficient.
    ClearanceViolation { required: i64, actual: i64 },
    /// Two different nets are shorted (zero clearance).
    ShortCircuit,
    /// Same-net overlap not at a valid VirtualJunction or component port.
    SameNetOverlap,
}

// ============================================================================
// Ghost Registry
// ============================================================================

/// Tracks ghost (duplicate) segments across adjacent G-cells.
///
/// When a segment is within `max_clearance_nm` of a G-cell boundary,
/// it must be registered in both adjacent cells as a ghost duplicate.
/// The registry identifies which segments in the local list are ghosts
/// (their center lies outside the unexpanded cell bounds).
#[derive(Clone, Debug)]
pub struct GhostRegistry {
    ghost_indices: Vec<usize>,
}

impl GhostRegistry {
    pub fn new() -> Self {
        Self {
            ghost_indices: Vec::new(),
        }
    }

    #[inline]
    pub fn register_ghost(&mut self, local_index: usize) {
        self.ghost_indices.push(local_index);
    }

    #[inline]
    pub fn is_ghost(&self, local_index: usize) -> bool {
        self.ghost_indices.contains(&local_index)
    }

    /// Build a ghost registry from segments and cell bounds.
    ///
    /// A segment is a ghost if its center is outside the unexpanded cell
    /// but was included because it falls within the halo-expanded query region.
    pub fn from_segments(
        segments: &[IndexedSegment],
        cell_bounds: &BoundingBox,
    ) -> Self {
        let mut registry = Self::new();
        for (i, seg) in segments.iter().enumerate() {
            let center = seg.center();
            if !cell_bounds.contains(center) {
                registry.register_ghost(i);
            }
        }
        registry
    }
}

impl Default for GhostRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Morton Ordering (Z-order curve)
// ============================================================================

/// Compute a 2D Morton code (Z-order curve) for cache-friendly spatial sorting.
///
/// Interleaves the bits of x and y coordinates to produce a single u64 value.
/// Voxel positions close in 2D space produce similar Morton codes, yielding
/// excellent L1/L2 cache hit rates during the sweep.
#[inline]
pub fn compute_morton_code(x: i64, y: i64) -> u64 {
    let xu = (x as u64) & 0xFFFFFFFF;
    let yu = (y as u64) & 0xFFFFFFFF;
    spread_bits_2d(xu) | (spread_bits_2d(yu) << 1)
}

/// Spread bits of a 32-bit value so each bit is separated by one zero.
/// Core primitive for 2D Morton encoding.
#[inline(always)]
fn spread_bits_2d(mut v: u64) -> u64 {
    v &= 0xFFFFFFFF;
    v = (v | (v << 16)) & 0x0000FFFF0000FFFF;
    v = (v | (v << 8)) & 0x00FF00FF00FF00FF;
    v = (v | (v << 4)) & 0x0F0F0F0F0F0F0F0F;
    v = (v | (v << 2)) & 0x3333333333333333;
    v = (v | (v << 1)) & 0x5555555555555555;
    v
}

/// Sort segments by Morton code for cache-friendly access patterns.
///
/// Uses each segment's center point to compute the Morton code, ensuring
/// spatially proximate segments are adjacent in the sorted array.
#[inline]
pub fn sort_segments_by_morton(segments: &mut [IndexedSegment]) {
    segments.sort_by_key(|s| {
        let center = s.center();
        compute_morton_code(center.x, center.y)
    });
}

// ============================================================================
// Flat Active Interval Sweep
// ============================================================================

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

// ============================================================================
// SIMD-Style 4-Wide Batched AABB Overlap
// ============================================================================

/// SIMD-style 4-wide batched AABB overlap check.
///
/// Processes 4 bounding box pairs simultaneously using branchless i64
/// comparisons. Since nightly SIMD intrinsics are unavailable on stable,
/// this uses bitwise `&` on boolean results for branchless evaluation.
/// Falls back to scalar for the remainder (handled by the loop itself).
#[inline]
pub fn batch_aabb_overlap(
    boxes_a: &[BoundingBox2D; 4],
    boxes_b: &[BoundingBox2D; 4],
) -> [bool; 4] {
    let mut results = [false; 4];

    for i in 0..4 {
        let a = &boxes_a[i];
        let b = &boxes_b[i];
        let x_overlap = (a.min_x < b.max_x) & (a.max_x > b.min_x);
        let y_overlap = (a.min_y < b.max_y) & (a.max_y > b.min_y);
        results[i] = x_overlap & y_overlap;
    }

    results
}

// ============================================================================
// Unified Overlap Dispatch
// ============================================================================

/// Result of classifying the overlap between two segments.
#[derive(Clone, Debug)]
pub enum OverlapResult {
    /// Different nets overlap with insufficient clearance.
    DifferentNet {
        net_a: u32,
        net_b: u32,
        overlap_area: i64,
        required_clearance: i64,
    },
    /// Same-net overlap — valid only at a VirtualJunction or component port.
    SameNet {
        net_id: u32,
        is_valid_junction: bool,
    },
    /// No meaningful overlap.
    NoOverlap,
}

/// Classify the overlap between two segments.
///
/// Different-net overlaps are checked against clearance rules.
/// Same-net overlaps must land on a `VirtualJunctionNode` or component port bbox.
pub fn classify_overlap(
    seg_a: &IndexedSegment,
    seg_b: &IndexedSegment,
    junctions: &[VirtualJunction],
    default_clearance_nm: i64,
) -> OverlapResult {
    if seg_a.net_id == seg_b.net_id {
        let is_valid_junction = junctions.iter().any(|j| {
            j.net_id.0 == seg_a.net_id as u32
                && is_point_in_overlap_envelope(j.position, seg_a, seg_b)
        });

        OverlapResult::SameNet {
            net_id: seg_a.net_id as u32,
            is_valid_junction,
        }
    } else {
        let actual_clearance = compute_actual_clearance(seg_a, seg_b);

        if actual_clearance < default_clearance_nm {
            OverlapResult::DifferentNet {
                net_a: seg_a.net_id as u32,
                net_b: seg_b.net_id as u32,
                overlap_area: compute_overlap_area(seg_a, seg_b),
                required_clearance: default_clearance_nm,
            }
        } else {
            OverlapResult::NoOverlap
        }
    }
}

/// Check if a junction position lies within the combined envelope of two segments.
#[inline]
fn is_point_in_overlap_envelope(point: Point3D, seg_a: &IndexedSegment, seg_b: &IndexedSegment) -> bool {
    let a = segment_bbox(seg_a);
    let b = segment_bbox(seg_b);

    let min_x = a.min_x.min(b.min_x);
    let max_x = a.max_x.max(b.max_x);
    let min_y = a.min_y.min(b.min_y);
    let max_y = a.max_y.max(b.max_y);

    point.x >= min_x && point.x <= max_x && point.y >= min_y && point.y <= max_y
}

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
fn compute_overlap_area(seg_a: &IndexedSegment, seg_b: &IndexedSegment) -> i64 {
    let a = segment_bbox(seg_a);
    let b = segment_bbox(seg_b);

    let overlap_w = (a.max_x.min(b.max_x) - a.min_x.max(b.min_x)).max(0);
    let overlap_h = (a.max_y.min(b.max_y) - a.min_y.max(b.min_y)).max(0);
    overlap_w * overlap_h
}

// ============================================================================
// Per-G-Cell Sweep Context
// ============================================================================

struct GCellSweepContext {
    #[allow(dead_code)]
    cell_id: u32,
    segments: Vec<IndexedSegment>,
    ghost_registry: GhostRegistry,
}

// ============================================================================
// Main Entry Point
// ============================================================================

/// Verify all G-cells using Rayon parallelism.
///
/// Each G-cell is processed on a separate thread via `par_iter()`.
/// No global memory locks — each thread collects violations locally.
/// Returns a merged `Vec<SweepViolation>` of all DRC violations found.
pub fn verify_gcell_sweep(
    grid: &PartitionGrid,
    spatial_index: &DynamicSpatialIndex,
    junctions: &[VirtualJunction],
    default_clearance_nm: i64,
) -> Vec<SweepViolation> {
    let contexts: Vec<GCellSweepContext> = grid
        .cells
        .iter()
        .map(|cell| {
            let expanded_bounds = cell.bounds.expand(grid.max_clearance_nm);
            let segments: Vec<IndexedSegment> = spatial_index
                .query_bbox(&expanded_bounds)
                .into_iter()
                .cloned()
                .collect();

            let ghost_registry = GhostRegistry::from_segments(&segments, &cell.bounds);

            GCellSweepContext {
                cell_id: cell.id.0,
                segments,
                ghost_registry,
            }
        })
        .collect();

    let violation_results: Vec<Vec<SweepViolation>> = contexts
        .par_iter()
        .map(|ctx| verify_single_gcell(ctx, junctions, default_clearance_nm))
        .collect();

    violation_results.into_iter().flatten().collect()
}

/// Verify a single G-cell using the flat interval sweep.
fn verify_single_gcell(
    ctx: &GCellSweepContext,
    junctions: &[VirtualJunction],
    default_clearance_nm: i64,
) -> Vec<SweepViolation> {
    if ctx.segments.len() < 2 {
        return Vec::new();
    }

    let mut sorted_segments = ctx.segments.clone();
    sort_segments_by_morton(&mut sorted_segments);

    let bboxes: Vec<SegmentBbox> = sorted_segments.iter().map(segment_bbox).collect();
    let mut sweep = FlatIntervalSweep::new();
    let overlaps = sweep.sweep(&bboxes);

    let mut violations = Vec::new();

    for (sid_a, sid_b) in overlaps {
        let seg_a = match sorted_segments.iter().find(|s| s.segment_id == sid_a) {
            Some(s) => s,
            None => continue,
        };
        let seg_b = match sorted_segments.iter().find(|s| s.segment_id == sid_b) {
            Some(s) => s,
            None => continue,
        };

        let idx_a = match sorted_segments.iter().position(|s| s.segment_id == sid_a) {
            Some(i) => i,
            None => continue,
        };
        let idx_b = match sorted_segments.iter().position(|s| s.segment_id == sid_b) {
            Some(i) => i,
            None => continue,
        };

        let a_is_ghost = ctx.ghost_registry.is_ghost(idx_a);
        let b_is_ghost = ctx.ghost_registry.is_ghost(idx_b);
        if a_is_ghost && b_is_ghost {
            continue;
        }

        let result = classify_overlap(seg_a, seg_b, junctions, default_clearance_nm);

        match result {
            OverlapResult::DifferentNet {
                net_a,
                net_b,
                required_clearance,
                ..
            } => {
                let center_a = seg_a.center();
                let center_b = seg_b.center();
                let location = (
                    (center_a.x + center_b.x) / 2,
                    (center_a.y + center_b.y) / 2,
                );
                let actual = compute_actual_clearance(seg_a, seg_b);
                violations.push(SweepViolation {
                    net_a,
                    net_b,
                    location,
                    violation_type: ViolationType::ClearanceViolation {
                        required: required_clearance,
                        actual,
                    },
                });
            }
            OverlapResult::SameNet {
                net_id,
                is_valid_junction,
            } => {
                if !is_valid_junction {
                    let center_a = seg_a.center();
                    let center_b = seg_b.center();
                    let location = (
                        (center_a.x + center_b.x) / 2,
                        (center_a.y + center_b.y) / 2,
                    );
                    violations.push(SweepViolation {
                        net_a: net_id,
                        net_b: net_id,
                        location,
                        violation_type: ViolationType::SameNetOverlap,
                    });
                }
            }
            OverlapResult::NoOverlap => {}
        }
    }

    violations
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Morton code tests
    // ------------------------------------------------------------------

    #[test]
    fn morton_code_origin() {
        assert_eq!(compute_morton_code(0, 0), 0);
    }

    #[test]
    fn morton_code_unit_axes() {
        assert_eq!(compute_morton_code(1, 0), 1);
        assert_eq!(compute_morton_code(0, 1), 2);
        assert_eq!(compute_morton_code(1, 1), 3);
    }

    #[test]
    fn morton_code_monotonic_along_x() {
        let codes: Vec<u64> = (0..16).map(|x| compute_morton_code(x, 0)).collect();
        for i in 1..codes.len() {
            assert!(codes[i] > codes[i - 1], "Morton code must increase along X");
        }
    }

    #[test]
    fn morton_code_z_pattern() {
        // Z-pattern: (0,0)->0, (1,0)->1, (0,1)->2, (1,1)->3
        assert_eq!(compute_morton_code(0, 0), 0);
        assert_eq!(compute_morton_code(1, 0), 1);
        assert_eq!(compute_morton_code(0, 1), 2);
        assert_eq!(compute_morton_code(1, 1), 3);
    }

    #[test]
    fn morton_code_large_coordinates() {
        let c1 = compute_morton_code(100_000_000, 200_000_000);
        let c2 = compute_morton_code(100_000_001, 200_000_000);
        assert!(c2 > c1);
    }

    // ------------------------------------------------------------------
    // Ghost registry tests
    // ------------------------------------------------------------------

    #[test]
    fn ghost_registry_identifies_out_of_cell_segments() {
        let cell_bounds = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(10_000_000, 10_000_000, 0),
        );

        let segments = vec![
            IndexedSegment {
                segment_id: 0,
                net_id: 1,
                width_nm: 500_000,
                start: Point3D::new(5_000_000, 5_000_000, 0),
                end: Point3D::new(5_500_000, 5_500_000, 0),
                layer: 1,
            },
            IndexedSegment {
                segment_id: 1,
                net_id: 1,
                width_nm: 500_000,
                start: Point3D::new(11_000_000, 5_000_000, 0),
                end: Point3D::new(11_500_000, 5_500_000, 0),
                layer: 1,
            },
        ];

        let registry = GhostRegistry::from_segments(&segments, &cell_bounds);
        assert!(!registry.is_ghost(0), "Segment inside cell should not be ghost");
        assert!(registry.is_ghost(1), "Segment outside cell should be ghost");
    }

    #[test]
    fn ghost_registry_empty() {
        let cell_bounds = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(10_000_000, 10_000_000, 0),
        );
        let registry = GhostRegistry::from_segments(&[], &cell_bounds);
        assert!(!registry.is_ghost(0));
    }

    // ------------------------------------------------------------------
    // AABB batch overlap tests
    // ------------------------------------------------------------------

    #[test]
    fn batch_aabb_overlap_known_overlaps() {
        let a = [
            BoundingBox2D::new(0, 0, 10, 10),
            BoundingBox2D::new(0, 0, 10, 10),
            BoundingBox2D::new(0, 0, 5, 5),
            BoundingBox2D::new(0, 0, 20, 20),
        ];
        let b = [
            BoundingBox2D::new(5, 5, 15, 15),
            BoundingBox2D::new(20, 20, 30, 30),
            BoundingBox2D::new(3, 3, 8, 8),
            BoundingBox2D::new(10, 10, 15, 15),
        ];

        let results = batch_aabb_overlap(&a, &b);
        assert!(results[0], "Overlapping boxes");
        assert!(!results[1], "Non-overlapping boxes");
        assert!(results[2], "Nested boxes");
        assert!(results[3], "Overlapping boxes");
    }

    #[test]
    fn batch_aabb_overlap_edge_cases() {
        let a = [
            BoundingBox2D::new(0, 0, 10, 10),
            BoundingBox2D::new(0, 0, 10, 10),
            BoundingBox2D::new(0, 0, 10, 10),
            BoundingBox2D::new(0, 0, 10, 10),
        ];
        let b = [
            BoundingBox2D::new(10, 0, 20, 10),
            BoundingBox2D::new(11, 0, 20, 10),
            BoundingBox2D::new(5, 5, 15, 15),
            BoundingBox2D::new(-5, -5, 0, 0),
        ];

        let results = batch_aabb_overlap(&a, &b);
        assert!(!results[0], "Edge-touching (strict <) = no overlap");
        assert!(!results[1], "Gap = no overlap");
        assert!(results[2], "Overlapping");
        assert!(!results[3], "Corner-touching = no overlap");
    }

    // ------------------------------------------------------------------
    // Sweep line tests
    // ------------------------------------------------------------------

    #[test]
    fn sweep_finds_crossing_segments() {
        let segs = vec![
            IndexedSegment {
                segment_id: 0,
                net_id: 1,
                width_nm: 1_000_000,
                start: Point3D::new(0, 5_000_000, 0),
                end: Point3D::new(10_000_000, 5_000_000, 0),
                layer: 1,
            },
            IndexedSegment {
                segment_id: 1,
                net_id: 2,
                width_nm: 1_000_000,
                start: Point3D::new(5_000_000, 0, 0),
                end: Point3D::new(5_000_000, 10_000_000, 0),
                layer: 1,
            },
        ];

        let overlaps = find_overlaps(&segs);
        assert!(!overlaps.is_empty(), "Crossing traces must produce overlap");
    }

    #[test]
    fn sweep_no_false_positives_for_separated_segments() {
        let segs = vec![
            IndexedSegment {
                segment_id: 0,
                net_id: 1,
                width_nm: 1_000_000,
                start: Point3D::new(0, 5_000_000, 0),
                end: Point3D::new(4_000_000, 5_000_000, 0),
                layer: 1,
            },
            IndexedSegment {
                segment_id: 1,
                net_id: 2,
                width_nm: 1_000_000,
                start: Point3D::new(6_000_000, 5_000_000, 0),
                end: Point3D::new(10_000_000, 5_000_000, 0),
                layer: 1,
            },
        ];

        let overlaps = find_overlaps(&segs);
        assert!(overlaps.is_empty(), "Separated segments must not overlap");
    }

    #[test]
    fn sweep_handles_single_segment() {
        let segs = vec![IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 1_000_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(10_000_000, 0, 0),
            layer: 1,
        }];

        let overlaps = find_overlaps(&segs);
        assert!(overlaps.is_empty());
    }

    #[test]
    fn sweep_handles_empty_input() {
        let overlaps = find_overlaps(&[]);
        assert!(overlaps.is_empty());
    }

    #[test]
    fn sweep_finds_parallel_close_segments() {
        let segs = vec![
            IndexedSegment {
                segment_id: 0,
                net_id: 1,
                width_nm: 1_000_000,
                start: Point3D::new(0, 5_000_000, 0),
                end: Point3D::new(10_000_000, 5_000_000, 0),
                layer: 1,
            },
            IndexedSegment {
                segment_id: 1,
                net_id: 2,
                width_nm: 1_000_000,
                start: Point3D::new(0, 5_500_000, 0),
                end: Point3D::new(10_000_000, 5_500_000, 0),
                layer: 1,
            },
        ];

        let overlaps = find_overlaps(&segs);
        assert!(!overlaps.is_empty(), "Close parallel traces must overlap");
    }

    // ------------------------------------------------------------------
    // Classification tests
    // ------------------------------------------------------------------

    #[test]
    fn classify_different_net_crossing() {
        let seg_a = IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 1_000_000,
            start: Point3D::new(0, 5_000_000, 0),
            end: Point3D::new(10_000_000, 5_000_000, 0),
            layer: 1,
        };
        let seg_b = IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 1_000_000,
            start: Point3D::new(5_000_000, 0, 0),
            end: Point3D::new(5_000_000, 10_000_000, 0),
            layer: 1,
        };

        match classify_overlap(&seg_a, &seg_b, &[], 500_000) {
            OverlapResult::DifferentNet { net_a, net_b, .. } => {
                assert_eq!(net_a, 1);
                assert_eq!(net_b, 2);
            }
            other => panic!("Expected DifferentNet, got {:?}", other),
        }
    }

    #[test]
    fn classify_same_net_without_junction() {
        let seg_a = IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 1_000_000,
            start: Point3D::new(0, 5_000_000, 0),
            end: Point3D::new(10_000_000, 5_000_000, 0),
            layer: 1,
        };
        let seg_b = IndexedSegment {
            segment_id: 1,
            net_id: 1,
            width_nm: 1_000_000,
            start: Point3D::new(5_000_000, 0, 0),
            end: Point3D::new(5_000_000, 10_000_000, 0),
            layer: 1,
        };

        match classify_overlap(&seg_a, &seg_b, &[], 500_000) {
            OverlapResult::SameNet {
                net_id,
                is_valid_junction,
            } => {
                assert_eq!(net_id, 1);
                assert!(!is_valid_junction);
            }
            other => panic!("Expected SameNet, got {:?}", other),
        }
    }

    #[test]
    fn classify_same_net_with_valid_junction() {
        let seg_a = IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 1_000_000,
            start: Point3D::new(0, 5_000_000, 0),
            end: Point3D::new(10_000_000, 5_000_000, 0),
            layer: 1,
        };
        let seg_b = IndexedSegment {
            segment_id: 1,
            net_id: 1,
            width_nm: 1_000_000,
            start: Point3D::new(5_000_000, 0, 0),
            end: Point3D::new(5_000_000, 10_000_000, 0),
            layer: 1,
        };

        let junctions = vec![VirtualJunction {
            junction_id: 0,
            position: Point3D::new(5_000_000, 5_000_000, 0),
            connected_segments: vec![0, 1],
            net_id: crate::netlist::NetId(1),
            capacitance_pf: 0.0,
            inductance_nh: 0.0,
        }];

        match classify_overlap(&seg_a, &seg_b, &junctions, 500_000) {
            OverlapResult::SameNet {
                is_valid_junction, ..
            } => {
                assert!(is_valid_junction);
            }
            other => panic!("Expected SameNet with junction, got {:?}", other),
        }
    }

    #[test]
    fn classify_far_apart_segments_no_overlap() {
        let seg_a = IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 1_000_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(1_000_000, 0, 0),
            layer: 1,
        };
        let seg_b = IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 1_000_000,
            start: Point3D::new(100_000_000, 100_000_000, 0),
            end: Point3D::new(101_000_000, 100_000_000, 0),
            layer: 1,
        };

        match classify_overlap(&seg_a, &seg_b, &[], 500_000) {
            OverlapResult::NoOverlap => {}
            other => panic!("Expected NoOverlap, got {:?}", other),
        }
    }

    // ------------------------------------------------------------------
    // Actual clearance computation tests
    // ------------------------------------------------------------------

    #[test]
    fn clearance_crossing_at_origin() {
        let seg_a = IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 1_000_000,
            start: Point3D::new(-5_000_000, 0, 0),
            end: Point3D::new(5_000_000, 0, 0),
            layer: 1,
        };
        let seg_b = IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 1_000_000,
            start: Point3D::new(0, -5_000_000, 0),
            end: Point3D::new(0, 5_000_000, 0),
            layer: 1,
        };

        let clearance = compute_actual_clearance(&seg_a, &seg_b);
        assert_eq!(clearance, -1_000_000, "Crossing at centers: -width");
    }

    #[test]
    fn clearance_parallel_horizontal() {
        let seg_a = IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 1_000_000,
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(10_000_000, 0, 0),
            layer: 1,
        };
        let seg_b = IndexedSegment {
            segment_id: 1,
            net_id: 2,
            width_nm: 1_000_000,
            start: Point3D::new(0, 2_000_000, 0),
            end: Point3D::new(10_000_000, 2_000_000, 0),
            layer: 1,
        };

        let clearance = compute_actual_clearance(&seg_a, &seg_b);
        assert_eq!(clearance, 1_000_000, "2mm center spacing - 1mm width = 1mm clearance");
    }

    // ------------------------------------------------------------------
    // Segment bbox tests
    // ------------------------------------------------------------------

    #[test]
    fn segment_bbox_horizontal() {
        let seg = IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 2_000_000,
            start: Point3D::new(0, 5_000_000, 0),
            end: Point3D::new(10_000_000, 5_000_000, 0),
            layer: 1,
        };

        let bbox = segment_bbox(&seg);
        assert_eq!(bbox.min_x, -1_000_000);
        assert_eq!(bbox.max_x, 11_000_000);
        assert_eq!(bbox.min_y, 4_000_000);
        assert_eq!(bbox.max_y, 6_000_000);
    }

    #[test]
    fn segment_bbox_vertical() {
        let seg = IndexedSegment {
            segment_id: 0,
            net_id: 1,
            width_nm: 2_000_000,
            start: Point3D::new(5_000_000, 0, 0),
            end: Point3D::new(5_000_000, 10_000_000, 0),
            layer: 1,
        };

        let bbox = segment_bbox(&seg);
        assert_eq!(bbox.min_x, 4_000_000);
        assert_eq!(bbox.max_x, 6_000_000);
        assert_eq!(bbox.min_y, -1_000_000);
        assert_eq!(bbox.max_y, 11_000_000);
    }
}
