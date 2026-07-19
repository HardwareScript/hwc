use hwc_engine::HardwareSpace;
use hwc_physics::geometry::{BoundingBox, Point3D};

pub fn convert_metadata_to_physics(
    space: &HardwareSpace,
) -> (
    Vec<hwc_physics::connectivity::SubstrateLayerMetadata>,
    Vec<hwc_physics::RouteSegmentMetadata>,
) {
    let mut physics_substrate_layers: Vec<hwc_physics::connectivity::SubstrateLayerMetadata> =
        Vec::new();

    // v0.1.8: Preserve indices to maintain compatibility with the Unified Spatial Index.
    // We no longer skip Net 0 layers here; they are handled by the Conductive Island Gate
    // and Substrate Isolation logic in the IslandBuilder.
    for layer in space.entity_graph.get_substrate_layers() {
        let net_name = if layer.net != 0 {
            space
                .netlist
                .get_net(hwc_engine::netlist::NetId::new(layer.net))
                .map(|net_data| net_data.name.clone())
        } else {
            None
        };

        if layer.regions.is_empty() {
            physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                material: layer.material,
                net: layer.net,
                net_name,
                bbox: BoundingBox::new(
                    Point3D::new(layer.bbox.min.x, layer.bbox.min.y, layer.bbox.min.z),
                    Point3D::new(layer.bbox.max.x, layer.bbox.max.y, layer.bbox.max.z),
                ),
                layer_type: layer.layer_type,
            });
        } else {
            for region in &layer.regions {
                physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                    material: layer.material,
                    net: layer.net,
                    net_name: net_name.clone(),
                    bbox: BoundingBox::new(
                        Point3D::new(region.min.x, region.min.y, region.min.z),
                        Point3D::new(region.max.x, region.max.y, region.max.z),
                    ),
                    layer_type: layer.layer_type,
                });
            }
        }
    }

    // v0.1.8: Include auto-inserted vias (space.contacts) in the physics metadata.
    // The AutoViaInserter adds vias to the space.contacts list, but these are not
    // automatically reflected in the entity graph's substrate layers. We must explicitly
    // convert them to SubstrateLayerMetadata with SubstrateLayerType::Contact to ensure
    // the IslandBuilder sees the complete conductive path.
    for contact in &space.contacts {
        let material_id = space.material_registry.get_id(&contact.material_name);
        if let Some(id) = material_id {
            let net_id = contact
                .net
                .as_ref()
                .and_then(|name| space.netlist.get_net_by_name(name))
                .map(|n| n.raw())
                .unwrap_or(0);

            if let Some(bbox) = &contact.bbox {
                physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                    material: id,
                    net: net_id,
                    net_name: contact.net.clone(),
                    bbox: BoundingBox::new(
                        Point3D::new(bbox.min.x, bbox.min.y, bbox.min.z),
                        Point3D::new(bbox.max.x, bbox.max.y, bbox.max.z),
                    ),
                    layer_type: hwc_physics::connectivity::SubstrateLayerType::Contact,
                });
            }
        }
    }

    let mut physics_route_segments: Vec<hwc_physics::RouteSegmentMetadata> = Vec::new();

    // v0.1.8: Process analytic routes.
    // Vertical routes (where start.x == end.x and start.y == end.y) are treated
    // as vertical bridges (contacts). Horizontal routes are treated as planar segments.
    for trace in &space.analytic_routes {
        let half_w = trace.cross_section.width_nm / 2;
        let half_t = trace.cross_section.thickness_nm / 2;

        for seg in &trace.segments {
            let is_vertical = seg.start.x == seg.end.x && seg.start.y == seg.end.y;

            let x_min = seg.start.x.min(seg.end.x) - half_w;
            let x_max = seg.start.x.max(seg.end.x) + half_w;
            let y_min = seg.start.y.min(seg.end.y) - half_w;
            let y_max = seg.start.y.max(seg.end.y) + half_w;
            let z_min = seg.start.z.min(seg.end.z) - half_t;
            let z_max = seg.start.z.max(seg.end.z) + half_t;

            let bbox = BoundingBox::new(
                Point3D::new(x_min, y_min, z_min),
                Point3D::new(x_max, y_max, z_max),
            );

            if is_vertical {
                // Treat as a vertical bridge
                physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                    material: trace.material,
                    net: trace.net_id.raw(),
                    net_name: Some(trace.net_name.clone()),
                    bbox,
                    layer_type: hwc_physics::connectivity::SubstrateLayerType::Contact,
                });
            } else {
                // Treat as a horizontal route
                physics_route_segments.push(hwc_physics::RouteSegmentMetadata {
                    net: trace.net_id.raw(),
                    net_name: Some(trace.net_name.clone()),
                    material: trace.material,
                    bbox,
                });
            }
        }
    }

    // v0.1.8: Process explicit via objects (vias placed by the auto-router or ViaResolver)
    for via in &space.vias {
        let radius = via.diameter_nm / 2;
        let bbox = BoundingBox::new(
            Point3D::new(
                via.position.0 - radius,
                via.position.1 - radius,
                via.from_z_nm,
            ),
            Point3D::new(
                via.position.0 + radius,
                via.position.1 + radius,
                via.to_z_nm,
            ),
        );

        let material_id = via.material_id;
        let net_name = space.netlist.get_net_name(via.net_id);

        physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
            material: material_id,
            net: via.net_id.raw(),
            net_name,
            bbox,
            layer_type: hwc_physics::connectivity::SubstrateLayerType::Contact,
        });
    }

    (physics_substrate_layers, physics_route_segments)
}
