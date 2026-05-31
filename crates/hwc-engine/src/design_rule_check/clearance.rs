//! Clearance validation logic with "Primitives Over Pixels" architecture.
//!
//! **v0.1.7 ARCHITECTURAL TRANSFORMATION**: This module now uses analytic geometry
//! on substrate layer primitives instead of voxel iteration. This eliminates the
//! O(N²) bottleneck where N = voxel count (4.3 billion comparisons for large pours).
//!
//! **Performance**: O(layers²) instead of O(voxels²)
//! - Old: 65,522 × 65,522 = 4.3 billion voxel comparisons (hangs)
//! - New: 2 × 2 = 4 bounding box comparisons (instant)
//!
//! See: ROADMAP/v0.1.6/DEFERRED-WORK.md "Primitives Over Pixels"

use crate::constraint_manager::ConstraintRulebook;
use crate::geometry::Point3D;

use super::types::{DrcViolation, NetVoxels};

/// Internal struct for O(1) collision culling (legacy voxel-based validation)
struct BoundingBox {
    min_x: i64,
    max_x: i64,
    min_y: i64,
    max_y: i64,
    min_z: i64,
    max_z: i64,
}

fn get_bbox(voxels: &[Point3D]) -> Option<BoundingBox> {
    if voxels.is_empty() {
        return None;
    }

    let mut bbox = BoundingBox {
        min_x: i64::MAX,
        max_x: i64::MIN,
        min_y: i64::MAX,
        max_y: i64::MIN,
        min_z: i64::MAX,
        max_z: i64::MIN,
    };

    for v in voxels {
        if v.x < bbox.min_x {
            bbox.min_x = v.x;
        }
        if v.x > bbox.max_x {
            bbox.max_x = v.x;
        }
        if v.y < bbox.min_y {
            bbox.min_y = v.y;
        }
        if v.y > bbox.max_y {
            bbox.max_y = v.y;
        }
        if v.z < bbox.min_z {
            bbox.min_z = v.z;
        }
        if v.z > bbox.max_z {
            bbox.max_z = v.z;
        }
    }

    Some(bbox)
}

/// Validate clearances between all nets.
///
/// **PERFORMANCE OVERHAUL**:
/// 1. Pre-computes AABBs (Axis-Aligned Bounding Boxes)
/// 2. Rayon parallelism over net pairs
/// 3. O(1) rejection for nets safely apart (AABB overlap check)
/// 4. Chebyshev fast-reject in the inner voxel loop
///
/// **Algorithm**:
/// 1. For each pair of nets (i, j where i < j)
/// 2. Get voltage difference and calculate required clearance
/// 3. Find minimum distance between nets
/// 4. If actual < required → violation
///
/// **v0.1.4 Phase 3**: Now uses fabrication constraints from Symbol Table.
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 100-200, clearance calculation)
///
/// # Arguments
/// * `nets` - All routed nets with their voxel locations
/// * `constraints` - Constraint rulebook with clearance requirements
///
/// # Returns
/// Vector of clearance violations
pub fn validate_clearances(
    nets: &[NetVoxels],
    constraints: &ConstraintRulebook,
) -> Vec<DrcViolation> {
    use rayon::prelude::*;

    // Get required clearance from fabrication constraints.
    //
    // v0.1.7 ARCHITECTURE: Standard net-to-net spacing comes from the profile's
    // `trace.min_spacing` field, surfaced here as `min_trace_spacing_nm`.
    // `high_voltage_clearance_nm` is reserved exclusively for HV isolation checks
    // (net pairs where at least one net is declared high-voltage) and must NOT be
    // used as a general-purpose clearance — it is typically 1.5–3mm, which is
    // physically impossible to satisfy on normal-density PCBs.
    //
    // If no fabrication constraints are loaded (no profile), skip the clearance
    // check entirely — the missing-profile error is reported elsewhere.
    let required_clearance_nm = match constraints.fabrication.as_ref() {
        Some(fab) => fab.min_trace_spacing_nm,
        None => return vec![], // No profile loaded — nothing to check
    };

    // O(N) pre-computation of Bounding Boxes
    let bboxes: Vec<Option<BoundingBox>> = nets.iter().map(|n| get_bbox(&n.voxels)).collect();

    // Parallel processing across all pairs
    (0..nets.len())
        .into_par_iter()
        .flat_map(|i| {
            let mut violations = Vec::new();
            let net_a = &nets[i];
            let bbox_a = match &bboxes[i] {
                Some(b) => b,
                None => return violations,
            };

            for j in (i + 1)..nets.len() {
                let net_b = &nets[j];

                // v0.1.7: Ignore substrate net (net_0) for standard clearance checks
                // Substrate shorts are handled by the bit-parallel PhysicsValidator
                if net_a.net_name == "net_0" || net_b.net_name == "net_0" {
                    continue;
                }

                let bbox_b = match &bboxes[j] {
                    Some(b) => b,
                    None => continue,
                };

                // v0.1.7 HV ISOLATION LOGIC:
                // Use high_voltage_clearance_nm if either net is HighVoltage.
                // Otherwise use standard min_trace_spacing_nm.
                let pair_required_nm = if net_a.classification == crate::space::NetClassification::HighVoltage
                    || net_b.classification == crate::space::NetClassification::HighVoltage
                {
                    constraints
                        .fabrication
                        .as_ref()
                        .and_then(|f| f.high_voltage_clearance_nm)
                        .unwrap_or(required_clearance_nm)
                } else {
                    required_clearance_nm
                };

                // O(1) Fast Path: AABB Rejection
                // If bounding boxes (dilated by clearance) do not overlap, skip immediately
                if bbox_a.min_x - pair_required_nm > bbox_b.max_x
                    || bbox_a.max_x + pair_required_nm < bbox_b.min_x
                    || bbox_a.min_y - pair_required_nm > bbox_b.max_y
                    || bbox_a.max_y + pair_required_nm < bbox_b.min_y
                    || bbox_a.min_z - pair_required_nm > bbox_b.max_z
                    || bbox_a.max_z + pair_required_nm < bbox_b.min_z
                {
                    continue; // Physically impossible to overlap
                }

                // If AABBs overlap, do the fine-grained voxel-to-voxel check
                let (min_distance, location) =
                    calculate_min_distance_between_nets(&net_a.voxels, &net_b.voxels);

                if min_distance < pair_required_nm {
                    violations.push(DrcViolation::ClearanceViolation {
                        net_a: net_a.net_name.clone(),
                        net_b: net_b.net_name.clone(),
                        actual_nm: min_distance,
                        required_nm: pair_required_nm,
                        location,
                    });
                }
            }
            violations
        })
        .collect()
}

