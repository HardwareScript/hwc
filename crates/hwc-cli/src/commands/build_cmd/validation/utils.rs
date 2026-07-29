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

    eprintln!("[CONNECTIVITY DEBUG] Converting {} substrate layers from entity_graph", 
        space.entity_graph.get_substrate_layers().len());

    // v0.1.8: Preserve indices to maintain compatibility with the Unified Spatial Index.
    // We no longer skip Net 0 layers here; they are handled by the Conductive Island Gate
    // and Substrate Isolation logic in the IslandBuilder.
    for (idx, layer) in space.entity_graph.get_substrate_layers().iter().enumerate() {
        let net_name = if layer.net != hwc_engine::netlist::NetId::UNCONNECTED {
            space
                .netlist
                .get_net(layer.net)
                .map(|net_data| net_data.name.clone())
        } else {
            None
        };

        eprintln!("[CONNECTIVITY DEBUG] Substrate layer {}: net={:?} type={:?} bbox=({},{},{}) -> ({},{},{})",
            idx, layer.net, layer.layer_type, 
            layer.bbox.min.x, layer.bbox.min.y, layer.bbox.min.z,
            layer.bbox.max.x, layer.bbox.max.y, layer.bbox.max.z);

        // NOTE on Z extents: We preserve the full physical Z extent of substrate pours
        // for connectivity checking. A previous "Z-plane flattening" fix shrunk pours to
        // ±5nm around their middle Z for rendering purposes, but this incorrectly breaks
        // the PIVB connectivity checker: a route/contact touching a pour at its physical
        // boundary (e.g., route ending at Z=1250 meeting a pour at Z=1250–1650) would
        // appear to miss the flattened version (Z=1445–1455), producing a false gap error.
        // Connectivity metadata must use actual geometry, not rendering approximations.

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
                .unwrap_or(hwc_engine::netlist::NetId::UNCONNECTED);

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
    //
    // **v0.2.0 FIX**: Use layer_z_range from trace for horizontal segments instead of
    // centering around segment.z. This ensures traces sit on their physical layer bounds.
    eprintln!("[CONNECTIVITY DEBUG] Processing {} analytic routes", space.analytic_routes.len());
    for trace in &space.analytic_routes {
        let half_w = trace.cross_section.width_nm / 2;

        eprintln!("[CONNECTIVITY DEBUG] Trace net={:?} width={} thickness={} layer_z_range={:?}", 
            trace.net_id, trace.cross_section.width_nm, trace.cross_section.thickness_nm, trace.layer_z_range);

        for (seg_idx, seg) in trace.segments.iter().enumerate() {
            let is_vertical = seg.start.x == seg.end.x && seg.start.y == seg.end.y;

            let x_min = seg.start.x.min(seg.end.x) - half_w;
            let x_max = seg.start.x.max(seg.end.x) + half_w;
            let y_min = seg.start.y.min(seg.end.y) - half_w;
            let y_max = seg.start.y.max(seg.end.y) + half_w;

            // NOTE on Z extents for route segments: Preserve the physical Z span.
            // For vertical segments (vias), this is start.z to end.z.
            // For horizontal segments, both endpoints share the same Z (routing layer),
            // so we use the layer_z_range if available for accurate thickness, otherwise
            // we fall back to the segment's Z. This ensures connectivity checks can
            // correctly detect overlap between traces and substrate pours at layer boundaries.
            let (z_min, z_max) = if is_vertical {
                // Via: use segment's actual Z-span
                (seg.start.z.min(seg.end.z), seg.start.z.max(seg.end.z))
            } else {
                // Horizontal trace: use the layer's physical Z range if known, else
                // fall back to the segment Z (which makes the trace a zero-thickness plane,
                // still sufficient since z_min <= z_max inclusive checks catch it).
                if let Some((lz_min, lz_max)) = trace.layer_z_range {
                    (lz_min, lz_max)
                } else {
                    (seg.start.z, seg.start.z)
                }
            };

            let bbox = BoundingBox::new(
                Point3D::new(x_min, y_min, z_min),
                Point3D::new(x_max, y_max, z_max),
            );

            eprintln!("[CONNECTIVITY DEBUG]   Segment {}: ({},{},{}) -> ({},{},{})", 
                seg_idx, seg.start.x, seg.start.y, seg.start.z, seg.end.x, seg.end.y, seg.end.z);
            eprintln!("[CONNECTIVITY DEBUG]     is_vertical={} bbox=({},{},{}) -> ({},{},{})",
                is_vertical, bbox.min.x, bbox.min.y, bbox.min.z, bbox.max.x, bbox.max.y, bbox.max.z);

            if is_vertical {
                // Treat as a vertical bridge
                eprintln!("[CONNECTIVITY DEBUG]     -> Added as CONTACT");
                physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                    material: trace.material,
                    net: trace.net_id,
                    net_name: Some(trace.net_name.clone()),
                    bbox,
                    layer_type: hwc_physics::connectivity::SubstrateLayerType::Contact,
                });
            } else {
                // Treat as a horizontal route
                eprintln!("[CONNECTIVITY DEBUG]     -> Added as ROUTE SEGMENT");
                physics_route_segments.push(hwc_physics::RouteSegmentMetadata {
                    net: trace.net_id,
                    net_name: Some(trace.net_name.clone()),
                    material: trace.material,
                    bbox,
                });
            }
        }
    }

    // Process routed segments stored in entity_graph.routed_segments.
    //
    // CRITICAL: These segments come from child spaces that were hierarchically compiled
    // and flattened into the parent (e.g., PMOS_Cell routes become PMOS_Inst routes in the
    // Inverter_Cell). Unlike top-level analytic_routes, these segments are stored in the
    // entity graph's routed_segments list with only a NetId and TraceSegments (no AnalyticTrace
    // wrapper, hence no layer_z_range metadata).
    //
    // Without this section, the PIVB connectivity checker cannot see the internal VDD/GND
    // routing wires inside sub-cells, causing false "disconnected net" violations.
    let routed_segments: Vec<_> = space.entity_graph.routed_segments().to_vec();
    eprintln!(
        "[CONNECTIVITY DEBUG] Processing {} entity_graph routed segment groups",
        routed_segments.len()
    );
    for (seg_net_id, segments) in &routed_segments {
        let net_name = space.netlist.get_net(*seg_net_id);
        let net_name_str = net_name.map(|n| n.name.clone());

        for seg in segments {
            // Determine if segment is a vertical bridge (via) or horizontal route.
            // A segment is vertical in 3D if it only moves in Z (X and Y are equal).
            let is_via = seg.start.x == seg.end.x && seg.start.y == seg.end.y;

            let seg_bbox = seg.bounding_box();

            if is_via {
                // Vertical segment (via): treat as Contact
                physics_substrate_layers.push(hwc_physics::connectivity::SubstrateLayerMetadata {
                    material: seg.material_id,
                    net: *seg_net_id,
                    net_name: net_name_str.clone(),
                    bbox: BoundingBox::new(
                        Point3D::new(seg_bbox.min.x, seg_bbox.min.y, seg_bbox.min.z),
                        Point3D::new(seg_bbox.max.x, seg_bbox.max.y, seg_bbox.max.z),
                    ),
                    layer_type: hwc_physics::connectivity::SubstrateLayerType::Contact,
                });
            } else {
                // Horizontal segment: treat as planar route segment
                physics_route_segments.push(hwc_physics::RouteSegmentMetadata {
                    net: *seg_net_id,
                    net_name: net_name_str.clone(),
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
            net: via.net_id,
            net_name,
            bbox,
            layer_type: hwc_physics::connectivity::SubstrateLayerType::Contact,
        });
    }

    (physics_substrate_layers, physics_route_segments)
}
