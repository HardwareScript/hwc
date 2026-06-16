//! Trace width validation logic.

use crate::constraint_manager::ConstraintRulebook;
use crate::geometry::Point3D;

use super::types::{DrcViolation, NetVoxels};

/// Validate trace widths for all nets.
///
/// Checks that all traces meet minimum width requirements for their current.
///
/// **Algorithm**:
/// 1. For each net
/// 2. Get required trace width from constraints
/// 3. Measure actual trace width in grid
/// 4. If actual < required → violation
///
/// **v0.1.4 Phase 3**: Now uses fabrication constraints from Symbol Table.
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 200-300, IPC-2221 formula)
///
/// # Arguments
/// * `nets` - All routed nets with their voxel locations
/// * `constraints` - Constraint rulebook with trace width requirements
/// * `voxel_size_nm` - Size of one voxel in nanometers
///
/// # Returns
/// Vector of trace width violations
pub fn validate_trace_widths(
    nets: &[NetVoxels],
    constraints: &ConstraintRulebook,
    voxel_size_nm: i64,
) -> Vec<DrcViolation> {
    use rayon::prelude::*;

    // Get required trace width from fabrication constraints (v0.1.4)
    let required_width_nm = constraints
        .fabrication
        .as_ref()
        .map(|fab| fab.min_trace_width_nm)
        .unwrap_or(100_000); // Default 0.1mm

    // Rayon parallelism for the slow path (small voxels)
    nets.par_iter()
        .filter(|net| net.net_name != "net_0") // v0.1.7: Ignore substrate net (net_0) for trace width checks
        .filter(|net| {
            matches!(
                net.geometry_type,
                crate::design_rule_check::GeometryType::Trace
            )
        }) // ✅ NATIVE v0.1.7 FIX: Only run trace-width checks on actual traces.
        .filter_map(|net| {
            let actual_width_nm = measure_trace_width(&net.voxels, voxel_size_nm);

            if actual_width_nm < required_width_nm {
                let location = net.voxels.first().copied().unwrap_or(Point3D::new(0, 0, 0));
                Some(DrcViolation::TraceWidthViolation {
                    net: net.net_name.clone(),
                    actual_nm: actual_width_nm,
                    required_nm: required_width_nm,
                    location,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Measure trace width for a net.
///
/// Finds the narrowest point in the trace by analyzing cross-sections.
///
/// **Algorithm**:
/// 1. Group voxels by Z-layer (traces are typically on single layers)
/// 2. For each layer, analyze the trace geometry
/// 3. Find perpendicular cross-sections along the trace path
/// 4. Count voxels in each cross-section to determine width
/// 5. Return the minimum width found
///
/// **Simplified v0.1.4 Implementation**:
/// - Assumes traces are Manhattan-routed (horizontal/vertical only)
/// - Measures width by counting adjacent voxels perpendicular to trace direction
/// - For now, returns single voxel width as baseline (conservative estimate)
///
/// **Future Enhancement**:
/// - Implement full cross-sectional analysis
/// - Handle diagonal traces
/// - Account for via transitions
fn measure_trace_width(voxels: &[Point3D], voxel_size_nm: i64) -> i64 {
    if voxels.is_empty() {
        return voxel_size_nm;
    }

    // For single voxel traces, width is one voxel
    if voxels.len() == 1 {
        return voxel_size_nm;
    }

    // Group voxels by layer (Z coordinate)
    use rustc_hash::FxHashMap;
    let mut layers: FxHashMap<i64, Vec<Point3D>> = FxHashMap::default();
    for voxel in voxels {
        layers.entry(voxel.z).or_default().push(*voxel);
    }

    // Find minimum width across all layers
    let mut min_width_voxels = i64::MAX;

    for (_z, layer_voxels) in layers.iter() {
        if layer_voxels.len() == 1 {
            min_width_voxels = min_width_voxels.min(1);
            continue;
        }

        // Analyze trace width on this layer
        // For Manhattan routing, we can measure width by looking at perpendicular extent
        let width = measure_layer_width(layer_voxels, voxel_size_nm);
        min_width_voxels = min_width_voxels.min(width);
    }

    // Convert voxel count to nanometers
    if min_width_voxels == i64::MAX {
        voxel_size_nm
    } else {
        min_width_voxels * voxel_size_nm
    }
}

/// Measure the width of a trace on a single layer.
///
/// For Manhattan-routed traces, this finds the minimum perpendicular extent.
///
/// **PERFORMANCE**: O(V) instead of O(V²) by passing grid_spacing_nm directly.
fn measure_layer_width(voxels: &[Point3D], grid_spacing_nm: i64) -> i64 {
    if voxels.len() <= 1 {
        return 1;
    }

    let spacing = grid_spacing_nm.max(1); // prevent divide by zero

    let mut min_x = i64::MAX;
    let mut max_x = i64::MIN;
    let mut min_y = i64::MAX;
    let mut max_y = i64::MIN;

    for v in voxels {
        let gx = v.x / spacing;
        let gy = v.y / spacing;

        if gx < min_x {
            min_x = gx;
        }
        if gx > max_x {
            max_x = gx;
        }
        if gy < min_y {
            min_y = gy;
        }
        if gy > max_y {
            max_y = gy;
        }
    }

    let width_x = (max_x - min_x) + 1;
    let width_y = (max_y - min_y) + 1;

    width_x.min(width_y)
}
