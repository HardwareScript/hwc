//! Hierarchical space instantiation (v0.2.1)
//!
//! Implements the "Affine Flat-Blitter" pattern for hierarchical space composition.
//! It performs coordinate transformation and net remapping to flatten a pre-compiled
//! child space into the parent space's coordinate system. No defaults, no fallbacks:
//! all operations use proper lookup tables and explicit validation.
//!
//! ## Module layout
//!
//! - `transform`: Integer-based 2D/3D affine transformation (`FixedTransform2D`)
//! - `net_mapping`: Net ID remapping + child netlist flattening
//! - `substrate`: Substrate layers, routing segments, and vias
//! - `entity_registry`: Entity registry + PhysicalInterface (CIR) transfer
//! - `metadata`: Pours, contacts, keep-out zones, component bboxes
//! - `routing_db`: Layer-connection DB + analytic route registration

pub mod entity_registry;
pub mod metadata;
pub mod net_mapping;
pub mod routing_db;
pub mod substrate;
pub mod transform;

use crate::ir::errors::IrError;
use crate::SymbolTable;
use hwc_engine::HardwareSpace;
use hwc_parser::{EvaluationContext, SpaceDefinition, SpaceInstancePlacement};
use hwc_types::UnitRegistry;

use entity_registry::transform_entity_registry;
use metadata::{
    transform_component_bboxes, transform_contacts, transform_keep_out_zones, transform_pours,
};
use net_mapping::{build_net_id_map, transform_netlist};
use routing_db::{register_child_routes_in_database, transfer_layer_connections};
use substrate::{transform_routing_segments, transform_substrate_layers, transform_vias};
use transform::FixedTransform2D;

