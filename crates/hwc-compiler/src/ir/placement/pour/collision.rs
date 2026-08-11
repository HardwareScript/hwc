//! Collision and interpenetration checks during pour placement.

use super::super::super::errors::IrError;
use hwc_diagnostics::DiagnosticCollector;
use hwc_diagnostics::WaiverApplied;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::space::HardwareSpace;
use hwc_parser::PourPlacement;

/// Validate the placed pour against the substrate and previously placed pours.
///
/// Returns `Err` on hard interpenetration (different conductor/substrate
/// materials, or unwaived overlapping pours of different materials).
pub fn check_pour_collisions(
    space: &mut HardwareSpace,
    pour: &PourPlacement,
    bbox: BoundingBox,
    z_start_nm: i64,
    collector: &DiagnosticCollector,
) -> Result<(), IrError> {
    let material_id = space
        .material_registry
        .get_id(&pour.material)
        .unwrap_or_default();
    let skip_substrate_check = pour.waivers.merge == hwc_parser::MergeWaiver::All;

    if let Some(substrate_bbox) = &space.substrate_bbox {
        if bbox.intersects(substrate_bbox)
            && !skip_substrate_check
            && space.substrate_material_id != material_id
        {
            let is_conductive = space.material_registry.is_conductive(material_id);
            let is_substrate_dielectric = matches!(
                space.material_registry.get_category(space.substrate_material_id),
                Some(hwc_parser::MaterialCategory::Insulator | hwc_parser::MaterialCategory::Semiconductor)
            );

            if is_conductive && is_substrate_dielectric {
                let pour_net_id = if let Some(net_name) = &pour.net {
                    space
                        .netlist
                        .get_net_by_name(net_name.base.as_str())
                        .unwrap_or(hwc_engine::netlist::NetId::new(0))
                } else {
                    hwc_engine::netlist::NetId::new(0)
                };
                space.entity_graph.drill_hole(bbox, None, pour_net_id);
                println!(
                    "   ├─ Auto-carved substrate for pour '{}' ({})",
                    pour.name, pour.material
                );
            } else {
                let substrate_material_name = space
                    .material_registry
                    .get_name(space.substrate_material_id)
                    .unwrap_or("Unknown");

                return Err(IrError::PlacementConstraint {
                    message: format!(
                        "Substrate interpenetration detected: Pour '{}' ({}) overlaps with the base substrate ({}). \
                         Use the same material as the substrate, or place the pour outside the substrate bounds.",
                        pour.name,
                        pour.material,
                        substrate_material_name
                    ),
                    component: pour.name.to_string().into(),
                });
            }
        }
    }

    for existing in &space.pours {
        if let Some(existing_bbox) = &existing.bbox {
            if bbox.intersects(existing_bbox) {
                let z_overlap =
                    bbox.max.z > existing_bbox.min.z && existing_bbox.max.z > bbox.min.z;
                if z_overlap {
                    let is_waived = pour.waivers.merge == hwc_parser::MergeWaiver::All;

                    if existing.material_name != pour.material {
                        if is_waived {
                            collector.report(WaiverApplied::new(&format!(
                                "Pour '{}' (mat: {}) allowed to overlap '{}' (mat: {})",
                                pour.name, pour.material, existing.name, existing.material_name
                            )));
                        } else {
                            return Err(IrError::MaterialInterpenetration {
                                pour_a: existing.name.clone(),
                                mat_a: existing.material_name.clone(),
                                pour_b: pour.name.to_string(),
                                mat_b: pour.material.clone(),
                                z_nm: z_start_nm,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
