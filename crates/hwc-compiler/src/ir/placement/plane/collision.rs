//! Collision and interpenetration checks during plane placement.

use super::super::super::errors::IrError;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::material::MaterialId;
use hwc_engine::netlist::NetId;
use hwc_engine::space::HardwareSpace;
use hwc_parser::PlanePlacement;

/// Validate the placed plane against the substrate and previously placed pours.
///
/// Returns `Err` on hard interpenetration: a non-conductor overlapping the
/// substrate, or an overlapping pour of a different material at the same Z.
///
/// When a conductor overlaps an insulating/semiconducting substrate, the
/// substrate is auto-carved instead of erroring.
pub fn check_plane_collisions(
    space: &mut HardwareSpace,
    plane: &PlanePlacement,
    bbox: BoundingBox,
    material_id: MaterialId,
    z_start_nm: i64,
) -> Result<(), IrError> {
    check_substrate_interpenetration(space, plane, bbox, material_id)?;
    check_pour_interpenetration(space, plane, bbox, z_start_nm)
}

/// Check the plane against the base substrate, auto-carving where legal.
fn check_substrate_interpenetration(
    space: &mut HardwareSpace,
    plane: &PlanePlacement,
    bbox: BoundingBox,
    material_id: MaterialId,
) -> Result<(), IrError> {
    let Some(substrate_bbox) = &space.substrate_bbox else {
        return Ok(());
    };

    if !bbox.intersects(substrate_bbox) || space.substrate_material_id == material_id {
        return Ok(());
    }

    let is_conductive = space.material_registry.is_conductive(material_id);
    let is_substrate_dielectric = matches!(
        space
            .material_registry
            .get_category(space.substrate_material_id),
        Some(
            hwc_parser::MaterialCategory::Insulator | hwc_parser::MaterialCategory::Semiconductor
        )
    );

    if is_conductive && is_substrate_dielectric {
        let plane_net_id = plane
            .net
            .as_ref()
            .and_then(|net_name| space.netlist.get_net_by_name(net_name.base.as_str()))
            .unwrap_or(NetId::new(0));

        space.entity_graph.drill_hole(bbox, None, plane_net_id);
        println!(
            "   ├─ Auto-carved substrate for plane '{}' ({})",
            plane.name, plane.material
        );
        return Ok(());
    }

    let substrate_material_name = space
        .material_registry
        .get_name(space.substrate_material_id)
        .unwrap_or("Unknown");

    Err(IrError::PlacementConstraint {
        message: format!(
            "Substrate interpenetration detected: Plane '{}' ({}) overlaps with the base substrate ({}). \
             Use the same material as the substrate, or place the plane outside the substrate bounds.",
            plane.name, plane.material, substrate_material_name
        ),
        component: plane.name.to_string().into(),
    })
}

/// Check the plane against previously placed pours for Z-overlapping
/// different-material collisions.
fn check_pour_interpenetration(
    space: &HardwareSpace,
    plane: &PlanePlacement,
    bbox: BoundingBox,
    z_start_nm: i64,
) -> Result<(), IrError> {
    for existing in &space.pours {
        let Some(existing_bbox) = &existing.bbox else {
            continue;
        };

        if !bbox.intersects(existing_bbox) {
            continue;
        }

        let z_overlap = bbox.max.z > existing_bbox.min.z && existing_bbox.max.z > bbox.min.z;
        if z_overlap && existing.material_name != plane.material {
            return Err(IrError::MaterialInterpenetration {
                pour_a: existing.name.clone(),
                mat_a: existing.material_name.clone(),
                pour_b: plane.name.to_string(),
                mat_b: plane.material.clone(),
                z_nm: z_start_nm,
            });
        }
    }

    Ok(())
}
