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

    let min_annular_ring_nm = fabrication.min_annular_ring_nm;

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

            let actual_annular_ring_nm = (pad_diameter_nm - drill_diameter_nm) / 2;

            // Check against minimum annular ring constraint
            if actual_annular_ring_nm < min_annular_ring_nm {
                let center = Point3D::new(
                    (bbox.min.x + bbox.max.x) / 2,
                    (bbox.min.y + bbox.max.y) / 2,
                    (bbox.min.z + bbox.max.z) / 2,
                );

                report.add_violation(super::types::DrcViolation::EnclosureViolation {
                    net: net_name,
                    actual_nm: actual_annular_ring_nm,
                    required_nm: min_annular_ring_nm,
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
