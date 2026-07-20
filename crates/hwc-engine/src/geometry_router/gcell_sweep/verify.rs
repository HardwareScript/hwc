//! G-Cell sweep verification engine.

use crate::geometry_router::partition::PartitionGrid;
use crate::geometry_router::route_decomposition::VirtualJunction;
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::material::{MaterialId, MaterialRegistry};

use super::ghost::GhostRegistry;
use super::morton::sort_segments_by_morton;
use super::overlap::{classify_overlap, OverlapQuery, OverlapResult};
use super::sweep::{segment_bbox, FlatIntervalSweep, SegmentBbox};
use super::types::{BridgeTable, JunctionClassification, SweepViolation, ViolationType};

struct GCellSweepContext {
    segments: Vec<IndexedSegment>,
    ghost_registry: GhostRegistry,
}

/// Verify all G-cells using std::thread::scope parallelism.
///
/// Each G-cell is processed on a separate thread via coarse-grained chunks
/// across CPU cores. No global memory locks — each thread collects violations
/// locally. Returns a merged `Vec<SweepViolation>` of all DRC violations found.
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
                let actual = super::clearance::compute_actual_clearance(seg_a, seg_b);
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
            } => match classification {
                JunctionClassification::Forbidden => {
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
                JunctionClassification::BridgeRequired { .. } => {}
                JunctionClassification::Allowed => {}
            },
            OverlapResult::NoOverlap => {}
        }
    }

    violations
}
