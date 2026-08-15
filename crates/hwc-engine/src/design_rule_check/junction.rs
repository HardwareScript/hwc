//! Junction Breakdown Validation (P46)
//!
//! Enforces semiconductor p-n junction breakdown ratings.
//! Validates local potential differences across spatially overlapping or adjacent
//! semiconductor regions (diffusions, wells, and substrate layers).
//!
//! **Physical Phenomenon:**
//! - Avalanche breakdown when reverse bias exceeds V_BR
//! - Permanent junction damage and latch-up risk
//!
//! **Architecture:**
//! - Uses typed MaterialCategory::Semiconductor (no string matching)
//! - Fails loudly if net voltages or breakdown_voltage properties are missing
//! - Evaluates 2D spatial containment (diffusion-in-well, well-in-substrate)

use crate::geometry::Point3D;
use crate::material::{MaterialCategory, MaterialRegistry};
use crate::space::{HardwareSpace, NetClassification};
use compact_str::CompactString;

use super::types::DrcViolation;

/// Validate junction breakdown constraints for all semiconductor regions.
///
/// Uses typed MaterialCategory queries and fails loudly on missing declarations.
///
/// # Algorithm
/// 1. Filter semiconductor pours using MaterialCategory::Semiconductor (zero string magic)
/// 2. For each semiconductor, check spatial containment in other semiconductors (2D bbox)
/// 3. If contained, evaluate junction voltage across contained→container interface
/// 4. If not contained, evaluate junction voltage to stackup substrate layer
/// 5. Fail loudly if net voltage or breakdown_voltage property is missing
///
/// # Parameters
/// * `space` - Complete hardware space with pours, nets, and stackup
/// * `material_registry` - Material properties with typed category queries
///
/// # Returns
/// * `Ok(Vec<DrcViolation>)` - Junction violations (empty if all pass)
/// * `Err(String)` - Fatal error (missing required declarations)
pub fn validate_junction_breakdown(
    space: &HardwareSpace,
    material_registry: &MaterialRegistry,
) -> Result<Vec<DrcViolation>, String> {
    let mut violations = Vec::new();

    // 1. Resolve ground reference potential via typed NetClassification::Ground (Zero Magic)
    let ground_context = resolve_ground_reference_context(space)?;

    // 2. Filter semiconductor pours using typed MaterialCategory (Zero Magic)
    let semiconductor_pours: Vec<_> = space
        .pours
        .iter()
        .filter(|p| {
            material_registry
                .get_category_by_name(&p.material_name)
                .map(|cat| cat == MaterialCategory::Semiconductor)
                .unwrap_or(false)
        })
        .collect();

    // If no semiconductor geometry, pass cleanly (PCB design)
    if semiconductor_pours.is_empty() {
        return Ok(violations);
    }

    eprintln!("[DRC JUNCTION] Found {} semiconductor pours", semiconductor_pours.len());

    // 3. Evaluate each semiconductor pour
    for pour in &semiconductor_pours {
        // Enforce explicit net declaration (Fail Loudly)
        let net_name = pour.net.as_ref().ok_or_else(|| {
            format!(
                "[DRC JUNCTION] FATAL: Semiconductor pour '{}' has no net assignment. \
                 All semiconductor regions must declare a net connection.",
                pour.name
            )
        })?;

        // Enforce explicit net voltage (No default to 0V - Fail Loudly)
        let applied_voltage = space
            .net_electrical_properties
            .get(net_name)
            .and_then(|props| props.potential_v)
            .ok_or_else(|| {
                format!(
                    "[DRC JUNCTION] FATAL: Net '{}' on semiconductor pour '{}' is missing required 'potential' declaration. \
                     Add 'potential: <value>V' to your nets: section.",
                    net_name, pour.name
                )
            })?;

        // Query breakdown voltage using standard property name
        let props = material_registry
            .get_physical_props_by_name(&pour.material_name)
            .ok_or_else(|| {
                format!(
                    "[DRC JUNCTION] FATAL: Material '{}' not found in registry.",
                    pour.material_name
                )
            })?;

        let max_voltage = props
            .get("max_junction_voltage")
            .or_else(|| props.get("breakdown_voltage"))
            .ok_or_else(|| {
                format!(
                    "[DRC JUNCTION] FATAL: Semiconductor material '{}' is missing required 'max_junction_voltage' property. \
                     Add 'max_junction_voltage: <value>V' to your material definition.",
                    pour.material_name
                )
            })?;

        let pour_bbox = match &pour.bbox {
            Some(b) => b,
            None => continue,
        };

        eprintln!(
            "[DRC JUNCTION] Checking pour '{}': material={}, net={}, voltage={:.2}V, max={:.2}V",
            pour.name, pour.material_name, net_name, applied_voltage, max_voltage
        );

        // 4. Check spatial containment: is this pour inside another semiconductor? (e.g., diffusion in well)
        let mut parent_found = false;
        for other in &semiconductor_pours {
            if std::ptr::eq(*pour, *other) {
                continue;
            }

            if let Some(other_bbox) = &other.bbox {
                // Check 2D containment: is 'pour' fully inside 'other'?
                // (diffusion inside N-Well, or N-Well inside P-Substrate)
                let contained_x = pour_bbox.min.x >= other_bbox.min.x
                    && pour_bbox.max.x <= other_bbox.max.x;
                let contained_y = pour_bbox.min.y >= other_bbox.min.y
                    && pour_bbox.max.y <= other_bbox.max.y;

                if contained_x && contained_y {
                    parent_found = true;

                    // Get parent net voltage (Fail Loudly)
                    let other_net = other.net.as_ref().ok_or_else(|| {
                        format!(
                            "[DRC JUNCTION] FATAL: Containing well '{}' has no net assignment.",
                            other.name
                        )
                    })?;

                    let other_voltage = space
                        .net_electrical_properties
                        .get(other_net)
                        .and_then(|props| props.potential_v)
                        .ok_or_else(|| {
                            format!(
                                "[DRC JUNCTION] FATAL: Containing net '{}' is missing 'potential' declaration.",
                                other_net
                            )
                        })?;

                    // Calculate junction voltage
                    let delta_v = (applied_voltage - other_voltage).abs();

                    eprintln!(
                        "[DRC JUNCTION]   Contained in '{}': |{:.2}V - {:.2}V| = {:.2}V",
                        other.name, applied_voltage, other_voltage, delta_v
                    );

                    if delta_v > max_voltage {
                        let center = Point3D::new(
                            (pour_bbox.min.x + pour_bbox.max.x) / 2,
                            (pour_bbox.min.y + pour_bbox.max.y) / 2,
                            pour.z_bottom_nm,
                        );

                        eprintln!(
                            "[DRC JUNCTION]   • VIOLATION: {:.2}V exceeds {:.2}V limit",
                            delta_v, max_voltage
                        );

                        violations.push(DrcViolation::JunctionBreakdownViolation {
                            net: net_name.clone(),
                            material: pour.material_name.clone(),
                            substrate_material: other.material_name.clone(),
                            applied_voltage_v: delta_v,
                            max_voltage_v: max_voltage,
                            location: center,
                        });
                    }
                    break;
                }
            }
        }

        // 5. If not enclosed by another semiconductor, evaluate against declared Ground reference
        if !parent_found {
            if let Some((ground_net_name, ground_voltage)) = &ground_context {
                let delta_v = (applied_voltage - ground_voltage).abs();

                eprintln!(
                    "[DRC JUNCTION]   To ground net '{}': |{:.2}V - {:.2}V| = {:.2}V",
                    ground_net_name, applied_voltage, ground_voltage, delta_v
                );

                if delta_v > max_voltage {
                    let center = Point3D::new(
                        (pour_bbox.min.x + pour_bbox.max.x) / 2,
                        (pour_bbox.min.y + pour_bbox.max.y) / 2,
                        pour.z_bottom_nm,
                    );

                    eprintln!(
                        "[DRC JUNCTION]   • VIOLATION: {:.2}V exceeds {:.2}V limit",
                        delta_v, max_voltage
                    );

                    violations.push(DrcViolation::JunctionBreakdownViolation {
                        net: net_name.clone(),
                        material: pour.material_name.clone(),
                        substrate_material: ground_net_name.clone(),
                        applied_voltage_v: delta_v,
                        max_voltage_v: max_voltage,
                        location: center,
                    });
                }
            } else {
                // No ground net declared - this semiconductor is floating (error condition)
                return Err(format!(
                    "[DRC JUNCTION] FATAL: Semiconductor pour '{}' is not spatially enclosed by another semiconductor, \
                     and no net with 'classification: ground' was declared to establish substrate reference potential.",
                    pour.name
                ));
            }
        }
    }

    eprintln!(
        "[DRC JUNCTION] Completed with {} violation(s)",
        violations.len()
    );
    Ok(violations)
}