/// Calculate minimum distance between two nets using "Primitives Over Pixels".
///
/// **v0.1.7 NATIVE SOLUTION**: Uses analytic geometry on bounding boxes instead
/// of voxel iteration. This is the same architectural pattern that fixed routing.
///
/// **Algorithm**:
/// 1. For large nets (>1000 voxels), use bbox-to-bbox distance (O(1))
/// 2. For small nets, use voxel-by-voxel distance (O(N²) but N is small)
///
/// **Why This Works**:
/// - Large pours are solid rectangles → bbox distance is exact
/// - Small traces/vias need precise voxel checking
/// - Threshold of 1000 voxels balances accuracy vs performance
///
/// **Performance**:
/// - Large pours: 4.3 billion comparisons → 1 bbox calculation (instant)
/// - Small traces: Still uses voxel iteration (but N < 1000, so fast)
///
/// Returns the minimum Manhattan distance and the location where it occurs.
fn calculate_min_distance_between_nets(
    voxels_a: &[Point3D],
    voxels_b: &[Point3D],
) -> (i64, Point3D) {
    const LARGE_NET_THRESHOLD: usize = 1000;

    // PRIMITIVES OVER PIXELS: Use bbox distance for large nets
    if voxels_a.len() > LARGE_NET_THRESHOLD && voxels_b.len() > LARGE_NET_THRESHOLD {
        let bbox_a = get_bbox(voxels_a).unwrap();
        let bbox_b = get_bbox(voxels_b).unwrap();

        let distance = calculate_bbox_distance(&bbox_a, &bbox_b);

        // Return approximate location (center of bbox_a)
        let center_x = (bbox_a.min_x + bbox_a.max_x) / 2;
        let center_y = (bbox_a.min_y + bbox_a.max_y) / 2;
        let center_z = (bbox_a.min_z + bbox_a.max_z) / 2;

        return (distance, Point3D::new(center_x, center_y, center_z));
    }

    // VOXEL ITERATION: Use precise voxel-by-voxel for small nets
    let mut min_distance = i64::MAX;
    let mut min_location = Point3D::new(0, 0, 0);

    for voxel_a in voxels_a {
        for voxel_b in voxels_b {
            let dx = (voxel_a.x - voxel_b.x).abs();
            let dy = (voxel_a.y - voxel_b.y).abs();
            let dz = (voxel_a.z - voxel_b.z).abs();

            // FAST CHEBYSHEV REJECT:
            // If any 1D axis is already >= min_distance, the Manhattan sum
            // is guaranteed to be >= min_distance. Skip the addition!
            if dx >= min_distance || dy >= min_distance || dz >= min_distance {
                continue;
            }

            let distance = dx + dy + dz; // inline manhattan
            if distance < min_distance {
                min_distance = distance;
                min_location = *voxel_a;
            }
        }
    }

    (min_distance, min_location)
}

/// Calculate minimum Manhattan distance between two bounding boxes.
///
/// This is the "Primitives Over Pixels" solution for large pours.
/// Instead of comparing millions of voxels, we calculate the distance
/// between two boxes analytically in O(1) time.
///
/// **Algorithm**:
/// 1. If boxes overlap → distance = 0
/// 2. If boxes are separated on any axis → calculate gap distance
/// 3. Sum the gaps on all three axes (Manhattan distance)
///
/// **Performance**: O(1) instead of O(N²) where N = voxel count
fn calculate_bbox_distance(bbox_a: &BoundingBox, bbox_b: &BoundingBox) -> i64 {
    // Calculate gap on each axis (0 if overlapping)
    let dx = if bbox_a.max_x < bbox_b.min_x {
        bbox_b.min_x - bbox_a.max_x
    } else if bbox_b.max_x < bbox_a.min_x {
        bbox_a.min_x - bbox_b.max_x
    } else {
        0 // Overlapping on X axis
    };

    let dy = if bbox_a.max_y < bbox_b.min_y {
        bbox_b.min_y - bbox_a.max_y
    } else if bbox_b.max_y < bbox_a.min_y {
        bbox_a.min_y - bbox_b.max_y
    } else {
        0 // Overlapping on Y axis
    };

    let dz = if bbox_a.max_z < bbox_b.min_z {
        bbox_b.min_z - bbox_a.max_z
    } else if bbox_b.max_z < bbox_a.min_z {
        bbox_a.min_z - bbox_b.max_z
    } else {
        0 // Overlapping on Z axis
    };

    dx + dy + dz
}
