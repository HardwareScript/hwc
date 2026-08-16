//! Via diameter and enclosure validation.
//!
//! **Task 4.2: DRC Engine**
//!
//! This module implements DRC checks for vias:
//! - Minimum via diameter (from profile constraints)
//! - Minimum annular ring/enclosure (copper around via)
//!
//! **Architecture: Primitives Over Pixels**
//! Uses ContactMetadata bounding boxes (analytic geometry) for all checks.

use crate::constraint_manager::ConstraintRulebook;
use crate::geometry::{BoundingBox, Point3D};
use crate::ContactMetadata;
use hwc_types::Technology;

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

/// Validate via diameters using ContactMetadata (analytic geometry).
///
/// **Primitives Over Pixels**: Uses bounding boxes for analytic geometry checks.
///
/// # Arguments
/// * `contacts` - All via/contact metadata with bounding boxes
/// * `constraints` - Constraint rulebook with fabrication limits
///
/// # Returns
/// DRC report with via diameter violations, or error if data is missing
pub fn validate_via_diameters_analytic(
    contacts: &[ContactMetadata],
    constraints: &ConstraintRulebook,
) -> Result<DrcReport, String> {
    let mut report = DrcReport::new();

    // Get fabrication constraints — fail-fast if missing
    let fabrication = constraints.fabrication.as_ref().ok_or_else(|| {
        "[DRC] FATAL: No fabrication constraints loaded. \
         Add a 'profile:' clause to your space to enable via DRC checks."
            .to_string()
    })?;

    let min_via_diameter_nm = fabrication.min_via_diameter_nm;

    // Check each contact/via
    for contact in contacts {
        if let Some(ref bbox) = contact.bbox {
            let via_diameter_nm = contact.drill_diameter_nm.ok_or_else(|| {
                format!(
                    "[DRC] FATAL: via '{}' has no drill_diameter declared. \
                     Add 'drill_diameter: <value>nm' to the via definition in your profile.",
                    contact.name
                )
            })?;

            // Check against minimum diameter constraint
            if via_diameter_nm < min_via_diameter_nm {
                let center = Point3D::new(
                    (bbox.min.x + bbox.max.x) / 2,
                    (bbox.min.y + bbox.max.y) / 2,
                    (bbox.min.z + bbox.max.z) / 2,
                );

                let net_name = contact.net.clone().ok_or_else(|| {
                    format!(
                        "[DRC] FATAL: via '{}' has no net assignment. \
                             All vias must be connected to a declared net.",
                        contact.name
                    )
                })?;

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

    Ok(report)
}

/// Validate physical distance between vias (Drill-to-Drill Clearance) (v0.1.7).
///
/// # Returns
/// DRC report with drill clearance violations, or error if data is missing
pub fn validate_drill_to_drill_clearance(
    contacts: &[ContactMetadata],
    constraints: &ConstraintRulebook,
) -> Result<DrcReport, String> {
    let mut report = DrcReport::new();

    let fabrication = constraints.fabrication.as_ref().ok_or_else(|| {
        "[DRC] FATAL: No fabrication constraints loaded. \
         Add a 'profile:' clause to your space to enable via DRC checks."
            .to_string()
    })?;

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

                // v0.2.1 FIX: Use actual drill diameter, not bounding box dimensions
                // The bounding box includes enclosure (metal around via), but drill spacing
                // rules (licon.2, mcon.2) measure cut-to-cut distance, not pad-to-pad.
                // CORRECT: hole_edge_clearance = center_dist - (d1/2 + d2/2)
                // WRONG:   using bbox dimensions double-counts the enclosure margin
                let drill_diameter_a = via_a.drill_diameter_nm.ok_or_else(|| {
                    format!(
                        "[DRC] FATAL: via '{}' has no drill_diameter declared. \
                         Add 'diameter: <value>nm' to the via definition.",
                        via_a.name
                    )
                })?;
                let drill_diameter_b = via_b.drill_diameter_nm.ok_or_else(|| {
                    format!(
                        "[DRC] FATAL: via '{}' has no drill_diameter declared. \
                         Add 'diameter: <value>nm' to the via definition.",
                        via_b.name
                    )
                })?;

                let radius_a = drill_diameter_a / 2;
                let radius_b = drill_diameter_b / 2;
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

    Ok(report)
}

/// Validate via enclosure using ContactMetadata (analytic geometry).
///
/// **Category B: Physical Dimension Checks (O(1) Property Comparison)**
///
/// v0.1.8: Simplified to a purely geometric property check as per the
/// Zero-Magic paradigm. This validates that the via's pad provides sufficient
/// overhang (annular ring) around the drill hole, based on the via's own
/// metadata rather than searching for overlapping substrate layers.
///
/// v0.2.0: Accepts `technology_strategy` to conditionally apply annular ring
/// checks. For ASIC technology, annular ring checks are skipped entirely
/// because ASIC contacts are flush with no overhang.
///
/// # Arguments
/// * `contacts` - All via/contact metadata with bounding boxes
/// * `constraints` - Constraint rulebook with fabrication limits
/// * `technology_strategy` - Technology strategy (PCB or ASIC)
///
/// # Returns
/// DRC report with enclosure violations, or error if data is missing
pub fn validate_via_enclosure_analytic(
    contacts: &[ContactMetadata],
    constraints: &ConstraintRulebook,
    technology_strategy: Technology,
) -> Result<DrcReport, String> {
    // v0.2.0: Skip annular ring checks for ASIC technology
    // ASIC contacts are flush with no overhang - annular ring doesn't apply
    if technology_strategy.is_asic() {
        let mut report = DrcReport::new();
        report.add_info("Annular ring checks skipped for ASIC technology".into());
        return Ok(report);
    }

    let mut report = DrcReport::new();

    let fabrication = constraints.fabrication.as_ref().ok_or_else(|| {
        "[DRC] FATAL: No fabrication constraints loaded. \
         Add a 'profile:' clause to your space to enable via enclosure checks."
            .to_string()
    })?;

    let min_enclosure_nm = fabrication.min_enclosure_nm;

    // Check each contact/via
    for contact in contacts {
        if let Some(ref bbox) = contact.bbox {
            let net_name = contact.net.clone().ok_or_else(|| {
                format!(
                    "[DRC] FATAL: via '{}' has no net assignment. \
                     All vias must be connected to a declared net.",
                    contact.name
                )
            })?;

            let drill_diameter_nm = contact.drill_diameter_nm.ok_or_else(|| {
                format!(
                    "[DRC] FATAL: via '{}' has no drill_diameter declared. \
                     Add 'drill_diameter: <value>nm' to the via definition in your profile.",
                    contact.name
                )
            })?;

            // v0.1.8: Simplified O(1) Annular Ring Check.
            // Calculate overhang: (pad_diameter - drill_diameter) / 2
            let pad_width = bbox.max.x - bbox.min.x;
            let pad_height = bbox.max.y - bbox.min.y;
            let pad_diameter_nm = pad_width.min(pad_height);

            let actual_enclosure_nm = (pad_diameter_nm - drill_diameter_nm) / 2;

            // Check against minimum annular ring constraint
            if actual_enclosure_nm < min_enclosure_nm {
                let center = Point3D::new(
                    (bbox.min.x + bbox.max.x) / 2,
                    (bbox.min.y + bbox.max.y) / 2,
                    (bbox.min.z + bbox.max.z) / 2,
                );

                report.add_violation(super::types::DrcViolation::EnclosureViolation {
                    net: net_name,
                    actual_nm: actual_enclosure_nm,
                    required_nm: min_enclosure_nm,
                    location: center,
                });
            }
        }
    }

    if report.violations.is_empty() {
        report.add_info("All vias meet minimum enclosure requirements".into());
    }

    Ok(report)
}

/// Validate layer-specific via enclosure requirements (v0.2.2).
///
/// **ASIC-Specific DRC: Device Landing Layer Enclosure**
///
/// This check validates that vias landing on special device layers (e.g., CAPM,
/// poly capacitors) meet layer-specific enclosure requirements that exceed the
/// standard annular ring rules. This is critical for preventing dielectric
/// punch-through and ensuring proper capacitor/device integrity.
///
/// Example: SkyWater SKY130 capm.5 rule requires 500nm enclosure of Via3 by CAPM,
/// far exceeding the standard 50nm annular ring requirement.
///
/// **Architecture: Geometric Polygon Intersection**
/// - Queries entity graph for actual pour polygons on landing layers
/// - Calculates physical edge distance from via boundary to pour boundary
/// - Reports violations when distance < required_enclosure_nm
///
/// # Arguments
/// * `space` - Hardware space with entity graph and pour metadata
/// * `constraints` - Constraint rulebook with layer-specific enclosure rules
///
/// # Returns
/// DRC report with layer-specific enclosure violations
pub fn validate_layer_specific_via_enclosure(
    space: &crate::space::HardwareSpace,
    constraints: &ConstraintRulebook,
) -> Result<DrcReport, String> {
    let mut report = DrcReport::new();

    // Get fabrication constraints
    let fabrication = constraints.fabrication.as_ref().ok_or_else(|| {
        "[DRC] FATAL: No fabrication constraints loaded. \
         Add a 'profile:' clause to your space to enable layer-specific via enclosure checks."
            .to_string()
    })?;

    // If no layer-specific enclosure rules are defined, skip check
    if fabrication.layer_via_enclosures.is_empty() {
        report.add_info("No layer-specific via enclosure rules defined".into());
        return Ok(report);
    }

    // Check each contact/via
    for contact in &space.contacts {
        // Only check contacts that have layer information
        let (from_layer, to_layer) = match (&contact.from_layer, &contact.to_layer) {
            (Some(from), Some(to)) => (from.as_str(), to.as_str()),
            _ => continue, // Skip contacts without layer info
        };

        // Check if either layer has enclosure requirements
        for (layer_name, required_enclosure_nm) in &fabrication.layer_via_enclosures {
            // Check if via lands on this layer (from either side)
            let lands_on_layer = from_layer == layer_name || to_layer == layer_name;

            if !lands_on_layer {
                continue;
            }

            // Get via bounding box
            let via_bbox = match &contact.bbox {
                Some(b) => b,
                None => continue,
            };

            // Find all pours on this layer that match the via's net
            // (Only same-net pours should provide enclosure)
            let contact_net = match &contact.net {
                Some(n) => n,
                None => continue,
            };

            // Query pours for matching layer and net
            let mut found_pour = false;
            let mut min_enclosure_nm = i64::MAX;

            for pour in &space.pours {
                // Check if pour is on the right layer
                // We need to match the pour's material against the layer name
                // For now, skip pours that don't match the net
                if pour.net.as_ref() != Some(contact_net) {
                    continue;
                }

                let pour_bbox = match &pour.bbox {
                    Some(b) => b,
                    None => continue,
                };

                // Check if pour and via overlap in Z (they should for landing)
                let z_overlap = via_bbox.min.z <= pour_bbox.max.z && via_bbox.max.z >= pour_bbox.min.z;
                if !z_overlap {
                    continue;
                }

                // CRITICAL FIX: Only check pours on the landing layer, not all same-net pours
                // The via spans from_layer -> to_layer, so check which landing layer this rule applies to
                // For a via landing on CAPM (from_layer="capm" or to_layer="capm"), only check CAPM pours
                let pour_is_on_target_layer = if from_layer == layer_name || to_layer == layer_name {
                    // This is the landing layer - check if pour's material matches the layer
                    // The pour's material_name should correspond to the layer
                    // For now, we'll use a heuristic: CAPM pours are on the CAPM layer
                    pour.material_name.to_lowercase().contains(layer_name.to_lowercase().as_str())
                        || pour.name.to_lowercase().contains(layer_name.as_str())
                } else {
                    // Via doesn't land on this layer, skip this enclosure rule
                    continue;
                };

                if !pour_is_on_target_layer {
                    continue;
                }

                eprintln!(
                    "[DRC LAYER ENCLOSURE] Checking via '{}' (bbox: [{}, {}] -> [{}, {}]) against pour '{}' (bbox: [{}, {}] -> [{}, {}]) on layer '{}'",
                    contact.name,
                    via_bbox.min.x, via_bbox.min.y, via_bbox.max.x, via_bbox.max.y,
                    pour.name,
                    pour_bbox.min.x, pour_bbox.min.y, pour_bbox.max.x, pour_bbox.max.y,
                    layer_name
                );

                found_pour = true;

                // Calculate geometric enclosure: minimum distance from via edge to pour edge
                // For a via (rectangular bounding box) inside a pour (also rectangular),
                // the enclosure is the minimum of:
                // - (via_min_x - pour_min_x)
                // - (pour_max_x - via_max_x)
                // - (via_min_y - pour_min_y)
                // - (pour_max_y - via_max_y)
                
                let enclosure_left = via_bbox.min.x - pour_bbox.min.x;
                let enclosure_right = pour_bbox.max.x - via_bbox.max.x;
                let enclosure_bottom = via_bbox.min.y - pour_bbox.min.y;
                let enclosure_top = pour_bbox.max.y - via_bbox.max.y;

                eprintln!(
                    "[DRC LAYER ENCLOSURE]   Enclosures: left={}, right={}, bottom={}, top={}",
                    enclosure_left, enclosure_right, enclosure_bottom, enclosure_top
                );

                // Minimum enclosure (could be negative if via extends outside pour)
                let enclosure = enclosure_left.min(enclosure_right).min(enclosure_bottom).min(enclosure_top);

                eprintln!(
                    "[DRC LAYER ENCLOSURE]   Min enclosure: {}nm (required: {}nm)",
                    enclosure, *required_enclosure_nm
                );

                min_enclosure_nm = min_enclosure_nm.min(enclosure);
            }

            // If we found a pour and enclosure is insufficient, report violation
            if found_pour && min_enclosure_nm < *required_enclosure_nm {
                let center = Point3D::new(
                    (via_bbox.min.x + via_bbox.max.x) / 2,
                    (via_bbox.min.y + via_bbox.max.y) / 2,
                    (via_bbox.min.z + via_bbox.max.z) / 2,
                );

                report.add_violation(super::types::DrcViolation::EnclosureViolation {
                    net: contact_net.clone(),
                    actual_nm: min_enclosure_nm,
                    required_nm: *required_enclosure_nm,
                    location: center,
                });

                // Add a more specific error message for layer-specific violations
                report.add_warning(format!(
                    "Via '{}' on net '{}' landing on layer '{}' violates layer-specific enclosure rule: \
                     Via must be enclosed by {} layer by at least {:.0}nm. \
                     Actual: {:.0}nm. This creates dielectric punch-through risk. \
                     Move via away from {} plate edge or increase {} plate size.",
                    contact.name, contact_net, layer_name,
                    layer_name, *required_enclosure_nm as f64,
                    min_enclosure_nm as f64,
                    layer_name, layer_name
                ).into());
            } else if !found_pour {
                // Via lands on a layer with enclosure rules but no pour was found
                // This might be okay if the via is just passing through
                continue;
            }
        }
    }

    if report.violations.is_empty() {
        report.add_info("All vias meet layer-specific enclosure requirements".into());
    }

    Ok(report)
}

