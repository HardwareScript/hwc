//! Via diameter and enclosure validation.
//!
//! **Task 4.2: DRC Engine**
//!
//! This module implements DRC checks for vias:
//! - Minimum via diameter (from profile constraints)
//! - Minimum annular ring/enclosure (copper around via)
//!
//! **Architecture: Primitives Over Pixels**
//! Uses ContactMetadata bounding boxes (analytic geometry) instead of voxel sampling.

use crate::constraint_manager::ConstraintRulebook;
use crate::geometry::{BoundingBox, Point3D};
use crate::ContactMetadata;

use super::types::DrcReport;

// ============================================================================
// PRIMITIVES OVER PIXELS: Analytic Via Validation (v0.1.6)
// ============================================================================

/// Calculate via diameter from bounding box (analytic geometry).
///
/// Assumes circular via and calculates diameter from XY dimensions.
/// For a rectangular via, uses the smaller dimension as diameter.
fn calculate_via_diameter_from_bbox(bbox: &BoundingBox) -> i64 {
    let width = bbox.max.x - bbox.min.x;
    let height = bbox.max.y - bbox.min.y;

    // Use the smaller dimension (conservative estimate)
    width.min(height)
}

/// Find substrate layers (pours) that overlap with a via for annular ring calculation.
///
/// Returns the minimum distance from via edge to pad edge.
fn calculate_annular_ring_from_substrate(
    via_bbox: &BoundingBox,
    via_diameter_nm: i64,
    substrate_layers: &[crate::voxel_grid::SubstrateLayer],
    via_net_id: u32,
    material_registry: &crate::voxel::MaterialRegistry,
) -> i64 {
    let via_center_x = (via_bbox.min.x + via_bbox.max.x) / 2;
    let via_center_y = (via_bbox.min.y + via_bbox.max.y) / 2;
    let via_radius = via_diameter_nm / 2;

    let mut min_annular_ring = i64::MAX;

    // Find pads on the via's start/end layers
    for layer in substrate_layers {
        // ✅ NATIVE v0.1.7 FIX: Only enforce annular rings on CONDUCTIVE layers.
        // Vias passing through insulators (FR4, Air) do not need landing pads.
        if !material_registry.is_conductor(layer.material) {
            continue;
        }

        // Check if this layer is on the same net as the via
        if layer.net != via_net_id {
            continue;
        }

        // ✅ v0.1.7 FIXED: Ignore "Self-Enclosure". 
        // If the "pad" we found is actually just the via itself (same bbox), skip it.
        // A via only needs an annular ring on a pad that is larger than itself.
        if layer.bbox.min.x == via_bbox.min.x && layer.bbox.max.x == via_bbox.max.x &&
           layer.bbox.min.y == via_bbox.min.y && layer.bbox.max.y == via_bbox.max.y {
            continue; 
        }

        // Check if layer overlaps with via in Z
        if layer.bbox.max.z < via_bbox.min.z || layer.bbox.min.z > via_bbox.max.z {
            continue; // No Z overlap
        }

        // Check if layer overlaps with via in XY
        if layer.bbox.max.x < via_bbox.min.x
            || layer.bbox.min.x > via_bbox.max.x
            || layer.bbox.max.y < via_bbox.min.y
            || layer.bbox.min.y > via_bbox.max.y
        {
            continue; // No XY overlap
        }

        // Calculate distance to edge based on pad shape (v0.1.7: Primitives Over Pixels)
        let pad_min_x = layer.bbox.min.x;
        let pad_max_x = layer.bbox.max.x;
        let pad_min_y = layer.bbox.min.y;
        let pad_max_y = layer.bbox.max.y;

        let dist_to_edge = match layer.shape {
            crate::voxel_grid::SubstrateLayerShape::Cylinder { diameter, .. } => {
                let pad_center_x = (pad_min_x + pad_max_x) / 2;
                let pad_center_y = (pad_min_y + pad_max_y) / 2;
                let pad_radius = diameter / 2;

                let dx = (via_center_x - pad_center_x) as f64;
                let dy = (via_center_y - pad_center_y) as f64;
                let center_dist = (dx * dx + dy * dy).sqrt() as i64;

                pad_radius - center_dist
            }
            crate::voxel_grid::SubstrateLayerShape::Tube { pad_diameter, .. } => {
                let pad_center_x = (pad_min_x + pad_max_x) / 2;
                let pad_center_y = (pad_min_y + pad_max_y) / 2;
                let pad_radius = pad_diameter as i64 / 2;

                let dx = (via_center_x - pad_center_x) as f64;
                let dy = (via_center_y - pad_center_y) as f64;
                let center_dist = (dx * dx + dy * dy).sqrt() as i64;

                pad_radius - center_dist
            }
            crate::voxel_grid::SubstrateLayerShape::Rect => {
                // Find closest pad edge to via center (AABB logic)
                let dx = if via_center_x < pad_min_x {
                    pad_min_x - via_center_x
                } else if via_center_x > pad_max_x {
                    via_center_x - pad_max_x
                } else {
                    0 // Via center is inside pad in X
                };

                let dy = if via_center_y < pad_min_y {
                    pad_min_y - via_center_y
                } else if via_center_y > pad_max_y {
                    via_center_y - pad_max_y
                } else {
                    0 // Via center is inside pad in Y
                };

                if dx == 0 && dy == 0 {
                    let dist_to_left = via_center_x - pad_min_x;
                    let dist_to_right = pad_max_x - via_center_x;
                    let dist_to_bottom = via_center_y - pad_min_y;
                    let dist_to_top = pad_max_y - via_center_y;

                    dist_to_left
                        .min(dist_to_right)
                        .min(dist_to_bottom)
                        .min(dist_to_top)
                } else {
                    -1 // Outside pad
                }
            }
        };

        // If via center is inside pad, calculate annular ring
        if dist_to_edge >= 0 {
            // Annular ring = distance from via edge to pad edge
            let annular_ring = dist_to_edge - via_radius;
            
            if annular_ring < min_annular_ring {
                min_annular_ring = annular_ring;
            }
        }
    }

    // v0.1.7 FIXED: If no pads on the same net were found, return MAX instead of 0.
    // This prevents enclosure violations on dielectric layers or layers where the via 
    // is just passing through without a connection. The Physical Continuity Checker (P41)
    // already ensures that the via is connected to its net.
    if min_annular_ring == i64::MAX {
        i64::MAX
    } else {
        // v0.1.7: Don't clamp to 0. Negative values indicate the via is larger than the pad.
        min_annular_ring
    }
}

