//! G-Cell-Local Unified Sweep Verification
//!
//! Complete G-cell-local DRC sweep engine with:
//! - Boundary-halo expansion for ghost segment detection
//! - Morton-ordered segment sorting for cache-friendly access
//! - Flat active-interval sweep (no BST, no pointer chasing)
//! - Unified overlap dispatch (same-net, different-net, no-overlap)
//! - SIMD-style 4-wide batched AABB overlap
//! - std::thread::scope parallelism across G-cells

use crate::geometry::transform::BoundingBox2D;
use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::partition::PartitionGrid;
use crate::geometry_router::route_decomposition::VirtualJunction;
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::material::{MaterialConductivity, MaterialId, MaterialRegistry};

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
    /// v0.1.8: Coplanar forbidden junction — conductor touching semiconductor
    /// without an intermediate ohmic contact bridge.
    ForbiddenJunction {
        mat_a: CompactString,
        mat_b: CompactString,
    },
}

/// v0.1.8: Classification of a material junction between two touching geometries.
///
/// This is a table-driven classification: the DRC engine queries the material
/// registry for conductivity categories and the bridge table for registered
/// transitions. No hard-coding — all rules come from the profile + material DB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JunctionClassification {
    /// The junction is allowed (same category, or insulator involved).
    Allowed,
    /// A bridge is required and has been declared in the profile.
    /// Contains the bridge material name for diagnostic suggestions.
    BridgeRequired { bridge: CompactString },
    /// The junction is forbidden — conductor touching semiconductor with
    /// no declared bridge. This is a hard error.
    Forbidden,
}

/// Bridge table lookup key: "FromMaterial:ToMaterial" → bridge material name.
pub type BridgeTable = rustc_hash::FxHashMap<CompactString, CompactString>;

use compact_str::CompactString;

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
    pub fn from_segments(segments: &[IndexedSegment], cell_bounds: &BoundingBox) -> Self {
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
/// Positions close in 2D space produce similar Morton codes, yielding
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
pub fn batch_aabb_overlap(boxes_a: &[BoundingBox2D; 4], boxes_b: &[BoundingBox2D; 4]) -> [bool; 4] {
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
    /// v0.1.8: Same-net intersection with different materials (volumetric overlap).
    /// Clipper2 cannot weld different materials, so overlapping conductor+semiconductor
    /// on the same net produces invalid mesh data. Must trigger P45.
    SameNetIntersection {
        net_id: u32,
        mat_a: MaterialId,
        mat_b: MaterialId,
        intersection_area: i64,
    },
    /// v0.1.8: Material junction classification (coplanar face-touching).
    /// Two different materials touch on the same Z-layer. The classification
    /// determines whether this is Allowed, requires a Bridge, or is Forbidden.
    MaterialJunction {
        classification: JunctionClassification,
        mat_a_name: CompactString,
        mat_b_name: CompactString,
    },
    /// No meaningful overlap.
    NoOverlap,
}

/// Query parameters for [`classify_overlap`].
pub struct OverlapQuery<'a> {
    pub seg_a: &'a IndexedSegment,
    pub seg_b: &'a IndexedSegment,
    pub junctions: &'a [VirtualJunction],
    pub default_clearance_nm: i64,
    pub mat_a_id: Option<MaterialId>,
    pub mat_b_id: Option<MaterialId>,
    pub material_registry: &'a MaterialRegistry,
    pub bridge_table: &'a BridgeTable,
}