/// Resolve the ground reference potential using typed NetClassification.
///
/// **Zero String Magic:** Queries classification == NetClassification::Ground,
/// works for any net name (GND, VSS, AGND, RET, 0, etc.)
///
/// **Fail Loudly:** Returns error if ground net exists but has no potential declared.
fn resolve_ground_reference_context(
    space: &HardwareSpace,
) -> Result<Option<(CompactString, f64)>, String> {
    // Query nets by typed NetClassification::Ground (Zero Magic)
    let ground_net = space
        .net_classifications
        .iter()
        .find(|(_, classification)| **classification == NetClassification::Ground);

    if let Some((net_name, _)) = ground_net {
        // Get the declared potential for this ground net (Fail Loudly if missing)
        let potential = space
            .net_electrical_properties
            .get(net_name)
            .and_then(|props| props.potential_v)
            .ok_or_else(|| {
                format!(
                    "[DRC JUNCTION] FATAL: Ground reference net '{}' is missing required 'potential' declaration. \
                     Add 'potential: <value>V' to your nets: section.",
                    net_name
                )
            })?;

        eprintln!(
            "[DRC JUNCTION] Ground reference: net='{}', voltage={:.2}V (via typed NetClassification::Ground)",
            net_name, potential
        );

        Ok(Some((net_name.clone(), potential)))
    } else {
        // No ground net declared - this is acceptable for some designs (e.g., PCB with no substrate)
        eprintln!("[DRC JUNCTION] No ground reference net found (no net with classification: ground)");
        Ok(None)
    }
}

