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
//     eprintln!(
//         "[HIERARCHICAL] Transforming {} substrate layers",
//         child_graph.substrate_layers.len()
//     );

    for child_layer in &child_graph.substrate_layers {
        // Clone the layer
        let mut transformed_layer = child_layer.clone();

        // Transform bounding box
        transformed_layer.bbox = transform.transform_bbox(&child_layer.bbox)?;

        // Transform sub-regions if present
        let mut transformed_regions = smallvec::SmallVec::new();
        for region in &child_layer.regions {
            transformed_regions.push(transform.transform_bbox(region)?);
        }
        transformed_layer.regions = transformed_regions;

        // Transform Polygon shape coordinates (outer_contour and holes) to parent world space
        if let hwc_engine::geometry_router::substrate_types::SubstrateLayerShape::Polygon {
            outer_contour,
            holes,
            segments,
        } = &child_layer.shape
        {
            let mut transformed_contour = clipper2_rust::Path64::new();
            for pt in outer_contour {
                let (tx, ty, _) = transform.transform_point(pt.x, pt.y, 0)?;
                transformed_contour.push(clipper2_rust::Point64::new(tx, ty));
            }

            let mut transformed_holes = clipper2_rust::Paths64::new();
            for hole in holes {
                let mut transformed_hole = clipper2_rust::Path64::new();
                for pt in hole {
                    let (tx, ty, _) = transform.transform_point(pt.x, pt.y, 0)?;
                    transformed_hole.push(clipper2_rust::Point64::new(tx, ty));
                }
                transformed_holes.push(transformed_hole);
            }

            transformed_layer.shape =
                hwc_engine::geometry_router::substrate_types::SubstrateLayerShape::Polygon {
                    outer_contour: transformed_contour,
                    holes: transformed_holes,
                    segments: *segments,
                };
        }

        // Remap net ID
        // NetId(0) is the "no-net" sentinel used by dielectric/mask substrate layers;
        // pass it through unchanged without requiring a net_map entry.
        if child_layer.net == NetId(0) {
            transformed_layer.net = NetId(0);
        } else if let Some(&parent_net_id) = net_id_map.get(&child_layer.net) {
            transformed_layer.net = parent_net_id;
        } else {
            return Err(IrError::PlacementError(format!(
                "Substrate layer with net {:?} has no mapping in net_map",
                child_layer.net
            )));
        }

        // Register in parent graph
        parent_graph.substrate_layers.push(transformed_layer);
    }

//     eprintln!(
//         "[HIERARCHICAL] Substrate layer transformation complete: {} layers added to parent",
//         child_graph.substrate_layers.len()
//     );

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
//     eprintln!(
//         "[HIERARCHICAL] Transforming {} routing segment groups",
//         child_graph.routed_segment_count()
//     );

    for (child_net_id, segments) in child_graph.iter_routed_segments() {
        // Remap net ID. NetId(0) = no-net sentinel; pass through unchanged.
        let parent_net_id = if *child_net_id == NetId(0) {
            NetId(0)
        } else {
            net_id_map.get(child_net_id).copied().ok_or_else(|| {
                IrError::PlacementError(format!(
                    "Routing segment with net {:?} has no mapping in net_map",
                    child_net_id
                ))
            })?
        };

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
            transformed_seg.is_frozen = true;

            transformed_segments.push(transformed_seg);
        }

        // Register in parent graph
        parent_graph.add_routed_segments(parent_net_id, transformed_segments);
    }

//     eprintln!(
//         "[HIERARCHICAL] Routing segment transformation complete: {} total segments added to parent",
//         total_segments
//     );

    Ok(())
}

/// Transform and copy child vias to the parent space
pub(super) fn transform_vias(
    child_space: &hwc_engine::HardwareSpace,
    parent_space: &mut hwc_engine::HardwareSpace,
    transform: &FixedTransform2D,
    net_id_map: &FxHashMap<NetId, NetId>,
    instance_name: &str,
) -> Result<(), IrError> {
//     eprintln!(
//         "[HIERARCHICAL] Transforming {} vias",
//         child_space.vias.len()
//     );

    for via in &child_space.vias {
        let (tx, ty, _) =
            transform.transform_point(via.position.0, via.position.1, via.from_z_nm)?;
        let parent_from_z = via.from_z_nm + transform.offset_z_nm;
        let parent_to_z = via.to_z_nm + transform.offset_z_nm;

        // NetId(0) = no-net sentinel; pass through unchanged.
        let parent_net_id = if via.net_id == NetId(0) {
            NetId(0)
        } else {
            net_id_map.get(&via.net_id).copied().ok_or_else(|| {
                IrError::PlacementError(format!(
                    "Via with net {:?} has no mapping in net_map",
                    via.net_id
                ))
            })?
        };

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
            is_frozen: true,
            parent_instance: Some(instance_name.into()),
        });
    }

    Ok(())
}