/// Instantiate a pre-compiled sub-space into the parent space
///
/// This function implements the "Affine Flat-Blitter" flattening algorithm:
/// 1. Look up the child space from the symbol table
/// 2. Recursively compile the child space if not already compiled
/// 3. Construct the affine transformation matrix
/// 4. Build net ID remapping from net_map
/// 5. Transform and copy all entities from child to parent
///
/// ## NO DEFAULTS, NO FALLBACKS
/// All operations use proper lookup tables and explicit validation.
pub fn instantiate_sub_space(
    placement: &SpaceInstancePlacement,
    symbol_table: &SymbolTable,
    eval_context: &EvaluationContext,
    parent_space: &mut HardwareSpace,
    unit_registry: &UnitRegistry,
    arena: &hwc_parser::ast::arena::AstArena,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
) -> Result<(), IrError> {
    // STEP 1: Look up the child space definition from symbol table
    // NO FALLBACK: If space doesn't exist, this is a compilation error
    let space_def = symbol_table
        .get_space(placement.space_name.as_str())
        .map_err(|_| {
            IrError::PlacementError(format!(
                "Space '{}' not found in symbol table. Did you import it?",
                placement.space_name
            ))
        })?;

    // STEP 2: Recursively compile the child space
    // This will populate a HardwareSpace with all entities
    let child_space =
        compile_child_space(space_def, symbol_table, eval_context, unit_registry, arena)?;

    // STEP 3: Evaluate position and construct transformation matrix
    let (x_nm, y_nm, z_layer) = evaluate_coordinate_nm(&placement.position, eval_context)?;

    let rotation = placement.rotation.as_ref().ok_or_else(|| {
        IrError::PlacementError(format!(
            "Space instance '{}' missing required rotation. Rotation must be explicit (0deg, 90deg, 180deg, or 270deg)",
            placement.instance_name.base
        ))
    })?;

    let transform = FixedTransform2D::new(x_nm, y_nm, z_layer as i64, rotation);

    // Register the entire child space as an instance in BoundingBoxTracker
    let origin_point = hwc_engine::Point3D::new(x_nm, y_nm, z_layer as i64);
    let instance_bbox = hwc_engine::geometry::BoundingBox::new(
        hwc_physics::Point3D::new(x_nm, y_nm, z_layer as i64),
        hwc_physics::Point3D::new(
            x_nm + child_space.dimensions.width_nm,
            y_nm + child_space.dimensions.height_nm,
            z_layer as i64 + child_space.dimensions.depth_nm,
        ),
    );
    bbox_tracker.register(
        placement.instance_name.base.clone(),
        instance_bbox,
        origin_point,
    );

    // Register all child pours with namespaced names (e.g., "PMOS_Inst.Source_Pad")
    for pour in &child_space.pours {
        let parent_pour_name = format!("{}.{}", placement.instance_name.base, pour.name);
        if let Some(ref child_bbox) = pour.bbox {
            let parent_bbox = transform.transform_bbox(child_bbox)?;
            let child_origin = hwc_engine::Point3D::new(
                child_bbox.min.x,
                child_bbox.min.y,
                child_bbox.min.z,
            );
            let (px, py, pz) = transform.transform_point(child_origin.x, child_origin.y, child_origin.z)?;
            let parent_origin = hwc_engine::Point3D::new(px, py, pz);
            bbox_tracker.register(parent_pour_name.into(), parent_bbox, parent_origin);
        }
    }

    // Register all child components with namespaced names
    for (child_comp_name, child_bbox) in &child_space.component_bboxes {
        let parent_comp_name = format!("{}.{}", placement.instance_name.base, child_comp_name);
        let parent_bbox = transform.transform_bbox(child_bbox)?;
        let child_origin = hwc_engine::Point3D::new(
            child_bbox.min.x,
            child_bbox.min.y,
            child_bbox.min.z,
        );
        let (px, py, pz) = transform.transform_point(child_origin.x, child_origin.y, child_origin.z)?;
        let parent_origin = hwc_engine::Point3D::new(px, py, pz);
        bbox_tracker.register(parent_comp_name.into(), parent_bbox, parent_origin);
    }

    // Register all child contacts with namespaced names
    for contact in &child_space.contacts {
        let parent_contact_name = format!("{}.{}", placement.instance_name.base, contact.name);
        if let Some(ref child_bbox) = contact.bbox {
            let parent_bbox = transform.transform_bbox(child_bbox)?;
            let child_origin = hwc_engine::Point3D::new(
                child_bbox.min.x,
                child_bbox.min.y,
                child_bbox.min.z,
            );
            let (px, py, pz) = transform.transform_point(child_origin.x, child_origin.y, child_origin.z)?;
            let parent_origin = hwc_engine::Point3D::new(px, py, pz);
            bbox_tracker.register(parent_contact_name.into(), parent_bbox, parent_origin);
        }
    }

//     eprintln!(
//         "[HIERARCHICAL] Transform: offset=({}, {}, {}), rotation={}Â°",
//         x_nm, y_nm, z_layer, transform.rotation_deg
//     );

    // STEP 4: Build net ID remapping table
    // Maps child's local NetIds to parent's NetIds using net_map
    let mut net_id_map = build_net_id_map(
        &placement.net_map,
        &child_space.netlist,
        &parent_space.netlist,
    )?;

    // Register any internal nets of the child space that were not in net_map
    for child_net_id in child_space.netlist.all_net_ids() {
        use std::collections::hash_map::Entry;
        if let Entry::Vacant(e) = net_id_map.entry(child_net_id) {
            let child_net_data = child_space.netlist.get_net(child_net_id).ok_or_else(|| {
                IrError::PlacementError(format!("Internal net ID {} not found", child_net_id.raw()))
            })?;
            let parent_net_name =
                format!("{}.{}", placement.instance_name.base, child_net_data.name);
            let parent_net_id =
                if let Some(id) = parent_space.netlist.get_net_by_name(&parent_net_name) {
                    id
                } else {
                    parent_space.netlist.add_net(
                        parent_net_name.into(),
                        child_net_data.width_nm,
                        child_net_data.material,
                    )
                };
            e.insert(parent_net_id);
        }
    }

//     eprintln!(
//         "[HIERARCHICAL] Net mapping: {} nets remapped",
//         net_id_map.len()
//     );

    // STEP 4.5: Transform and copy child netlist into parent (v0.2.1)
    transform_netlist(
        &child_space.netlist,
        &mut parent_space.netlist,
        &transform,
        &net_id_map,
        &placement.instance_name.base,
    )?;

    // STEP 5: Transform and copy substrate layers
    transform_substrate_layers(
        &child_space.entity_graph,
        &mut parent_space.entity_graph,
        &transform,
        &net_id_map,
        &placement.instance_name.base,
    )?;

    // STEP 6: Transform and copy routing segments
    transform_routing_segments(
        &child_space.entity_graph,
        &mut parent_space.entity_graph,
        &transform,
        &net_id_map,
        &placement.instance_name.base,
    )?;

    // STEP 7: Copy and rename entity registry entries for cross-instance routing
    transform_entity_registry(
        &child_space.entity_graph,
        &mut parent_space.entity_graph,
        &transform,
        &net_id_map,
        &placement.instance_name.base,
    )?;

    // STEP 7.5: Transform and copy other child space metadata (pours, contacts, keep-out zones, bboxes, vias)
    transform_pours(
        &child_space,
        parent_space,
        &transform,
        &placement.net_map,
        &placement.instance_name.base,
    )?;

    transform_contacts(
        &child_space,
        parent_space,
        &transform,
        &placement.net_map,
        &placement.instance_name.base,
    )?;

    transform_keep_out_zones(
        &child_space,
        parent_space,
        &transform,
        &net_id_map,
        &placement.net_map,
        &placement.instance_name.base,
    )?;

    transform_component_bboxes(
        &child_space,
        parent_space,
        &transform,
        &placement.instance_name.base,
    )?;

    transform_vias(
        &child_space,
        parent_space,
        &transform,
        &net_id_map,
        &placement.instance_name.base,
    )?;

    // v0.2.0: Transfer layer connection database entries from child to parent
    // This ensures that pours and vias registered in the child space are available
    // for routing in the parent space with their correct hierarchical names
    transfer_layer_connections(
        &child_space,
        parent_space,
        &transform,
        &placement.instance_name.base,
    )?;

    // v0.2.0: Register child routes in the hierarchical routing database
    // This enables proper connectivity validation and provenance tracking
    // Routes are now registered ONLY in the routing database, not in analytic_routes directly.
    register_child_routes_in_database(
        &child_space,
        parent_space,
        &transform,
        &net_id_map,
        &placement.net_map,
        &placement.instance_name.base,
    )?;

//     eprintln!(
//         "[HIERARCHICAL] Successfully instantiated space '{}' as '{}'",
//         placement.space_name, placement.instance_name.base
//     );

    Ok(())
}

