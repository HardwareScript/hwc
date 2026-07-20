//! Overlap classification and junction detection for DRC sweep.

use crate::geometry::Point3D;
use crate::geometry_router::route_decomposition::VirtualJunction;
use crate::geometry_router::spatial_index::IndexedSegment;
use crate::material::{MaterialId, MaterialRegistry};
use compact_str::CompactString;

use super::clearance::{compute_bbox_intersection_area, compute_overlap_area};
use super::sweep::segment_bbox;
use super::types::JunctionClassification;
use super::BridgeTable;

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
    SameNetIntersection {
        net_id: u32,
        mat_a: MaterialId,
        mat_b: MaterialId,
        intersection_area: i64,
    },
    /// v0.1.8: Material junction classification (coplanar face-touching).
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

        if let (Some(ma), Some(mb)) = (mat_a_id, mat_b_id) {
            if ma != mb {
                let a = segment_bbox(seg_a);
                let b = segment_bbox(seg_b);
                let intersection_area = compute_bbox_intersection_area(&a, &b);

                if intersection_area > 0 {
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
                        JunctionClassification::Allowed => OverlapResult::SameNetIntersection {
                            net_id: seg_a.net_id as u32,
                            mat_a: ma,
                            mat_b: mb,
                            intersection_area,
                        },
                    };
                }
            }
        }

        if let (Some(ma), Some(mb)) = (mat_a_id, mat_b_id) {
            if ma != mb {
                let a = segment_bbox(seg_a);
                let b = segment_bbox(seg_b);
                let intersection_area = compute_bbox_intersection_area(&a, &b);

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
        let actual_clearance = super::clearance::compute_actual_clearance(seg_a, seg_b);

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
pub fn classify_junction(
    mat_a_id: MaterialId,
    mat_b_id: MaterialId,
    registry: &MaterialRegistry,
    bridge_table: &BridgeTable,
) -> JunctionClassification {
    use crate::material::MaterialConductivity;

    let cat_a = match registry.get_conductivity(mat_a_id) {
        Some(c) => c,
        None => return JunctionClassification::Allowed,
    };
    let cat_b = match registry.get_conductivity(mat_b_id) {
        Some(c) => c,
        None => return JunctionClassification::Allowed,
    };

    let name_a = registry.get_name(mat_a_id).unwrap_or("Unknown");
    let name_b = registry.get_name(mat_b_id).unwrap_or("Unknown");

    match (cat_a, cat_b) {
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
        _ => JunctionClassification::Allowed,
    }
}

/// v0.1.8: Check if two AABBs touch at a face (coplanar boundary contact).
#[inline]
fn aabb_faces_touch(a: &super::sweep::SegmentBbox, b: &super::sweep::SegmentBbox) -> bool {
    let x_adjacent = a.max_x == b.min_x || b.max_x == a.min_x;
    let y_adjacent = a.max_y == b.min_y || b.max_y == a.min_y;

    let x_overlap = a.min_x < b.max_x && a.max_x > b.min_x;
    let y_overlap = a.min_y < b.max_y && a.max_y > b.min_y;

    (x_adjacent && y_overlap) || (y_adjacent && x_overlap)
}
