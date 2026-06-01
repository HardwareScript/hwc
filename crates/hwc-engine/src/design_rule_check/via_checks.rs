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

use super::types::{DrcReport, DrcViolation};

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

    // v0.1.7: Group enclosures by Z-range to handle overlapping pours (e.g. plane + pad).
    // A via only fails if NO pour on a given layer provides sufficient enclosure.
    let mut z_layer_max_enclosure: std::collections::HashMap<(i64, i64), i64> =
        std::collections::HashMap::new();

    // Find all conductive pours that intersect the via's Z-range
    for layer in substrate_layers {
        // ✅ NATIVE v0.1.7 FIX: Only enforce annular rings on POUR layers (pads, planes).
        // A 'Contact' layer (via barrel) should not be used to satisfy another via's enclosure.
        if layer.layer_type != crate::voxel_grid::SubstrateLayerType::Pour {
            continue;
        }

        // Only enforce annular rings on CONDUCTIVE layers.
        if !material_registry.is_conductor(layer.material) {
            continue;
        }

        // Check if this layer is on the same net as the via
        if layer.net != via_net_id {
            continue;
        }

        // ✅ v0.1.7 NATIVE FIX: A via connects to a pour if their bounding boxes intersect.
        // This is robust against rounding offsets, handles intermediate layer connections,
        // and eliminates the legacy strict start/end boundary checks.
        if !layer.bbox.intersects(via_bbox) {
            continue;
        }

        // Calculate distance to edge based on pad shape
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
                let dx = if via_center_x < pad_min_x {
                    pad_min_x - via_center_x
                } else if via_center_x > pad_max_x {
                    via_center_x - pad_max_x
                } else {
                    0
                };

                let dy = if via_center_y < pad_min_y {
                    pad_min_y - via_center_y
                } else if via_center_y > pad_max_y {
                    via_center_y - pad_max_y
                } else {
                    0
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
                    -1
                }
            }
        };

        let enclosure = dist_to_edge - via_radius;
        let z_range = (layer.bbox.min.z, layer.bbox.max.z);
        let entry = z_layer_max_enclosure.entry(z_range).or_insert(i64::MIN);
        *entry = (*entry).max(enclosure);
    }

    if z_layer_max_enclosure.is_empty() {
        return i64::MAX;
    }

    // The via's overall enclosure is the WORST among the BEST enclosures on each layer.
    for &enclosure in z_layer_max_enclosure.values() {
        min_annular_ring = min_annular_ring.min(enclosure);
    }

    min_annular_ring
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

/// Validate physical distance between vias (Drill-to-Drill Clearance) (v0.1.7).
///
/// This check enforces the minimum spacing between drill holes to prevent
/// drill bit breakage during manufacturing, even if the vias share the same net.
pub fn validate_drill_to_drill_clearance(
    contacts: &[ContactMetadata],
    constraints: &ConstraintRulebook,
) -> DrcReport {
    let mut report = DrcReport::new();

    let fabrication = match &constraints.fabrication {
        Some(fab) => fab,
        None => return report,
    };

    // Use via min_spacing constraint from profile (if defined)
    // If undefined, default to 2x the minimum via diameter (industry safety standard)
    let min_drill_spacing_nm = fabrication.min_spacing_nm;

    for i in 0..contacts.len() {
        for j in (i + 1)..contacts.len() {
            let via_a = &contacts[i];
            let via_b = &contacts[j];

            // v0.1.7: Robust drill clearance check (including same-net).
            // Drill hits must never overlap horizontally if they share any vertical span.
            let z_overlap = via_a.z_start_nm < via_b.z_end_nm && via_b.z_start_nm < via_a.z_end_nm;

            if via_a.net.is_some() && via_a.net == via_b.net {
                if !z_overlap {
                    // Same-net vias that don't overlap in Z are 'stacked' and safe.
                    continue;
                }
                // Same-net vias sharing a Z-range must still respect horizontal clearance
                // to prevent interpenetrating drill cylinders.
            } else {
                // Different nets: even touching at the Z-boundary (z_start == other_z_end)
                // is risky due to drill wandering, so we use a more conservative check.
                let z_touching_or_overlapping =
                    via_a.z_start_nm <= via_b.z_end_nm && via_a.z_end_nm >= via_b.z_start_nm;
                if !z_touching_or_overlapping {
                    continue;
                }
            }

            if let (Some(bbox_a), Some(bbox_b)) = (&via_a.bbox, &via_b.bbox) {
                // Calculate center-to-center distance in XY plane
                let center_a_x = (bbox_a.min.x + bbox_a.max.x) / 2;
                let center_a_y = (bbox_a.min.y + bbox_a.max.y) / 2;
                let center_b_x = (bbox_b.min.x + bbox_b.max.x) / 2;
                let center_b_y = (bbox_b.min.y + bbox_b.max.y) / 2;

                let dx = (center_a_x - center_b_x) as f64;
                let dy = (center_a_y - center_b_y) as f64;
                let center_dist_nm = (dx * dx + dy * dy).sqrt() as i64;

                // Calculate edge-to-edge drill clearance
                let radius_a = calculate_via_diameter_from_bbox(bbox_a) / 2;
                let radius_b = calculate_via_diameter_from_bbox(bbox_b) / 2;
                let drill_clearance_nm = center_dist_nm - radius_a - radius_b;

                if drill_clearance_nm < min_drill_spacing_nm {
                    let center = Point3D::new(
                        (center_a_x + center_b_x) / 2,
                        (center_a_y + center_b_y) / 2,
                        (bbox_a.min.z + bbox_b.max.z) / 2,
                    );

                    report.add_violation(DrcViolation::DrillClearanceViolation {
                        via_a: via_a.name.clone(),
                        via_b: via_b.name.clone(),
                        actual_nm: drill_clearance_nm,
                        required_nm: min_drill_spacing_nm,
                        location: center,
                    });
                }
            }
        }
    }

    if report.violations.is_empty() {
        report.add_info("All drill hits meet minimum spacing requirements".into());
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
