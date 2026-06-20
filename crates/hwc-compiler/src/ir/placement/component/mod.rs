pub mod coordinates;
pub mod mounting;
pub mod unrolling;
pub mod validation;

use super::super::conversions::{coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::context::PlacementContext;
use super::module::place_module_instance;
use hwc_engine::{ComponentPlacer, HardwareSpace, PlacementParams};

pub fn place_component(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    layouts: &[hwc_parser::ModuleLayoutBlock],
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    if let Some(array_config) = &component.array_config {
        return super::array::place_component_array(
            space,
            component,
            array_config,
            layouts,
            bbox_tracker,
            ctx,
        );
    }

    if ctx
        .symbol_table
        .has_module(component.component_type.as_str())
    {
        return place_module_instance(space, component, layouts, bbox_tracker, ctx);
    }

    let resolved_position =
        coordinates::resolve_position(component, bbox_tracker, ctx.eval_context)?;

    let coord_ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: Some(bbox_tracker),
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };
    let mut position = coordinate_to_point(&resolved_position, &coord_ctx);

    let mut mounting_res = mounting::resolve_mounting_and_elevation(
        space,
        component,
        ctx.symbol_table,
        ctx.stackup_manager,
        position,
        ctx.origin,
    )?;
    position = mounting_res.position;

    if component.waivers.snap_to_surface {
        mounting::handle_snap_to_surface(space, &mut position);
        mounting_res.position = position;
    }

    let mut untransformed_origin =
        coordinates::calculate_untransformed_origin(&resolved_position, space, ctx, bbox_tracker)?;

    if component.elevation.is_some() || component.waivers.snap_to_surface {
        untransformed_origin.z = position.z;
    }

    let rotation_deg = component.rotation.as_ref().map(|r| r.angle).unwrap_or(0.0);
    let z_val = untransformed_origin.z / space.voxel_size.z_nm.max(1);
    let name = component
        .name
        .as_ref()
        .map(|n| n.to_string())
        .unwrap_or_else(|| format!("{}_{}", component.component_type, z_val).into());

    let engine_position =
        hwc_engine::geometry::Point3D::new(position.x, position.y, mounting_res.body_min_z);

    let placer = ComponentPlacer::new();
    placer
        .place_component(PlacementParams {
            entity_graph: &mut space.entity_graph,
            voxel_size: &space.voxel_size,
            arena: &mut space.netlist,
            symbol_table: ctx.symbol_table,
            material_registry: &mut space.material_registry,
            name: name.clone(),
            component_type: component.component_type.to_string().into(),
            position: engine_position,
            rotation_deg,
            merge_waiver: component.waivers.merge.clone(),
            collector: Some(&crate::DiagnosticReporterAdapter(ctx.collector)),
        })
        .map_err(|e| IrError::PlacementError(e.to_string()))?;

    unrolling::unroll_internal_features(
        space,
        &super::context::ComponentPlacementData {
            component,
            name: name.to_string(),
            position,
            rotation_deg,
            mount_side: mounting_res.mount_side,
        },
        bbox_tracker,
        ctx,
    )?;

    validation::validate_and_register(
        space,
        &super::context::ComponentPlacementData {
            component,
            name: name.to_string(),
            position,
            rotation_deg,
            mount_side: mounting_res.mount_side,
        },
        &super::context::ValidationParams {
            untransformed_origin,
            position,
            rotation_deg,
            body_min_z: mounting_res.body_min_z,
            body_max_z: mounting_res.body_max_z,
        },
        bbox_tracker,
        ctx,
    )?;

    Ok(())
}
