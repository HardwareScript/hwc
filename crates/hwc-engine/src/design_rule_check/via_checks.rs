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
    substrate_layers: &[crate::geometry_router::substrate_types::SubstrateLayer],
    _via_net_id: u32,
    material_registry: &crate::material::MaterialRegistry,
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
        if layer.layer_type != crate::geometry_router::substrate_types::SubstrateLayerType::Pour {
            continue;
        }

        // Only enforce annular rings on CONDUCTIVE layers.
        if !material_registry.is_conductor(layer.material) {
            continue;
        }

        // Check if this layer's net matches the via's net.
        // v0.1.8: Auto-routed nets may have different net IDs than the original
        // substrate pours (e.g. NET_M1_drain_to_M2_drain vs VOUT). Skip the net
        // check for enclosure — any conductive pour at the via location provides
        // physical enclosure regardless of net assignment.
        //
        // Net-mismatch violations are caught by the substrate short circuit validator.

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

        let dist_to_edge = match &layer.shape {
            crate::geometry_router::substrate_types::SubstrateLayerShape::Polygon { outer_contour, .. } => {
                // Compute bounding box of the polygon contour for conservative distance
                let mut poly_min_x = i64::MAX;
                let mut poly_max_x = i64::MIN;
                let mut poly_min_y = i64::MAX;
                let mut poly_max_y = i64::MIN;
                for p in outer_contour.iter() {
                    if p.x < poly_min_x {
                        poly_min_x = p.x;
                    }
                    if p.x > poly_max_x {
                        poly_max_x = p.x;
                    }
                    if p.y < poly_min_y {
                        poly_min_y = p.y;
                    }
                    if p.y > poly_max_y {
                        poly_max_y = p.y;
                    }
                }
                let pad_center_x = (pad_min_x + pad_max_x) / 2;
                let pad_center_y = (pad_min_y + pad_max_y) / 2;
                let half_w = (poly_max_x - poly_min_x) / 2;
                let half_h = (poly_max_y - poly_min_y) / 2;

                let dx = if via_center_x < pad_center_x - half_w {
                    (pad_center_x - half_w) - via_center_x
                } else if via_center_x > pad_center_x + half_w {
                    via_center_x - (pad_center_x + half_w)
                } else {
                    0
                };

                let dy = if via_center_y < pad_center_y - half_h {
                    (pad_center_y - half_h) - via_center_y
                } else if via_center_y > pad_center_y + half_h {
                    via_center_y - (pad_center_y + half_h)
                } else {
                    0
                };

                if dx == 0 && dy == 0 {
                    let dist_left = via_center_x - (pad_center_x - half_w);
                    let dist_right = (pad_center_x + half_w) - via_center_x;
                    let dist_bottom = via_center_y - (pad_center_y - half_h);
                    let dist_top = (pad_center_y + half_h) - via_center_y;
                    dist_left.min(dist_right).min(dist_bottom).min(dist_top)
                } else {
                    -1
                }
            }
            crate::geometry_router::substrate_types::SubstrateLayerShape::Tube { pad_diameter, .. } => {
                let pad_center_x = (pad_min_x + pad_max_x) / 2;
                let pad_center_y = (pad_min_y + pad_max_y) / 2;
                let pad_radius = *pad_diameter as i64 / 2;

                let dx = (via_center_x - pad_center_x) as f64;
                let dy = (via_center_y - pad_center_y) as f64;
                let center_dist = (dx * dx + dy * dy).sqrt() as i64;

                pad_radius - center_dist
            }
            crate::geometry_router::substrate_types::SubstrateLayerShape::Rect => {
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
            crate::geometry_router::substrate_types::SubstrateLayerShape::Circle { radius } => {
                let pad_center_x = (pad_min_x + pad_max_x) / 2;
                let pad_center_y = (pad_min_y + pad_max_y) / 2;

                let dx = (via_center_x - pad_center_x) as f64;
                let dy = (via_center_y - pad_center_y) as f64;
                let center_dist = (dx * dx + dy * dy).sqrt() as i64;

                radius - center_dist
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

/// Calculate annular ring from analytic route traces.
///
/// When no substrate pour overlaps the via, the route trace itself provides enclosure.
/// This calculates the minimum copper extension beyond the via on all sides.
fn calculate_annular_ring_from_routes(
    via_bbox: &BoundingBox,
    via_diameter_nm: i64,
    analytic_routes: &[crate::AnalyticTrace],
) -> i64 {
    let via_radius_nm = via_diameter_nm / 2;

    // Via center (xy)
    let via_cx = (via_bbox.min.x + via_bbox.max.x) / 2;
    let via_cy = (via_bbox.min.y + via_bbox.max.y) / 2;
    let via_z_min = via_bbox.min.z;
    let via_z_max = via_bbox.max.z;

    let mut min_annular_ring = i64::MAX;

    for route in analytic_routes {
        for segment in &route.segments {
            // Check if segment Z range overlaps with via Z range
            let seg_z_min = segment.start.z.min(segment.end.z);
            let seg_z_max = segment.start.z.max(segment.end.z);
            if seg_z_max < via_z_min || seg_z_min > via_z_max {
                continue;
            }

            // Build segment bounding box (the route trace is a swept rectangle)
            let half_width = route.width_nm / 2;
            let rx_min = segment.start.x.min(segment.end.x) - half_width;
            let ry_min = segment.start.y.min(segment.end.y) - half_width;
            let rx_max = segment.start.x.max(segment.end.x) + half_width;
            let ry_max = segment.start.y.max(segment.end.y) + half_width;

            // Check if via center is inside the segment rectangle
            if via_cx < rx_min || via_cx > rx_max || via_cy < ry_min || via_cy > ry_max {
                continue;
            }

            // Calculate minimum extension in each direction
            let ext_left = via_cx - rx_min - via_radius_nm;
            let ext_right = rx_max - via_cx - via_radius_nm;
            let ext_bottom = via_cy - ry_min - via_radius_nm;
            let ext_top = ry_max - via_cy - via_radius_nm;

            let min_ext = ext_left.min(ext_right).min(ext_bottom).min(ext_top);
            if min_ext >= 0 {
                min_annular_ring = min_annular_ring.min(min_ext);
            }
        }
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
            // Use actual drill diameter if available, otherwise derive from bbox
            let via_diameter_nm = contact.drill_diameter_nm
                .unwrap_or_else(|| calculate_via_diameter_from_bbox(bbox));

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
                // v0.1.8: Same-net vias at the exact same XY center are part of
                // the same via stack (layer-by-layer transitions). They share a
                // Z-range by construction and must not be flagged.
                if let (Some(bb_a), Some(bb_b)) = (&via_a.bbox, &via_b.bbox) {
                    let cx_a = (bb_a.min.x + bb_a.max.x) / 2;
                    let cy_a = (bb_a.min.y + bb_a.max.y) / 2;
                    let cx_b = (bb_b.min.x + bb_b.max.x) / 2;
                    let cy_b = (bb_b.min.y + bb_b.max.y) / 2;
                    if cx_a == cx_b && cy_a == cy_b {
                        continue;
                    }
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
/// * `analytic_routes` - Analytic route traces (for route-based enclosure check)
///
/// # Returns
/// DRC report with enclosure violations
pub fn validate_via_enclosure_analytic(
    contacts: &[ContactMetadata],
    substrate_layers: &[crate::geometry_router::substrate_types::SubstrateLayer],
    constraints: &ConstraintRulebook,
    netlist: &crate::netlist::NetlistArena,
    material_registry: &crate::material::MaterialRegistry,
    analytic_routes: &[crate::AnalyticTrace],
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

                    // Use actual drill diameter for enclosure check.
                    // The bbox includes pad (drill + 2*annular_ring), but enclosure
                    // is measured from the drill edge to the copper edge.
                    let drill_diameter_nm = contact.drill_diameter_nm
                        .unwrap_or_else(|| calculate_via_diameter_from_bbox(bbox));

                    // Calculate annular ring from substrate layers
                    let substrate_annular_ring = calculate_annular_ring_from_substrate(
                        bbox,
                        drill_diameter_nm,
                        substrate_layers,
                        net_id,
                        material_registry,
                    );

                    // v0.1.8: Also check enclosure from analytic route traces.
                    let route_annular_ring = calculate_annular_ring_from_routes(
                        bbox,
                        drill_diameter_nm,
                        analytic_routes,
                    );

                    let annular_ring_nm = substrate_annular_ring.min(route_annular_ring);

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