/// Compile a child space definition into a HardwareSpace
///
/// This recursively invokes the main compilation pipeline for the child space.
/// The child space is compiled in isolation with its own entity graph and netlist.
pub(super) fn compile_child_space(
    space_def: &SpaceDefinition,
    symbol_table: &SymbolTable,
    eval_context: &EvaluationContext,
    unit_registry: &UnitRegistry,
    arena: &hwc_parser::ast::arena::AstArena,
) -> Result<HardwareSpace, IrError> {
//     eprintln!("[HIERARCHICAL] Compiling child space '{}'", space_def.name);

    // Create a fresh compilation context for the child space
    // This ensures the child has its own isolated netlist and entity graph
    let child_space = crate::ir::compilation::compile_space_recursive(
        space_def,
        symbol_table,
        eval_context,
        unit_registry,
        arena,
    )?;

//     eprintln!(
//         "[HIERARCHICAL] Child space '{}' compilation complete",
//         space_def.name
//     );

    Ok(child_space)
}

/// Evaluate a coordinate expression to nanometers for space instance placement.
///
/// Accepts both integer layer indices and physical measurements (e.g. `z: 0nm`)
/// for the Z axis, since space instances are placed at a physical origin in the
/// parent coordinate system rather than at a logical layer index.
pub(super) fn evaluate_coordinate_nm(
    coord: &hwc_parser::Coordinate,
    eval_context: &EvaluationContext,
) -> Result<(i64, i64, i32), IrError> {
    let (x_nm, y_nm, z_nm) = coord
        .evaluate_for_space_instance(eval_context)
        .map_err(|e| IrError::PlacementError(format!("Failed to evaluate coordinate: {}", e)))?;

    Ok((x_nm, y_nm, z_nm as i32))
}
