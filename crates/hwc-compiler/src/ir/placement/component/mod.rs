//! Component placement functionality.

pub mod coordinates;
pub mod mounting;
pub mod unrolling;
pub mod validation;

use super::super::conversions::{coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::module::place_module_instance;
use crate::SymbolTable;
use hwc_engine::{ComponentPlacer, HardwareSpace, PlacementParams};

/// Place a component in the voxel grid.
pub fn place_component(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    origin: hwc_parser::OriginPoint,
    symbol_table: &SymbolTable,
    layouts: &[hwc_parser::ModuleLayoutBlock],
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    eval_context: &hwc_parser::EvaluationContext,
    collector: &hwc_diagnostics::DiagnosticCollector,
    stackup_manager: &super::super::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    // Sprint 3, Task 3.2: Check if this is an array placement
    if let Some(array_config) = &component.array_config {
        let mut array_ctx = super::array::ArrayPlacementContext {
            origin,
            symbol_table,
            layouts,
            bbox_tracker,
            eval_context,
            collector,
            stackup_manager,
            profile,
        };
        return super::array::place_component_array(space, component, array_config, &mut array_ctx);
    }

    // Check if this is a module instantiation
    if symbol_table.has_module(component.component_type.as_str()) {
        // This is a module - we need to flatten it
        return place_module_instance(
            space,
            component,
            origin,
            symbol_table,
            layouts,
            bbox_tracker,
            eval_context,
            collector,
            stackup_manager,
            profile,
        );
    }

    // Sprint 3, Task 3.1: Resolve relative coordinates to absolute
    let resolved_position = coordinates::resolve_position(component, bbox_tracker, eval_context)?;

    // Regular component placement
    let ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin,
        space_dimensions: &space.dimensions,
        symbol_table,
        eval_context,
        bbox_tracker: Some(bbox_tracker),
        stackup_manager,
        profile,
    };
    let mut position = coordinate_to_point(&resolved_position, &ctx);

    // v0.1.7: Component Mounting Abstraction
    let mut mounting_res = mounting::resolve_mounting_and_elevation(
        space,
        component,
        symbol_table,
        stackup_manager,
        position,
        origin,
    )?;
    position = mounting_res.position;

    // v0.1.7: Implement snap_to_surface
    if component.waivers.snap_to_surface {
        mounting::handle_snap_to_surface(space, &mut position);
        mounting_res.position = position;
    }

    // v0.1.7 CRITICAL: Calculate untransformed origin
    let mut untransformed_origin = coordinates::calculate_untransformed_origin(
        &resolved_position,
        space,
        symbol_table,
        bbox_tracker,
        eval_context,
        stackup_manager,
        origin,
        profile,
    )?;

    // v0.1.7 FIX: If elevation is provided, update untransformed_origin.z
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

    // Engine placement
    let engine_position = hwc_engine::geometry::Point3D::new(position.x, position.y, mounting_res.body_min_z);

    let placer = ComponentPlacer::new();
    placer
        .place_component(PlacementParams {
            grid: &mut space.voxel_grid,
            voxel_size: &space.voxel_size,
            arena: &mut space.netlist,
            symbol_table,
            material_registry: &mut space.material_registry,
            name: name.clone(),
            component_type: component.component_type.to_string().into(),
            position: engine_position,
            rotation_deg,
            merge_waiver: component.waivers.merge.clone(),
            collector: Some(&crate::DiagnosticReporterAdapter(collector)),
        })
        .map_err(|e| IrError::PlacementError(e.to_string()))?;

    // Sprint 2.2: Unroll internal features
    unrolling::unroll_internal_features(
        space,
        component,
        &name,
        position,
        rotation_deg,
        mounting_res.mount_side,
        origin,
        symbol_table,
        bbox_tracker,
        eval_context,
        collector,
        profile,
        stackup_manager,
    )?;

    // Sprint 3, Task 3.1: Validation and Bounding Box Registration
    validation::validate_and_register(
        space,
        component,
        &name,
        untransformed_origin,
        position,
        rotation_deg,
        mounting_res.body_min_z,
        mounting_res.body_max_z,
        origin,
        symbol_table,
        bbox_tracker,
        collector,
    )?;

    Ok(())
}
