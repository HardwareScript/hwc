use hwc_engine::HardwareSpace;

pub fn convert_metadata_to_physics(
    space: &HardwareSpace,
) -> (
    Vec<hwc_physics::connectivity::SubstrateLayerMetadata>,
    Vec<hwc_physics::RouteSegmentMetadata>,
) {
    let mut physics_substrate_layers: Vec<hwc_physics::connectivity::SubstrateLayerMetadata> = Vec::new();

    for layer in space.entity_graph.get_substrate_layers() {
        if layer.net == 0 {
            continue;
        }

        let net_name = space
            .netlist
            .get_net(hwc_engine::netlist::NetId::new(layer.net))
            .map(|net_data| net_data.name.clone());

        if layer.regions.is_empty() {
            physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                material: layer.material,
                net: layer.net,
                net_name,
                bbox: hwc_physics::connectivity::BoundingBox {
                    min_x: layer.bbox.min.x,
                    min_y: layer.bbox.min.y,
                    min_z: layer.bbox.min.z,
                    max_x: layer.bbox.max.x,
                    max_y: layer.bbox.max.y,
                    max_z: layer.bbox.max.z,
                },
            });
        } else {
            for region in &layer.regions {
                physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                    material: layer.material,
                    net: layer.net,
                    net_name: net_name.clone(),
                    bbox: hwc_physics::connectivity::BoundingBox {
                        min_x: region.min.x,
                        min_y: region.min.y,
                        min_z: region.min.z,
                        max_x: region.max.x,
                        max_y: region.max.y,
                        max_z: region.max.z,
                    },
                });
            }
        }
    }

    let physics_route_segments: Vec<hwc_physics::RouteSegmentMetadata> = space
        .analytic_routes
        .iter()
        .flat_map(|trace| {
            let half_w = trace.width_nm / 2;
            let half_t = trace.thickness_nm / 2;

            trace.segments.iter().map(move |seg| {
                let x_min = seg.start.x.min(seg.end.x) - half_w;
                let x_max = seg.start.x.max(seg.end.x) + half_w;
                let y_min = seg.start.y.min(seg.end.y) - half_w;
                let y_max = seg.start.y.max(seg.end.y) + half_w;
                let z_min = seg.start.z.min(seg.end.z) - half_t;
                let z_max = seg.start.z.max(seg.end.z) + half_t;

                hwc_physics::RouteSegmentMetadata {
                    net: trace.net_id.raw(),
                    net_name: Some(trace.net_name.clone()),
                    material: trace.material,
                    bbox: hwc_physics::connectivity::BoundingBox {
                        min_x: x_min,
                        min_y: y_min,
                        min_z: z_min,
                        max_x: x_max,
                        max_y: y_max,
                        max_z: z_max,
                    },
                }
            })
        })
        .collect();

    (physics_substrate_layers, physics_route_segments)
}