/// Classify the overlap between two segments.
///
/// Different-net overlaps are checked against clearance rules.
/// Same-net overlaps must land on a `VirtualJunctionNode` or component port bbox.
///
/// v0.1.8: Also performs material junction classification for same-net
/// different-material intersections and coplanar face-touching.
pub fn classify_overlap(q: OverlapQuery) -> OverlapResult {
    let OverlapQuery {
        seg_a,
        seg_b,
        junctions,
        default_clearance_nm,
        mat_a_id,
        mat_b_id,
        material_registry,
        bridge_table,
    } = q;
    if seg_a.net_id == seg_b.net_id {
        let is_valid_junction = junctions.iter().any(|j| {
            j.net_id.0 == seg_a.net_id as u32
                && is_point_in_overlap_envelope(j.position, seg_a, seg_b)
        });

        // v0.1.8: Check for same-net different-material intersection.
        // If two segments on the same net have different materials and their
        // AABBs intersect (not just face-touch), this is a volumetric overlap
        // that Clipper2 cannot weld. Must trigger P45.
        if let (Some(ma), Some(mb)) = (mat_a_id, mat_b_id) {
            if ma != mb {
                let a = segment_bbox(seg_a);
                let b = segment_bbox(seg_b);
                let intersection_area = compute_bbox_intersection_area(&a, &b);

                if intersection_area > 0 {
                    // Volumetric intersection detected — classify the junction
                    let classification = classify_junction(ma, mb, material_registry, bridge_table);
                    let name_a = material_registry.get_name(ma).unwrap_or("Unknown");
                    let name_b = material_registry.get_name(mb).unwrap_or("Unknown");

                    return match classification {
                        JunctionClassification::Forbidden => OverlapResult::MaterialJunction {
                            classification,
                            mat_a_name: name_a.into(),
                            mat_b_name: name_b.into(),
                        },
                        JunctionClassification::BridgeRequired { .. } => {
                            OverlapResult::MaterialJunction {
                                classification,
                                mat_a_name: name_a.into(),
                                mat_b_name: name_b.into(),
                            }
                        }
                        JunctionClassification::Allowed => {
                            // Same net, different material, but allowed — still flag
                            // as SameNetIntersection for diagnostic purposes
                            OverlapResult::SameNetIntersection {
                                net_id: seg_a.net_id as u32,
                                mat_a: ma,
                                mat_b: mb,
                                intersection_area,
                            }
                        }
                    };
                }
            }
        }

        // v0.1.8: Check for coplanar face-touching with different materials.
        // This catches cases where two segments touch at a boundary (not volumetric
        // intersection) but have different materials on the same net.
        if let (Some(ma), Some(mb)) = (mat_a_id, mat_b_id) {
            if ma != mb {
                let a = segment_bbox(seg_a);
                let b = segment_bbox(seg_b);
                let intersection_area = compute_bbox_intersection_area(&a, &b);

                // Face-touching: bounding boxes touch but don't volumetrically overlap
                if intersection_area == 0 && aabb_faces_touch(&a, &b) {
                    let classification = classify_junction(ma, mb, material_registry, bridge_table);
                    let name_a = material_registry.get_name(ma).unwrap_or("Unknown");
                    let name_b = material_registry.get_name(mb).unwrap_or("Unknown");

                    return OverlapResult::MaterialJunction {
                        classification,
                        mat_a_name: name_a.into(),
                        mat_b_name: name_b.into(),
                    };
                }
            }
        }

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
fn is_point_in_overlap_envelope(
    point: Point3D,
    seg_a: &IndexedSegment,
    seg_b: &IndexedSegment,
) -> bool {
    let a = segment_bbox(seg_a);
    let b = segment_bbox(seg_b);

    let min_x = a.min_x.min(b.min_x);
    let max_x = a.max_x.max(b.max_x);
    let min_y = a.min_y.min(b.min_y);
    let max_y = a.max_y.max(b.max_y);

    point.x >= min_x && point.x <= max_x && point.y >= min_y && point.y <= max_y
}

/// v0.1.8: Classify a material junction between two touching geometries.
///
/// This is the core table-driven junction classifier for Physical Synthesis
/// Guardrails. It uses the `MaterialRegistry` (symbol table) for conductivity
/// lookups and the `BridgeTable` (profile bridge rules) for junction rules.
///
/// # Classification Rules
/// - Conductor touching Semiconductor without a declared bridge → `Forbidden`
/// - Conductor touching Semiconductor with a declared bridge → `BridgeRequired`
/// - Same category or insulator involved → `Allowed`
///
/// # Arguments
/// * `mat_a_id` - Material ID of the first geometry (from `MaterialRegistry`)
/// * `mat_b_id` - Material ID of the second geometry
/// * `registry` - The engine's material registry (lookup table for conductivity)
/// * `bridge_table` - Profile bridge rules (lookup table for junctions)
///
/// # Returns
/// `JunctionClassification` indicating whether the junction is allowed,
/// requires a bridge, or is forbidden.
pub fn classify_junction(
    mat_a_id: MaterialId,
    mat_b_id: MaterialId,
    registry: &MaterialRegistry,
    bridge_table: &BridgeTable,
) -> JunctionClassification {
    let cat_a = match registry.get_conductivity(mat_a_id) {
        Some(c) => c,
        None => return JunctionClassification::Allowed, // Unknown material → skip
    };
    let cat_b = match registry.get_conductivity(mat_b_id) {
        Some(c) => c,
        None => return JunctionClassification::Allowed,
    };

    let name_a = registry.get_name(mat_a_id).unwrap_or("Unknown");
    let name_b = registry.get_name(mat_b_id).unwrap_or("Unknown");

    match (cat_a, cat_b) {
        // Conductor touching Semiconductor → check for bridge
        (MaterialConductivity::Conductor, MaterialConductivity::Semiconductor)
        | (MaterialConductivity::Semiconductor, MaterialConductivity::Conductor) => {
            let key: CompactString = format!("{}:{}", name_a, name_b).into();
            if let Some(bridge_name) = bridge_table.get(key.as_str()) {
                JunctionClassification::BridgeRequired {
                    bridge: bridge_name.clone(),
                }
            } else {
                JunctionClassification::Forbidden
            }
        }
        // Same category or insulator involved → OK
        _ => JunctionClassification::Allowed,
    }
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

/// v0.1.8: Compute the intersection area of two AABBs (axis-aligned bounding boxes).
/// Returns 0 if the boxes don't overlap. Used to detect volumetric intersections
/// between different-material segments on the same net.
#[inline]
fn compute_bbox_intersection_area(a: &SegmentBbox, b: &SegmentBbox) -> i64 {
    let overlap_w = (a.max_x.min(b.max_x) - a.min_x.max(b.min_x)).max(0);
    let overlap_h = (a.max_y.min(b.max_y) - a.min_y.max(b.min_y)).max(0);
    overlap_w * overlap_h
}

/// v0.1.8: Check if two AABBs touch at a face (coplanar boundary contact).
/// Two boxes "face-touch" when they are adjacent along one axis with zero gap
/// but don't volumetrically overlap. This is distinct from corner/edge touching.
#[inline]
fn aabb_faces_touch(a: &SegmentBbox, b: &SegmentBbox) -> bool {
    // Boxes must be strictly adjacent (not overlapping) on one axis
    // and overlapping on the other axis.
    let x_adjacent = a.max_x == b.min_x || b.max_x == a.min_x;
    let y_adjacent = a.max_y == b.min_y || b.max_y == a.min_y;

    let x_overlap = a.min_x < b.max_x && a.max_x > b.min_x;
    let y_overlap = a.min_y < b.max_y && a.max_y > b.min_y;

    (x_adjacent && y_overlap) || (y_adjacent && x_overlap)
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

/// Verify all G-cells using std::thread::scope parallelism.
///
/// Each G-cell is processed on a separate thread via coarse-grained chunks
/// across CPU cores. No global memory locks — each thread collects violations
/// locally. Returns a merged `Vec<SweepViolation>` of all DRC violations found.
///
/// # Arguments
/// * `grid` - G-cell partition grid
/// * `spatial_index` - R*-tree of routed segments
/// * `junctions` - Virtual junctions from route decomposition
/// * `default_clearance_nm` - Minimum clearance between different nets
/// * `layer_to_material` - v0.1.8: Table mapping layer ID to material ID
/// * `material_registry` - v0.1.8: Material symbol table for conductivity lookups
/// * `bridge_table` - v0.1.8: Profile bridge rules for junction classification
pub fn verify_gcell_sweep(
    grid: &PartitionGrid,
    spatial_index: &DynamicSpatialIndex,
    junctions: &[VirtualJunction],
    default_clearance_nm: i64,
    layer_to_material: &rustc_hash::FxHashMap<i64, MaterialId>,
    material_registry: &MaterialRegistry,
    bridge_table: &BridgeTable,
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

    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let chunk_size = (contexts.len() + cpu_cores - 1).max(1);

    let violation_results: Vec<Vec<SweepViolation>> = std::thread::scope(|s| {
        let mut handles = Vec::new();

        for chunk in contexts.chunks(chunk_size) {
            let handle = s.spawn(move || {
                let mut local_violations: Vec<SweepViolation> = Vec::new();
                for ctx in chunk {
                    local_violations.extend(verify_single_gcell(
                        ctx,
                        junctions,
                        default_clearance_nm,
                        layer_to_material,
                        material_registry,
                        bridge_table,
                    ));
                }
                local_violations
            });
            handles.push(handle);
        }

        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    violation_results.into_iter().flatten().collect()
}

/// Verify a single G-cell using the flat interval sweep.
fn verify_single_gcell(
    ctx: &GCellSweepContext,
    junctions: &[VirtualJunction],
    default_clearance_nm: i64,
    layer_to_material: &rustc_hash::FxHashMap<i64, MaterialId>,
    material_registry: &MaterialRegistry,
    bridge_table: &BridgeTable,
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

        // v0.1.8: Look up material IDs for both segments via the layer-to-material table.
        let mat_a_id = layer_to_material.get(&seg_a.layer).copied();
        let mat_b_id = layer_to_material.get(&seg_b.layer).copied();

        let result = classify_overlap(OverlapQuery {
            seg_a,
            seg_b,
            junctions,
            default_clearance_nm,
            mat_a_id,
            mat_b_id,
            material_registry,
            bridge_table,
        });

        let center_a = seg_a.center();
        let center_b = seg_b.center();
        let midpoint = ((center_a.x + center_b.x) / 2, (center_a.y + center_b.y) / 2);

        match result {
            OverlapResult::DifferentNet {
                net_a,
                net_b,
                required_clearance,
                ..
            } => {
                let actual = compute_actual_clearance(seg_a, seg_b);
                violations.push(SweepViolation {
                    net_a,
                    net_b,
                    location: midpoint,
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
                    violations.push(SweepViolation {
                        net_a: net_id,
                        net_b: net_id,
                        location: midpoint,
                        violation_type: ViolationType::SameNetOverlap,
                    });
                }
            }
            OverlapResult::SameNetIntersection {
                net_id,
                mat_a,
                mat_b,
                ..
            } => {
                // v0.1.8: Same-net different-material intersection → P45
                let mat_a_name = material_registry
                    .get_name(mat_a)
                    .unwrap_or("Unknown")
                    .to_string();
                let mat_b_name = material_registry
                    .get_name(mat_b)
                    .unwrap_or("Unknown")
                    .to_string();
                violations.push(SweepViolation {
                    net_a: net_id,
                    net_b: net_id,
                    location: midpoint,
                    violation_type: ViolationType::ForbiddenJunction {
                        mat_a: mat_a_name.into(),
                        mat_b: mat_b_name.into(),
                    },
                });
            }
            OverlapResult::MaterialJunction {
                classification,
                mat_a_name,
                mat_b_name,
            } => {
                match classification {
                    JunctionClassification::Forbidden => {
                        // v0.1.8: Conductor-semiconductor without bridge → P45 error
                        violations.push(SweepViolation {
                            net_a: 0,
                            net_b: 0,
                            location: midpoint,
                            violation_type: ViolationType::ForbiddenJunction {
                                mat_a: mat_a_name,
                                mat_b: mat_b_name,
                            },
                        });
                    }
                    JunctionClassification::BridgeRequired { .. } => {
                        // Bridge declared → warning only, not a violation
                        // The bridge_validator.rs handles the actual bridge validation
                    }
                    JunctionClassification::Allowed => {}
                }
            }
            OverlapResult::NoOverlap => {}
        }
    }

    violations
}