/// Validate via diameters using ContactMetadata (analytic geometry).
///
/// **Primitives Over Pixels**: Uses bounding boxes instead of voxel sampling.
///
/// # Arguments
/// * `contacts` - All via/contact metadata with bounding boxes
/// * `constraints` - Constraint rulebook with fabrication limits
///
/// # Returns
/// DRC report with via diameter violations
pub fn validate_via_diameters_analytic(
    contacts: &[ContactMetadata],
    constraints: &ConstraintRulebook,
) -> DrcReport {
    let mut report = DrcReport::new();

    // Get fabrication constraints
    let fabrication = match &constraints.fabrication {
        Some(fab) => fab,
        None => {
            report.add_info(
                "No fabrication constraints defined - skipping via diameter check".into(),
            );
            return report;
        }
    };

    let min_via_diameter_nm = fabrication.min_via_diameter_nm;

    // Check each contact/via
    for contact in contacts {
        if let Some(ref bbox) = contact.bbox {
            // Calculate via diameter from bounding box
            let via_diameter_nm = calculate_via_diameter_from_bbox(bbox);

            // Check against minimum diameter constraint
            if via_diameter_nm < min_via_diameter_nm {
                let center = Point3D::new(
                    (bbox.min.x + bbox.max.x) / 2,
                    (bbox.min.y + bbox.max.y) / 2,
                    (bbox.min.z + bbox.max.z) / 2,
                );

                let net_name = contact.net.clone().unwrap_or_else(|| "unknown".into());

                report.add_violation(super::types::DrcViolation::ViaDiameterViolation {
                    net: net_name,
                    actual_nm: via_diameter_nm,
                    required_nm: min_via_diameter_nm,
                    location: center,
                });
            }
        }
    }

    if report.violations.is_empty() {
        report.add_info("All vias meet minimum diameter requirements".into());
    }

    report
}

/// Validate via enclosure using ContactMetadata and substrate layers (analytic geometry).
///
/// **Primitives Over Pixels**: Uses bounding boxes instead of voxel sampling.
///
/// # Arguments
/// * `contacts` - All via/contact metadata with bounding boxes
/// * `substrate_layers` - Substrate layers (pours) for annular ring calculation
/// * `constraints` - Constraint rulebook with fabrication limits
/// * `netlist` - Netlist for looking up net IDs from net names
///
/// # Returns
/// DRC report with enclosure violations
pub fn validate_via_enclosure_analytic(
    contacts: &[ContactMetadata],
    substrate_layers: &[crate::voxel_grid::SubstrateLayer],
    constraints: &ConstraintRulebook,
    netlist: &crate::netlist::NetlistArena,
    material_registry: &crate::voxel::MaterialRegistry,
) -> DrcReport {
    let mut report = DrcReport::new();

    // Get fabrication constraints
    let fabrication = match &constraints.fabrication {
        Some(fab) => fab,
        None => {
            report.add_info(
                "No fabrication constraints defined - skipping via enclosure check".into(),
            );
            return report;
        }
    };

    let min_annular_ring_nm = fabrication.min_annular_ring_nm;

    // Check each contact/via
    for contact in contacts {
        if let Some(ref bbox) = contact.bbox {
            if let Some(ref net_name) = contact.net {
                // Look up net ID from netlist
                if let Some(net_data) = netlist.get_net_by_name(net_name.as_str()) {
                    let net_id = net_data.raw();

                    // Calculate via diameter
                    let via_diameter_nm = calculate_via_diameter_from_bbox(bbox);

                    // Calculate annular ring from substrate layers
                    let annular_ring_nm = calculate_annular_ring_from_substrate(
                        bbox,
                        via_diameter_nm,
                        substrate_layers,
                        net_id,
                        material_registry,
                    );

                    // Check against minimum annular ring constraint
                    if annular_ring_nm < min_annular_ring_nm && annular_ring_nm != i64::MAX {
                        let center = Point3D::new(
                            (bbox.min.x + bbox.max.x) / 2,
                            (bbox.min.y + bbox.max.y) / 2,
                            (bbox.min.z + bbox.max.z) / 2,
                        );

                        report.add_violation(super::types::DrcViolation::EnclosureViolation {
                            net: net_name.clone(),
                            actual_nm: annular_ring_nm,
                            required_nm: min_annular_ring_nm,
                            location: center,
                        });
                    }
                }
            }
        }
    }

    if report.violations.is_empty() {
        report.add_info("All vias meet minimum enclosure requirements".into());
    }

    report
}
