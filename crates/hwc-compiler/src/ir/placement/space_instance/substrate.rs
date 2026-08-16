//! Transform and copy substrate layers, routing segments, and vias.

use crate::ir::errors::IrError;
use hwc_engine::geometry_router::entity_graph::EntityGraph;
use hwc_engine::netlist::NetId;
use rustc_hash::FxHashMap;

use super::transform::FixedTransform2D;

/// Transform and copy substrate layers from child to parent
///
/// Applies coordinate transformation and net remapping to each substrate layer.
/// NO IMPLICIT BEHAVIOR: Every layer is explicitly transformed and validated.
///
/// v0.2.1: Registers entities with hierarchical names (e.g., "PMOS_Inst.Out_Pad")
/// to enable cross-instance routing in the parent space.
pub(super) fn transform_substrate_layers(
    child_graph: &EntityGraph,
    parent_graph: &mut EntityGraph,
    transform: &FixedTransform2D,
    net_id_map: &FxHashMap<NetId, NetId>,
    _instance_name: &str, // Reserved for future use
) -> Result<(), IrError> {
    eprintln!(
        "[HIERARCHICAL] Transforming {} substrate layers",
        child_graph.substrate_layers.len()
    );

    for child_layer in &child_graph.substrate_layers {
        // Clone the layer
        let mut transformed_layer = child_layer.clone();

        // Transform bounding box
        transformed_layer.bbox = transform.transform_bbox(&child_layer.bbox)?;

        // Remap net ID
        if let Some(&parent_net_id) = net_id_map.get(&child_layer.net) {
            transformed_layer.net = parent_net_id;
        } else {
            // Net not in map - this is an error (no implicit behavior)
            return Err(IrError::PlacementError(format!(
                "Substrate layer with net {:?} has no mapping in net_map",
                child_layer.net
            )));
        }

        // Register in parent graph
        parent_graph.substrate_layers.push(transformed_layer);
    }

    eprintln!(
        "[HIERARCHICAL] Substrate layer transformation complete: {} layers added to parent",
        child_graph.substrate_layers.len()
    );

    Ok(())
}

/// Transform and copy routing segments from child to parent
///
/// Applies coordinate transformation and net remapping to each routing segment.
/// NO IMPLICIT BEHAVIOR: Every segment is explicitly transformed and validated.
pub(super) fn transform_routing_segments(
    child_graph: &EntityGraph,
    parent_graph: &mut EntityGraph,
    transform: &FixedTransform2D,
    net_id_map: &FxHashMap<NetId, NetId>,
    _instance_name: &str, // Reserved for future use
) -> Result<(), IrError> {
    eprintln!(
        "[HIERARCHICAL] Transforming {} routing segment groups",
        child_graph.routed_segment_count()
    );

    let mut total_segments = 0;

    for (child_net_id, segments) in child_graph.iter_routed_segments() {
        // Remap net ID
        let parent_net_id = net_id_map.get(child_net_id).copied().ok_or_else(|| {
            IrError::PlacementError(format!(
                "Routing segment with net {:?} has no mapping in net_map",
                child_net_id
            ))
        })?;

        // Transform each segment
        let mut transformed_segments = Vec::new();
        for seg in segments {
            let mut transformed_seg = seg.clone();

            // Transform start and end points
            let (start_x, start_y, start_z) =
                transform.transform_point(seg.start.x, seg.start.y, seg.start.z)?;
            let (end_x, end_y, end_z) =
                transform.transform_point(seg.end.x, seg.end.y, seg.end.z)?;

            transformed_seg.start.x = start_x;
            transformed_seg.start.y = start_y;
            transformed_seg.start.z = start_z;

            transformed_seg.end.x = end_x;
            transformed_seg.end.y = end_y;
            transformed_seg.end.z = end_z;

            transformed_segments.push(transformed_seg);
            total_segments += 1;
        }

        // Register in parent graph
        parent_graph.add_routed_segments(parent_net_id, transformed_segments);
    }

    eprintln!(
        "[HIERARCHICAL] Routing segment transformation complete: {} total segments added to parent",
        total_segments
    );

    Ok(())
}

/// Transform and copy child vias to the parent space
pub(super) fn transform_vias(
    child_space: &hwc_engine::HardwareSpace,
    parent_space: &mut hwc_engine::HardwareSpace,
    transform: &FixedTransform2D,
    net_id_map: &FxHashMap<NetId, NetId>,
) -> Result<(), IrError> {
    eprintln!(
        "[HIERARCHICAL] Transforming {} vias",
        child_space.vias.len()
    );

    for via in &child_space.vias {
        let (tx, ty, _) =
            transform.transform_point(via.position.0, via.position.1, via.from_z_nm)?;
        let parent_from_z = via.from_z_nm + transform.offset_z_nm;
        let parent_to_z = via.to_z_nm + transform.offset_z_nm;

        let parent_net_id = net_id_map.get(&via.net_id).copied().ok_or_else(|| {
            IrError::PlacementError(format!(
                "Via with net {:?} has no mapping in net_map",
                via.net_id
            ))
        })?;

        parent_space.vias.push(hwc_engine::geometry_router::Via {
            position: (tx, ty),
            from_z_nm: parent_from_z,
            to_z_nm: parent_to_z,
            diameter_nm: via.diameter_nm,
            net_id: parent_net_id,
            material_id: via.material_id,
            via_type: via.via_type,
            enclosure_nm: via.enclosure_nm,
            properties: via.properties.clone(),
        });
    }

    Ok(())
}
