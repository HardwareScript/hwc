//! Transfer layer-connection DB entries and register child routes in the routing DB.

use crate::ir::errors::IrError;
use hwc_engine::netlist::NetId;
use hwc_engine::HardwareSpace;
use rustc_hash::FxHashMap;

use super::transform::FixedTransform2D;

/// Transfer layer connection database entries from child to parent space (v0.2.0)
///
/// When a child space is instantiated, all its registered connection points
/// (from pours, vias, etc.) need to be transferred to the parent space with
/// hierarchical naming and transformed coordinates.
pub(super) fn transfer_layer_connections(
    child_space: &HardwareSpace,
    parent_space: &mut HardwareSpace,
    transform: &FixedTransform2D,
    instance_name: &str,
) -> Result<(), IrError> {
    eprintln!(
        "[HIERARCHICAL] Transferring layer connection database ({} entities)",
        child_space
            .layer_connection_db
            .registered_entities()
            .count()
    );

    // Iterate over all entities that have registered connections in the child
    for entity_name in child_space.layer_connection_db.registered_entities() {
        // Get all layers this entity connects to
        if let Some(layers) = child_space
            .layer_connection_db
            .get_entity_connections(entity_name)
        {
            for layer_name in layers {
                // Get the connection point
                if let Ok(conn) = child_space
                    .layer_connection_db
                    .get_connection_point(entity_name, layer_name)
                {
                    // Transform the 2D position
                    let (new_x, new_y, _) = transform.transform_point(
                        conn.position_2d.0,
                        conn.position_2d.1,
                        0, // Z doesn't matter for 2D transform
                    )?;

                    // Transform the Z elevation
                    let new_z = conn.z_elevation + transform.offset_z_nm;

                    // Create hierarchical name
                    let hierarchical_name = format!("{}.{}", instance_name, entity_name);

                    // Register in parent space
                    let result = parent_space.layer_connection_db.register_surface(
                        &hierarchical_name,
                        &conn.layer_name,
                        new_z,
                        (new_x, new_y),
                        conn.material_id,
                        conn.connection_type,
                    );

                    if let Err(e) = result {
                        eprintln!(
                            "[HIERARCHICAL] WARNING: Failed to transfer connection for '{}' on layer '{}': {}",
                            hierarchical_name, conn.layer_name, e
                        );
                    } else {
                        eprintln!(
                            "[HIERARCHICAL] Transferred connection: '{}' -> '{}' on layer '{}' at Z={}nm",
                            entity_name, hierarchical_name, conn.layer_name, new_z
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

/// Register child instance routes in the hierarchical routing database (v0.2.0)
///
/// This function converts the transformed child analytic routes into TraceSegments
/// and registers them with provenance tracking in the parent space's routing database.
///
/// This enables:
/// - Proper hierarchical connectivity validation
/// - Clear error messages identifying which instance has routing issues
/// - Provenance tracking for debugging
pub(super) fn register_child_routes_in_database(
    child_space: &HardwareSpace,
    parent_space: &mut HardwareSpace,
    transform: &FixedTransform2D,
    net_id_map: &FxHashMap<NetId, NetId>,
    net_map: &FxHashMap<compact_str::CompactString, compact_str::CompactString>,
    instance_name: &str,
) -> Result<(), IrError> {
    eprintln!(
        "[ROUTING DB] Registering {} child routes for instance '{}'",
        child_space.analytic_routes.len(),
        instance_name
    );

    for route in &child_space.analytic_routes {
        // Remap net ID
        let parent_net_id = net_id_map.get(&route.net_id).copied().ok_or_else(|| {
            IrError::PlacementError(format!(
                "Analytic route with net {:?} has no mapping in net_map for instance '{}'",
                route.net_id, instance_name
            ))
        })?;

        // Get original child net name for provenance
        let original_net_name = route.net_name.clone();

        // Remap net name
        let parent_net_name = if let Some(parent_name) = net_map.get(&route.net_name) {
            parent_name.clone()
        } else {
            format!("{}.{}", instance_name, route.net_name).into()
        };

        // Convert LineSegments to TraceSegments
        let mut trace_segments = Vec::with_capacity(route.segments.len());
        for seg in &route.segments {
            let (start_x, start_y, start_z) =
                transform.transform_point(seg.start.x, seg.start.y, seg.start.z)?;
            let (end_x, end_y, end_z) =
                transform.transform_point(seg.end.x, seg.end.y, seg.end.z)?;

            trace_segments.push(hwc_engine::geometry::TraceSegment::new(
                hwc_engine::geometry::Point3D::new(start_x, start_y, start_z),
                hwc_engine::geometry::Point3D::new(end_x, end_y, end_z),
                route.cross_section.width_nm,
                route.material,
            ));
        }

        // Clone for debug print before moving
        let original_net_name_for_print = original_net_name.clone();
        let parent_net_name_for_print = parent_net_name.clone();

        // Register in routing database
        parent_space.routing_database.register_child_routes(
            instance_name.into(),
            parent_net_id,
            original_net_name,
            trace_segments,
        );

        eprintln!(
            "[ROUTING DB] Registered child route: instance='{}', net='{}' (parent net='{}', parent net_id={:?}), {} segments",
            instance_name, original_net_name_for_print, parent_net_name_for_print, parent_net_id, route.segments.len()
        );
    }

    Ok(())
}
