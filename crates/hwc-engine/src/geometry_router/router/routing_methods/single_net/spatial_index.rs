use crate::geometry_router::router::core::GeometryRouter;
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::geometry_router::types::NetRoute;
use crate::geometry_router::EntityGraph;

impl GeometryRouter {
    /// Build a DynamicSpatialIndex from component obstacles and committed vector trace segments.
    ///
    /// Excludes the start and goal components of the active route from the obstacle map,
    /// enabling the pathfinder to dock to ports cleanly without self-collision deadlocks.
    /// All OTHER components (even those on the same net!) and all previously routed segments
    /// (except those touching our endpoints) are treated as hard obstacles.
    pub(crate) fn build_routing_spatial_index(
        &self,
        entity_graph: &EntityGraph,
        active_route: &NetRoute,
    ) -> DynamicSpatialIndex {
        let mut spatial_index = DynamicSpatialIndex::new();

        // Configure layer Z-ranges from the stackup for layered queries
        if self.config.layer_z_positions.len() >= 2 {
            let mut z_ranges = Vec::with_capacity(self.config.layer_z_positions.len());
            for i in 0..self.config.layer_z_positions.len() {
                let z_min = self.config.layer_z_positions[i];
                let z_max = if i + 1 < self.config.layer_z_positions.len() {
                    self.config.layer_z_positions[i + 1]
                } else {
                    self.bounds.depth_nm
                };
                z_ranges.push((z_min, z_max));
            }
            spatial_index.set_layer_z_ranges(&z_ranges);
        }

        let mut seg_id = 0usize;

        // Resolve component names for the start and goal positions of the active route
        let start_comp = entity_graph.point_in_component(
            active_route.start.x,
            active_route.start.y,
            active_route.start.z,
        );
        let goal_comp = entity_graph.point_in_component(
            active_route.goal.x,
            active_route.goal.y,
            active_route.goal.z,
        );

        // 1. Insert component boundaries as layer-aware keepout obstacles
        // Reference: Docs/v0.1.9/13-PHYSICAL-SYNTHESIS-GUARDRAILS.md (Interior Lockout Rule)
        eprintln!(
            "[OBSTACLE-DEBUG] net {:?}: {} component metadata entries",
            active_route.net_id,
            entity_graph.get_component_metadata().len()
        );
        for meta in entity_graph.get_component_metadata() {
            eprintln!(
                "[OBSTACLE-DEBUG]   meta '{}' type='{}' bbox=({},{},{})-({},{},{})",
                meta.name,
                meta.component_type,
                meta.bbox.min.x,
                meta.bbox.min.y,
                meta.bbox.min.z,
                meta.bbox.max.x,
                meta.bbox.max.y,
                meta.bbox.max.z
            );
            // EXEMPTION GUARD: Only exclude the start and goal components of the active route.
            // Even if other components are on the same net, they are physical obstacles
            // that should be routed around unless we are specifically tapping into them.
            if start_comp.as_deref() == Some(meta.name.as_str())
                || goal_comp.as_deref() == Some(meta.name.as_str())
            {
                continue;
            }

            // LAYER-AWARE KEEPOUT: Components only block routing on layers where they have physical material
            // If blocked_z_ranges is empty, the component blocks its entire bbox Z-range (legacy behavior)
            let z_ranges_to_block: Vec<(i64, i64)> = if meta.blocked_z_ranges.is_empty() {
                // No explicit layer blocking - use full component Z extent
                vec![(meta.bbox.min.z, meta.bbox.max.z)]
            } else {
                // Use explicit layer-aware blocking ranges
                meta.blocked_z_ranges.iter().copied().collect()
            };

            // Insert one obstacle segment per Z-range where component has material
            for (z_min, z_max) in z_ranges_to_block {
                // Component keepout semantics:
                // - start/end define the rectangular physical boundary
                // - width_nm = 0 means "no additional inflation" (raw bbox)
                // - Router applies clearance via route segment inflation, not obstacle inflation
                // - This prevents traces from penetrating component interior while maintaining
                //   proper clearance via the routing algorithm's Minkowski sum
                spatial_index.insert(IndexedSegment {
                    source: hwc_physics::spatial_index::SpatialEntitySource::ComponentInstance {
                        instance_id: seg_id,
                    },
                    segment_id: seg_id,
                    net_id: crate::netlist::NetId::UNCONNECTED,
                    width_nm: hwc_physics::spatial_index::IndexedSegment::BBOX_OBSTACLE_WIDTH,
                    thickness_nm: z_max - z_min,
                    start: hwc_physics::geometry::Point3D::new(
                        meta.bbox.min.x,
                        meta.bbox.min.y,
                        z_min,
                    ),
                    end: hwc_physics::geometry::Point3D::new(
                        meta.bbox.max.x,
                        meta.bbox.max.y,
                        z_max,
                    ),
                    layer: z_min,
                    device_binding: None, // Component keepouts don't have device bindings
                });
                seg_id += 1;
            }
        }

        // 2. Insert substrate layers (pours) as hard obstacles.
        // Pours belong to specific nets; routes must not pass through pours of other nets.
        // v0.1.9: Planes without nets (net_id = 0) are keepout zones and MUST be obstacles.

        // v0.1.9: Use self.substrate_layers (populated by route_space) instead of
        // entity_graph.get_substrate_layers() which is empty during routing.
        if let Some(substrate_layers) = &self.substrate_layers {
            eprintln!(
                "[OBSTACLE-DEBUG]   {} substrate layers",
                substrate_layers.len()
            );
            for (i, sub_layer) in substrate_layers.iter().enumerate() {
                eprintln!(
                    "[OBSTACLE-DEBUG]     sub[{}] net={:?} bbox=({},{},{})-({},{},{})",
                    i,
                    sub_layer.net,
                    sub_layer.bbox.min.x,
                    sub_layer.bbox.min.y,
                    sub_layer.bbox.min.z,
                    sub_layer.bbox.max.x,
                    sub_layer.bbox.max.y,
                    sub_layer.bbox.max.z
                );
            }
            for (substrate_idx, sub_layer) in substrate_layers.iter().enumerate() {
                // v0.2.3: Use centralized obstacle query system (NO inline conditionals!)
                let route_context = crate::geometry_router::obstacle_query::RouteContext {
                    net_id: active_route.net_id,
                    start: active_route.start,
                    goal: active_route.goal,
                    trace_width_nm: self.trace_width_nm,
                };

                use crate::geometry_router::obstacle_query::{ObstacleDecision, ObstacleQuery};

                match ObstacleQuery::is_obstacle_for(sub_layer, &route_context) {
                    Ok(ObstacleDecision::Exempt { reason }) => {
                        eprintln!(
                            "[OBSTACLE-DEBUG]     sub[{}] EXEMPTED: {:?}",
                            substrate_idx, reason
                        );
                        continue;
                    }
                    Ok(ObstacleDecision::IsObstacle { reason }) => {
                        eprintln!(
                            "[OBSTACLE-DEBUG]     sub[{}] IS OBSTACLE: {:?}",
                            substrate_idx, reason
                        );
                        // Continue to obstacle insertion below
                    }
                    Err(err) => {
                        // FAIL LOUDLY: Obstacle logic is ambiguous
                        eprintln!(
                            "[OBSTACLE-DEBUG] ERROR: Obstacle query failed for sub[{}]: {}",
                            substrate_idx, err
                        );
                        eprintln!(
                            "[OBSTACLE-DEBUG]   Layer: net={:?}, type={:?}, bbox={:?}",
                            sub_layer.net, sub_layer.layer_type, sub_layer.bbox
                        );
                        panic!(
                            "Routing obstacle query encountered unhandled state. This is a compiler bug.\n\
                             Fix the obstacle query system to handle this case explicitly.\n\
                             Error: {}",
                            err
                        );
                    }
                }

                // Use a stable segment_id based on the substrate layer index
                // This ensures the SAME physical substrate layer always gets the SAME segment_id
                // across multiple builds of the spatial index
                let stable_segment_id = 1_000_000 + substrate_idx;

                spatial_index.insert(IndexedSegment {
                    source: hwc_physics::spatial_index::SpatialEntitySource::SubstrateLayer {
                        index: substrate_idx,
                    },
                    segment_id: stable_segment_id,
                    net_id: sub_layer.net, // v0.2.3: Use sub_layer.net directly
                    width_nm: 0,
                    thickness_nm: sub_layer.bbox.max.z - sub_layer.bbox.min.z,
                    start: sub_layer.bbox.min,
                    end: sub_layer.bbox.max,
                    layer: sub_layer.bbox.min.z,
                    device_binding: sub_layer.device_binding.as_ref().map(|(dev, term)| {
                        hwc_physics::connectivity::DeviceBinding {
                            device_name: dev.as_str().into(),
                            terminals: vec![term.as_str().into()], // v0.2.2: Wrap single terminal in Vec
                        }
                    }), // v0.2.2: Convert (String, String) to DeviceBinding
                });
                seg_id += 1;
            }
        }

        // 3. Insert already-routed vector segments directly from the EntityGraph
        eprintln!(
            "[SPATIAL INDEX DEBUG] Building spatial index for net {:?}",
            active_route.net_id
        );
        eprintln!(
            "[SPATIAL INDEX DEBUG] entity_graph.get_all_routes() returned {} route groups",
            entity_graph.get_all_routes().len()
        );

        for (net_id, segments) in entity_graph.get_all_routes() {
            eprintln!(
                "[SPATIAL INDEX DEBUG]   Route group: net {:?}, {} segments",
                net_id,
                segments.len()
            );

            // v0.1.8 Same-Net Tapping: If these segments belong to the same net,
            // they are NOT hard obstacles. We can tap into them or overlap them.
            if *net_id == active_route.net_id {
                eprintln!(
                    "[SPATIAL INDEX DEBUG]   Skipping net {:?} - same as active route (same-net tapping)",
                    net_id
                );
                continue;
            }

            for (seg_idx, segment) in segments.iter().enumerate() {
                // Skip segments that touch our start or goal endpoints to allow connection
                let touches_start =
                    segment.start == active_route.start || segment.end == active_route.start;
                let touches_goal =
                    segment.start == active_route.goal || segment.end == active_route.goal;

                if touches_start || touches_goal {
                    continue;
                }

                spatial_index.insert(IndexedSegment {
                    source: hwc_physics::spatial_index::SpatialEntitySource::RouteSegment {
                        net_idx: (*net_id).raw() as usize,
                        seg_idx,
                    },
                    segment_id: seg_idx,
                    net_id: *net_id,
                    width_nm: segment.width_nm,
                    thickness_nm: {
                        let mat_props = self
                            .material_registry
                            .get_material(segment.material_id)
                            .unwrap_or_else(|| {
                                panic!(
                                    "FATAL: Route segment references unregistered material_id={}",
                                    segment.material_id
                                )
                            });

                        let thickness = mat_props.get("thickness").unwrap_or_else(|| {
                            panic!(
                                "FATAL: Material id={} has no 'thickness' property defined",
                                segment.material_id
                            )
                        }) as i64;

                        assert!(
                            thickness > 0,
                            "FATAL: Material id={} has zero thickness",
                            segment.material_id
                        );
                        thickness
                    },
                    start: segment.start,
                    end: segment.end,
                    layer: segment.start.z,
                    device_binding: None, // Routed traces don't have device bindings
                });
                seg_id += 1;
            }
        }

        spatial_index
    }
}
