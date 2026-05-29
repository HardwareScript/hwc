//! Substrate placement functionality.

use super::super::conversions::{spanning_coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use hwc_engine::{ComponentPlacer, HardwareSpace};

/// Place substrate in the voxel grid.
pub fn place_substrate(
    space: &mut HardwareSpace,
    substrate: &hwc_parser::SubstratePlacement,
    origin: hwc_parser::OriginPoint,
    symbol_table: &crate::SymbolTable,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    eval_context: &hwc_parser::EvaluationContext,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
) -> Result<(), IrError> {
    // Get or register the substrate material in the material registry
    let material_id = space.material_registry.get_or_register(&substrate.material);

    space.substrate_material_id = material_id;

    // Use spanning_coordinate_to_point for substrate (no origin transformation)
    let ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin,
        space_dimensions: &space.dimensions,
        symbol_table,
        eval_context,
        bbox_tracker: None, // substrate doesn't use anchor references
        stackup_manager,
    };
    let start = spanning_coordinate_to_point(&substrate.from, &ctx, false)
        .map_err(|e| IrError::PlacementError(e))?;
    let end = spanning_coordinate_to_point(&substrate.to, &ctx, true)
        .map_err(|e| IrError::PlacementError(e))?;

    // Resolve cutouts (v0.1.7 Phase 2.2)
    let mut cutout_bboxes = Vec::new();
    for cutout in &substrate.cutouts {
        let cutout_start = spanning_coordinate_to_point(&cutout.from, &ctx, false)
            .map_err(|e| IrError::PlacementError(e))?;
        let cutout_end = spanning_coordinate_to_point(&cutout.to, &ctx, true)
            .map_err(|e| IrError::PlacementError(e))?;
        let cutout_bbox = hwc_engine::geometry::BoundingBox::new(cutout_start, cutout_end);
        cutout_bboxes.push(cutout_bbox);
    }

    // GAP2: Track substrate bounding box for component overlap validation
    let physical_substrate_bbox = hwc_engine::geometry::BoundingBox::new(start, end);
    space.substrate_bbox = Some(physical_substrate_bbox);

    // Sprint 6: Add substrate.min_z and substrate.max_z as anchors
    // Register the substrate in the bounding box tracker using USER-SPACE coordinates
    // This ensures consistency with component anchors (GAP1 FIX)
    let z_ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin,
        space_dimensions: &space.dimensions,
        symbol_table,
        eval_context,
        bbox_tracker: None,
        stackup_manager,
    };
    let user_start = hwc_engine::geometry::Point3D::new(
        crate::ir::conversions::evaluate_expression_to_nm(substrate.from.x(), symbol_table)
            .unwrap_or(0),
        crate::ir::conversions::evaluate_expression_to_nm(substrate.from.y(), symbol_table)
            .unwrap_or(0),
        crate::ir::conversions::resolve_coordinate_z_nm(substrate.from.z(), &z_ctx, false)
            .unwrap_or(0),
    );
    let user_end_z = crate::ir::conversions::spanning_coordinate_to_point(&substrate.to, &z_ctx, true)
        .map_err(|e| IrError::PlacementError(e))?
        .z;
    let user_end = hwc_engine::geometry::Point3D::new(
        crate::ir::conversions::evaluate_expression_to_nm(substrate.to.x(), symbol_table)
            .unwrap_or(0),
        crate::ir::conversions::evaluate_expression_to_nm(substrate.to.y(), symbol_table)
            .unwrap_or(0),
        user_end_z,
    );
    let user_substrate_bbox = hwc_engine::geometry::BoundingBox::new(user_start, user_end);
    bbox_tracker.register("substrate".into(), user_substrate_bbox, user_start);

    // println!($3"[DEBUG GAP2] Substrate bbox: min=({:.3}mm, {:.3}mm, {:.3}mm) max=({:.3}mm, {:.3}mm, {:.3}mm)",
    // start.x as f64 / 1_000_000.0,
    // start.y as f64 / 1_000_000.0,
    // start.z as f64 / 1_000_000.0,
    // end.x as f64 / 1_000_000.0,
    // end.y as f64 / 1_000_000.0,
    // end.z as f64 / 1_000_000.0,
    // );

    let placer = ComponentPlacer::new();

    if !cutout_bboxes.is_empty() {
        // First, place the main substrate
        placer
            .place_substrate(
                &mut space.voxel_grid,
                &space.voxel_size,
                material_id,
                start,
                end,
                0,
            )
            .map_err(|e| IrError::PlacementError(e.to_string()))?;

        // Then, carve out each cutout (holes) by placing void material
        for cutout_bbox in cutout_bboxes {
            placer
                .place_substrate(
                    &mut space.voxel_grid,
                    &space.voxel_size,
                    0, // Material 0 = void/empty (erases substrate)
                    cutout_bbox.min,
                    cutout_bbox.max,
                    0,
                )
                .map_err(|e| IrError::PlacementError(e.to_string()))?;
        }
    } else {
        placer
            .place_substrate(
                &mut space.voxel_grid,
                &space.voxel_size,
                material_id,
                start,
                end,
                0, // Base substrate has no net assignment
            )
            .map_err(|e| IrError::PlacementError(e.to_string()))?;
    }

    Ok(())
}
