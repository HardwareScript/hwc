use super::super::conversions::{spanning_coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::context::PlacementContext;
use hwc_engine::HardwareSpace;

pub fn place_substrate(
    space: &mut HardwareSpace,
    substrate: &hwc_parser::SubstratePlacement,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let material_id = space
        .material_registry
        .get_id(&substrate.material)
        .ok_or_else(|| IrError::UndeclaredMaterial {
            material: substrate.material.clone(),
        })?;

    space.substrate_material_id = material_id;

    let coord_ctx = CoordinateContext {
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: None,
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };
    let start = spanning_coordinate_to_point(&substrate.from, &coord_ctx, false).map_err(|e| {
        IrError::CoordinateResolutionFailed {
            coordinate_str: "substrate from".into(),
            reason: e,
        }
    })?;
    let end = spanning_coordinate_to_point(&substrate.to, &coord_ctx, true).map_err(|e| {
        IrError::CoordinateResolutionFailed {
            coordinate_str: "substrate to".into(),
            reason: e,
        }
    })?;

    let mut cutout_bboxes = Vec::new();
    for cutout in &substrate.cutouts {
        let cutout_start =
            spanning_coordinate_to_point(&cutout.from, &coord_ctx, false).map_err(|e| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: "cutout from".into(),
                    reason: e,
                }
            })?;
        let cutout_end =
            spanning_coordinate_to_point(&cutout.to, &coord_ctx, true).map_err(|e| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: "cutout to".into(),
                    reason: e,
                }
            })?;
        let cutout_bbox = hwc_engine::geometry::BoundingBox::new(cutout_start, cutout_end);
        cutout_bboxes.push(cutout_bbox);
    }

    let physical_substrate_bbox = hwc_engine::geometry::BoundingBox::new(start, end);
    space.substrate_bbox = Some(physical_substrate_bbox);

    let z_ctx = CoordinateContext {
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: None,
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };
    let user_start = hwc_engine::geometry::Point3D::new(
        crate::ir::conversions::evaluate_expression_to_nm(
            substrate.from.x(),
            ctx.symbol_table,
            ctx.eval_context,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: "substrate from X".into(),
            reason: e.to_string(),
        })?,
        crate::ir::conversions::evaluate_expression_to_nm(
            substrate.from.y(),
            ctx.symbol_table,
            ctx.eval_context,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: "substrate from Y".into(),
            reason: e.to_string(),
        })?,
        crate::ir::conversions::resolve_coordinate_z_nm(substrate.from.z(), &z_ctx, false)
            .map_err(|e| IrError::CoordinateResolutionFailed {
                coordinate_str: "substrate from Z".into(),
                reason: e.to_string(),
            })?,
    );
    let user_end_z =
        crate::ir::conversions::spanning_coordinate_to_point(&substrate.to, &z_ctx, true)
            .map_err(|e| IrError::CoordinateResolutionFailed {
                coordinate_str: "substrate Z end".into(),
                reason: e,
            })?
            .z;
    let user_end = hwc_engine::geometry::Point3D::new(
        crate::ir::conversions::evaluate_expression_to_nm(
            substrate.to.x(),
            ctx.symbol_table,
            ctx.eval_context,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: "substrate to X".into(),
            reason: e.to_string(),
        })?,
        crate::ir::conversions::evaluate_expression_to_nm(
            substrate.to.y(),
            ctx.symbol_table,
            ctx.eval_context,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: "substrate to Y".into(),
            reason: e.to_string(),
        })?,
        user_end_z,
    );
    let user_substrate_bbox = hwc_engine::geometry::BoundingBox::new(user_start, user_end);
    bbox_tracker.register("substrate".into(), user_substrate_bbox, user_start);

    // Add substrate to entity graph (v0.1.8 replacement for ComponentPlacer)
    space.entity_graph.add_substrate_layer(
        material_id,
        hwc_engine::NetId::UNCONNECTED, // Substrate is typically net 0
        physical_substrate_bbox,
        hwc_engine::geometry_router::substrate_types::SubstrateLayerType::Substrate,
    );

    for cutout_bbox in cutout_bboxes {
        space.drill_hole(cutout_bbox, None, hwc_engine::netlist::NetId::new(0));
    }

    Ok(())
}
