use super::super::conversions::{spanning_coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::context::PlacementContext;
use hwc_engine::{ComponentPlacer, HardwareSpace};

pub fn place_substrate(
    space: &mut HardwareSpace,
    substrate: &hwc_parser::SubstratePlacement,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let material_id = space.material_registry.get_id(&substrate.material).ok_or_else(|| {
        IrError::UndeclaredMaterial { material: substrate.material.clone() }
    })?;

    space.substrate_material_id = material_id;

    let coord_ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: None,
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };
    let start = spanning_coordinate_to_point(&substrate.from, &coord_ctx, false)
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: "substrate from".into(),
            reason: e,
        })?;
    let end = spanning_coordinate_to_point(&substrate.to, &coord_ctx, true)
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: "substrate to".into(),
            reason: e,
        })?;

    let mut cutout_bboxes = Vec::new();
    for cutout in &substrate.cutouts {
        let cutout_start = spanning_coordinate_to_point(&cutout.from, &coord_ctx, false)
            .map_err(|e| IrError::CoordinateResolutionFailed {
                coordinate_str: "cutout from".into(),
                reason: e,
            })?;
        let cutout_end = spanning_coordinate_to_point(&cutout.to, &coord_ctx, true)
            .map_err(|e| IrError::CoordinateResolutionFailed {
                coordinate_str: "cutout to".into(),
                reason: e,
            })?;
        let cutout_bbox = hwc_engine::geometry::BoundingBox::new(cutout_start, cutout_end);
        cutout_bboxes.push(cutout_bbox);
    }

    let physical_substrate_bbox = hwc_engine::geometry::BoundingBox::new(start, end);
    space.substrate_bbox = Some(physical_substrate_bbox);

    let z_ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: None,
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };
    let user_start = hwc_engine::geometry::Point3D::new(
        crate::ir::conversions::evaluate_expression_to_nm(substrate.from.x(), ctx.symbol_table)
            .unwrap_or(0),
        crate::ir::conversions::evaluate_expression_to_nm(substrate.from.y(), ctx.symbol_table)
            .unwrap_or(0),
        crate::ir::conversions::resolve_coordinate_z_nm(substrate.from.z(), &z_ctx, false)
            .unwrap_or(0),
    );
    let user_end_z =
        crate::ir::conversions::spanning_coordinate_to_point(&substrate.to, &z_ctx, true)
            .map_err(|e| IrError::CoordinateResolutionFailed {
                coordinate_str: "substrate Z end".into(),
                reason: e,
            })?
            .z;
    let user_end = hwc_engine::geometry::Point3D::new(
        crate::ir::conversions::evaluate_expression_to_nm(substrate.to.x(), ctx.symbol_table)
            .unwrap_or(0),
        crate::ir::conversions::evaluate_expression_to_nm(substrate.to.y(), ctx.symbol_table)
            .unwrap_or(0),
        user_end_z,
    );
    let user_substrate_bbox = hwc_engine::geometry::BoundingBox::new(user_start, user_end);
    bbox_tracker.register("substrate".into(), user_substrate_bbox, user_start);

    let placer = ComponentPlacer::new();

    if !cutout_bboxes.is_empty() {
        placer
            .place_substrate(
                &mut space.entity_graph,
                &space.voxel_size,
                material_id,
                start,
                end,
                0,
            )
            .map_err(|e| IrError::PlacementConstraint {
                message: format!("Failed to place substrate: {}", e),
                component: "substrate".into(),
            })?;

        for cutout_bbox in cutout_bboxes {
            placer
                .place_substrate(
                    &mut space.entity_graph,
                    &space.voxel_size,
                    0,
                    cutout_bbox.min,
                    cutout_bbox.max,
                    0,
                )
                .map_err(|e| IrError::PlacementConstraint {
                    message: format!("Failed to place substrate cutout: {}", e),
                    component: "substrate".into(),
                })?;
        }
    } else {
        placer
            .place_substrate(
                &mut space.entity_graph,
                &space.voxel_size,
                material_id,
                start,
                end,
                0,
            )
            .map_err(|e| IrError::PlacementConstraint {
                message: format!("Failed to place substrate: {}", e),
                component: "substrate".into(),
            })?;
    }

    Ok(())
}
