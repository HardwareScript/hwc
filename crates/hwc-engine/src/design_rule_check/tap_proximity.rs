//! Substrate Tap Proximity DRC: Validates latch-up prevention rules (SKY130 latchup.1).
//!
//! # Physical Verification Contract
//!
//! In CMOS semiconductor layouts, active device channels and diffusion regions must have a low-resistance
//! substrate/well tap connected to the bulk potential within a maximum distance declared
//! by the PDK profile (`clearance.max_substrate_tap_distance`).
//!
//! # Zero-Magic Architecture
//!
//! - Lookups are strictly table-driven and strongly typed via `MaterialCategory::Semiconductor`.
//! - Substrate tap potential is resolved strictly from the PDK profile's declared `substrate_net`.
//! - No hardcoded terminal strings ("S", "D", "G", "B"), no heuristic substring checks, and no magic defaults.

use super::types::DrcViolation;
use crate::constraint_manager::ConstraintRulebook;
use crate::geometry::Point3D;
use crate::material::MaterialCategory;
use crate::space::HardwareSpace;
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Validate substrate tap proximity for latch-up prevention.
pub fn validate_tap_proximity(
    space: &HardwareSpace,
    constraints: &ConstraintRulebook,
) -> Result<Vec<DrcViolation>, String> {
    let mut violations = Vec::new();

    let fab = match constraints.fabrication.as_ref() {
        Some(f) => f,
        None => return Ok(violations),
    };

    // 1. Strict PDK lookup: No fallback defaults. If max_substrate_tap_distance_nm is not declared,
    // this process does not enforce a tap proximity constraint.
    let max_tap_distance_nm = match fab.max_substrate_tap_distance_nm {
        Some(d) => d,
        None => return Ok(violations),
    };

    // 2. Resolve substrate net: lookup from PDK profile's declared substrate_net
    let substrate_net_name = match &fab.substrate_net {
        Some(net) => net.as_str(),
        None => {
            return Err(
                "DRC Error: Profile declared 'max_substrate_tap_distance' but is missing REQUIRED 'substrate_net:' definition."
                    .to_string(),
            );
        }
    };

    let substrate_net_id = space.netlist.get_net_by_name(substrate_net_name);

    // 3. Strongly typed classification of pours using MaterialCategory from MaterialRegistry
    let mut tap_pours = Vec::new();
    let mut active_device_diffusions: FxHashMap<CompactString, Vec<&crate::space::PourMetadata>> =
        FxHashMap::default();

    for pour in &space.pours {
        let bbox = match pour.bbox {
            Some(b) => b,
            None => continue,
        };

        let material_id = match space.material_registry.get_id(&pour.material_name) {
            Some(id) => id,
            None => continue,
        };

        let category = match space.material_registry.get_category(material_id) {
            Some(c) => c,
            None => continue,
        };

        // Only semiconductor materials participate in diffusion channel / substrate tap validation
        if category != MaterialCategory::Semiconductor {
            continue;
        }

        // Check if this semiconductor pour is connected to the substrate net
        let is_substrate_tap = if let Some(sub_id) = substrate_net_id {
            pour.net
                .as_ref()
                .and_then(|net_name| space.netlist.get_net_by_name(net_name.as_str()))
                .map(|id| id == sub_id)
                .unwrap_or(false)
        } else {
            pour.net
                .as_ref()
                .map(|net_name| net_name.as_str() == substrate_net_name)
                .unwrap_or(false)
        };

        if is_substrate_tap {
            tap_pours.push((pour, bbox));
        } else if let Some(ref binding) = pour.device_binding {
            // Device-bound semiconductor diffusion that is not a bulk tap is an active channel/terminal
            active_device_diffusions
                .entry(binding.device_name.clone())
                .or_default()
                .push(pour);
        }
    }

    if active_device_diffusions.is_empty() {
        return Ok(violations);
    }

    // 4. Validate Euclidean distance to nearest substrate tap
    for (device_name, diff_pours) in active_device_diffusions {
        for diff_pour in diff_pours {
            let diff_bbox = match diff_pour.bbox {
                Some(b) => b,
                None => continue,
            };

            let mut min_dist_nm = i64::MAX;

            for (_tap_pour, tap_bbox) in &tap_pours {
                let dist = diff_bbox.distance_to(tap_bbox);
                if dist < min_dist_nm {
                    min_dist_nm = dist;
                }
            }

            let center_x = (diff_bbox.min.x + diff_bbox.max.x) / 2;
            let center_y = (diff_bbox.min.y + diff_bbox.max.y) / 2;
            let center_z = (diff_bbox.min.z + diff_bbox.max.z) / 2;

            if tap_pours.is_empty() || min_dist_nm > max_tap_distance_nm {
                violations.push(DrcViolation::LatchUpTapTooDistant {
                    device: device_name.clone(),
                    actual_nm: if tap_pours.is_empty() { i64::MAX } else { min_dist_nm },
                    max_allowed_nm: max_tap_distance_nm,
                    location: Point3D::new(center_x, center_y, center_z),
                });
                break; // One violation per device is sufficient
            }
        }
    }

    Ok(violations)
}
