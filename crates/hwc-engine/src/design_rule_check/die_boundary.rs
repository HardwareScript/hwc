//! Die boundary validation.
//!
//! Checks that every piece of geometry (pours, contacts, vias) is fully
//! contained within the declared `space.dimensions`. Any geometry that
//! overflows the die/board boundary triggers a `DieBoundaryViolation` error,
//! which would previously silently produce a visually malformed 3D export.

use compact_str::CompactString;

use crate::geometry::Point3D;
use crate::space::HardwareSpace;

use super::types::DrcViolation;

/// Validate that all geometry is contained within `space.dimensions`.
///
/// Checks every pour polygon vertex, contact bounding box, and via position
/// against `[0, width_nm] × [0, height_nm]`.
pub fn validate_die_boundary(space: &HardwareSpace) -> Result<Vec<DrcViolation>, String> {
    let mut violations = Vec::new();

    let width_nm = space.dimensions.width_nm;
    let height_nm = space.dimensions.height_nm;

    // ── Pours ──────────────────────────────────────────────────────────────────
    for pour in &space.pours {
        let name: CompactString = format!("pour:{}", pour.layer_name).into();
        if let Some(ref bbox) = pour.bbox {
            for &(x, y) in &[
                (bbox.min.x, bbox.min.y),
                (bbox.max.x, bbox.min.y),
                (bbox.max.x, bbox.max.y),
                (bbox.min.x, bbox.max.y),
            ] {
                check_xy(&mut violations, &name, x, y, 0, width_nm, height_nm);
            }
        }
    }

    // ── Contacts / IC vias ─────────────────────────────────────────────────────
    for contact in &space.contacts {
        let name: CompactString = contact.name.clone();
        if let Some(ref bbox) = contact.bbox {
            // Check all four corners of the contact footprint
            for &(x, y) in &[
                (bbox.min.x, bbox.min.y),
                (bbox.max.x, bbox.min.y),
                (bbox.max.x, bbox.max.y),
                (bbox.min.x, bbox.max.y),
            ] {
                check_xy(&mut violations, &name, x, y, 0, width_nm, height_nm);
            }
        }
    }

    // ── PCB Vias ───────────────────────────────────────────────────────────────
    for via in &space.vias {
        let name: CompactString = "pcb_via".into();
        let r = via.diameter_nm / 2;
        for &(x, y) in &[
            (via.position.0 - r, via.position.1),
            (via.position.0 + r, via.position.1),
            (via.position.0, via.position.1 - r),
            (via.position.0, via.position.1 + r),
        ] {
            check_xy(&mut violations, &name, x, y, 0, width_nm, height_nm);
        }
    }

    Ok(violations)
}

/// Check a single (x, y) point and push a violation if it overflows.
fn check_xy(
    violations: &mut Vec<DrcViolation>,
    element: &CompactString,
    x: i64,
    y: i64,
    _z: i64,
    width_nm: i64,
    height_nm: i64,
) {
    if x < 0 {
        violations.push(DrcViolation::DieBoundaryViolation {
            element: element.clone(),
            location: Point3D::new(x, y, 0),
            axis: "X (negative)".into(),
            actual_nm: x,
            limit_nm: 0,
        });
    } else if x > width_nm {
        violations.push(DrcViolation::DieBoundaryViolation {
            element: element.clone(),
            location: Point3D::new(x, y, 0),
            axis: "X".into(),
            actual_nm: x,
            limit_nm: width_nm,
        });
    }

    if y < 0 {
        violations.push(DrcViolation::DieBoundaryViolation {
            element: element.clone(),
            location: Point3D::new(x, y, 0),
            axis: "Y (negative)".into(),
            actual_nm: y,
            limit_nm: 0,
        });
    } else if y > height_nm {
        violations.push(DrcViolation::DieBoundaryViolation {
            element: element.clone(),
            location: Point3D::new(x, y, 0),
            axis: "Y".into(),
            actual_nm: y,
            limit_nm: height_nm,
        });
    }
}
