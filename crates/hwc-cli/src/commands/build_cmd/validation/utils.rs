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

    // v0.2.0: Use technology strategy from space (set during compilation)
    let strategy = space.technology_strategy;
    let annular_ring_nm = strategy.contact_expansion(
        space.fabrication_constraints.as_ref().map(|c| c.via.min_annular_ring_nm).unwrap_or(0),
    );

    // v0.1.8: Preserve indices to maintain compatibility with the Unified Spatial Index.
    // We no longer skip Net 0 layers here; they are handled by the Conductive Island Gate
    // and Substrate Isolation logic in the IslandBuilder.
    for (_idx, layer) in space.entity_graph.get_substrate_layers().iter().enumerate() {
        let net_name = if layer.net != hwc_engine::netlist::NetId::UNCONNECTED {
            space
                .netlist
                .get_net(layer.net)
                .map(|net_data| net_data.name.clone())
        } else {
            None
        };

        // NOTE on Z extents: We preserve the full physical Z extent of substrate pours
        // for connectivity checking. A previous "Z-plane flattening" fix shrunk pours to
        // ±5nm around their middle Z for rendering purposes, but this incorrectly breaks
        // the PIVB connectivity checker: a route/contact touching a pour at its physical
        // boundary (e.g., route ending at Z=1250 meeting a pour at Z=1250–1650) would
        // appear to miss the flattened version (Z=1445–1455), producing a false gap error.
        // Connectivity metadata must use actual geometry, not rendering approximations.

        // v0.2.0 FIX: Apply annular ring expansion for Contact layers to account for
        // PCB pad overhangs. For ASIC (annular_ring_nm == 0), this has no effect.
        let is_contact = layer.layer_type == hwc_engine::geometry_router::substrate_types::SubstrateLayerType::Contact;
        let expansion = if is_contact { annular_ring_nm } else { 0 };

        if layer.regions.is_empty() {
            let min_x = layer.bbox.min.x - expansion;
            let max_x = layer.bbox.max.x + expansion;
            let min_y = layer.bbox.min.y - expansion;
            let max_y = layer.bbox.max.y + expansion;

            let device_binding = layer.device_binding.as_ref().map(|(dev_name, terminal)| {
                hwc_physics::connectivity::DeviceBinding {
                    device_name: dev_name.as_str().into(),
                    terminal: terminal.as_str().into(),
                }
            });

            physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                material: layer.material,
                net: layer.net,
                net_name,
                bbox: BoundingBox::new(
                    Point3D::new(min_x, min_y, layer.bbox.min.z),
                    Point3D::new(max_x, max_y, layer.bbox.max.z),
                ),
                layer_type: layer.layer_type,
                device_binding,
            });
        } else {
            for region in &layer.regions {
                let reg_min_x = region.min.x - expansion;
                let reg_max_x = region.max.x + expansion;
                let reg_min_y = region.min.y - expansion;
                let reg_max_y = region.max.y + expansion;

                let device_binding = layer.device_binding.as_ref().map(|(dev_name, terminal)| {
                    hwc_physics::connectivity::DeviceBinding {
                        device_name: dev_name.as_str().into(),
                        terminal: terminal.as_str().into(),
                    }
                });

                physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                    material: layer.material,
                    net: layer.net,
                    net_name: net_name.clone(),
                    bbox: BoundingBox::new(
                        Point3D::new(reg_min_x, reg_min_y, region.min.z),
                        Point3D::new(reg_max_x, reg_max_y, region.max.z),
                    ),
                    layer_type: layer.layer_type,
                    device_binding,
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
                .unwrap_or(hwc_engine::netlist::NetId::UNCONNECTED);

            if let Some(bbox) = &contact.bbox {
                // v0.2.0 FIX: Apply annular ring expansion for contacts
                physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                    material: id,
                    net: net_id,
                    net_name: contact.net.clone(),
                    bbox: BoundingBox::new(
                        Point3D::new(bbox.min.x - annular_ring_nm, bbox.min.y - annular_ring_nm, bbox.min.z),
                        Point3D::new(bbox.max.x + annular_ring_nm, bbox.max.y + annular_ring_nm, bbox.max.z),
                    ),
                    layer_type: hwc_physics::connectivity::SubstrateLayerType::Contact,
                    device_binding: None,
                });
            }
        }
    }

    let mut physics_route_segments: Vec<hwc_physics::RouteSegmentMetadata> = Vec::new();

    // v0.1.8: Process analytic routes.
    // Vertical routes (where start.x == end.x and start.y == end.y) are treated
    // as vertical bridges (contacts). Horizontal routes are treated as planar segments.
    //
    // **v0.2.0 FIX**: Use hierarchical routing database to get all routes (child + parent)
    // with proper provenance tracking instead of mixing entity_graph and analytic_routes.
    //
    // **v0.2.2 ARCHITECTURAL FIX**: Use direct layer lineage lookup instead of reverse
    // Z-coordinate guessing. Routes store their layer name; materials come from the
    // RoutingLayerDatabase, not from spatial stackup queries.
    let all_routes = space.routing_database.export_as_routed_segments_with_lineage(
        &space.routing_layer_db,
    );
    
    for (net_id, segments) in &all_routes {
        let net_name = space.netlist.get_net(*net_id).map(|n| n.name.clone());
        
        for (_seg_idx, seg) in segments.iter().enumerate() {
            let is_vertical = seg.start.x == seg.end.x && seg.start.y == seg.end.y;
            let seg_bbox = seg.bounding_box();
            
            if is_vertical {
                // Treat as a vertical bridge
                physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                    material: seg.material_id,
                    net: *net_id,
                    net_name: net_name.clone(),
                    bbox: BoundingBox::new(
                        Point3D::new(seg_bbox.min.x, seg_bbox.min.y, seg_bbox.min.z),
                        Point3D::new(seg_bbox.max.x, seg_bbox.max.y, seg_bbox.max.z),
                    ),
                    layer_type: hwc_physics::connectivity::SubstrateLayerType::Contact,
                    device_binding: None,
                });
            } else {
                // Treat as a horizontal route
                physics_route_segments.push(hwc_physics::RouteSegmentMetadata {
                    net: *net_id,
                    net_name: net_name.clone(),
                    material: seg.material_id,
                    bbox: BoundingBox::new(
                        Point3D::new(seg_bbox.min.x, seg_bbox.min.y, seg_bbox.min.z),
                        Point3D::new(seg_bbox.max.x, seg_bbox.max.y, seg_bbox.max.z),
                    ),
                });
            }
        }
    }

    // v0.1.8: Process explicit via objects (vias placed by the auto-router or ViaResolver)
    for via in &space.vias {
        let radius = via.diameter_nm / 2;
        // v0.2.0 FIX: Apply annular ring expansion for vias
        let bbox = BoundingBox::new(
            Point3D::new(
                via.position.0 - radius - annular_ring_nm,
                via.position.1 - radius - annular_ring_nm,
                via.from_z_nm,
            ),
            Point3D::new(
                via.position.0 + radius + annular_ring_nm,
                via.position.1 + radius + annular_ring_nm,
                via.to_z_nm,
            ),
        );

        let material_id = via.material_id;
        let net_name = space.netlist.get_net_name(via.net_id);

        physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
            material: material_id,
            net: via.net_id,
            net_name,
            bbox,
            layer_type: hwc_physics::connectivity::SubstrateLayerType::Contact,
            device_binding: None,
        });
    }

    (physics_substrate_layers, physics_route_segments)
}
