use crate::netlist::NetId;
use super::super::super::types::{NetRoute, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::geometry::BoundingBox;
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::geometry_router::topological_router::TopologicalRouter;
use crate::geometry_router::EntityGraph;
use rustc_hash::FxHashMap;

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
        eprintln!("[OBSTACLE-DEBUG] net {:?}: {} component metadata entries", active_route.net_id, entity_graph.get_component_metadata().len());
        for meta in entity_graph.get_component_metadata() {
            eprintln!("[OBSTACLE-DEBUG]   meta '{}' type='{}' bbox=({},{},{})-({},{},{})", meta.name, meta.component_type, meta.bbox.min.x, meta.bbox.min.y, meta.bbox.min.z, meta.bbox.max.x, meta.bbox.max.y, meta.bbox.max.z);
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
                    start: hwc_physics::geometry::Point3D::new(meta.bbox.min.x, meta.bbox.min.y, z_min),
                    end: hwc_physics::geometry::Point3D::new(meta.bbox.max.x, meta.bbox.max.y, z_max),
                    layer: z_min,
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
            eprintln!("[OBSTACLE-DEBUG]   {} substrate layers", substrate_layers.len());
            for (i, sub_layer) in substrate_layers.iter().enumerate() {
                eprintln!("[OBSTACLE-DEBUG]     sub[{}] net={:?} bbox=({},{},{})-({},{},{})", i, sub_layer.net, sub_layer.bbox.min.x, sub_layer.bbox.min.y, sub_layer.bbox.min.z, sub_layer.bbox.max.x, sub_layer.bbox.max.y, sub_layer.bbox.max.z);
            }
            for (substrate_idx, sub_layer) in substrate_layers.iter().enumerate() {
                let sub_net_id = sub_layer.net;

              
                // Same-net pours are not obstacles (we can route over our own pours)
                // BUT: net_id = 0 pours (keepout zones) are ALWAYS obstacles
                if sub_net_id != NetId::UNCONNECTED && sub_net_id == active_route.net_id {
                  
                    continue;
                }

                // DESTINATION PAD EXEMPTION (v0.1.9 C-Space fix):
                // The goal anchor is placed at pad_edge - trace_width/2, so it sits just
                // OUTSIDE the raw bbox. After Minkowski inflation by trace_width/2 the
                // destination pad's inflated boundary swallows the goal → EndPointOutsideSpace.
                // Fix: use trace_width/2 as a proximity margin. If the goal (or start) lands
                // within that margin of the bbox boundary, this layer is an endpoint pad and
                // must be exempted from the obstacle list.
                // Only keepout zones (net_id = 0) are always obstacles regardless.
                if sub_net_id != NetId::UNCONNECTED {
                    let proximity = self.trace_width_nm / 2;
                    let goal = active_route.goal;
                    let bbox = &sub_layer.bbox;
                    // Expanded bbox by proximity margin on all XY sides (Z uses raw bounds)
                    if goal.x >= bbox.min.x - proximity
                        && goal.x <= bbox.max.x + proximity
                        && goal.y >= bbox.min.y - proximity
                        && goal.y <= bbox.max.y + proximity
                        && goal.z >= bbox.min.z
                        && goal.z <= bbox.max.z
                    {
                       
                        continue;
                    }
                    // Also exempt if the start point is docking into this pad (different net-id source)
                    let start = active_route.start;
                    if start.x >= bbox.min.x - proximity
                        && start.x <= bbox.max.x + proximity
                        && start.y >= bbox.min.y - proximity
                        && start.y <= bbox.max.y + proximity
                        && start.z >= bbox.min.z
                        && start.z <= bbox.max.z
                    {
                       
                        continue;
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
                    net_id: sub_net_id,
                    width_nm: 0,
                    thickness_nm: sub_layer.bbox.max.z - sub_layer.bbox.min.z,
                    start: sub_layer.bbox.min,
                    end: sub_layer.bbox.max,
                    layer: sub_layer.bbox.min.z,
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
                        assert!(
                            mat_props.thickness_nm > 0,
                            "FATAL: Material id={} has zero thickness",
                            segment.material_id
                        );
                        mat_props.thickness_nm
                    },
                    start: segment.start,
                    end: segment.end,
                    layer: segment.start.z,
                });
                seg_id += 1;
            }
        }

        spatial_index
    }

    /// Continuous detailed route of a point-to-point NetRoute with active legalization fallback.
    pub fn route_net(
        &mut self,
        entity_graph: &mut EntityGraph,
        route: &NetRoute,
    ) -> Result<RoutedNet, RoutingError> {
        // v0.2.0 HIERARCHICAL ROUTING: Check if same-net segments already exist (child routes)
        // If they do, route to them as intermediate waypoints instead of direct routing
        // Do this BEFORE borrowing fabrication to avoid borrow checker conflicts
        let existing_segments = entity_graph.get_all_routes()
            .iter()
            .find(|(net_id, _)| *net_id == route.net_id)
            .map(|(_, segments)| segments.clone());
        
        if let Some(segments) = &existing_segments {
            if !segments.is_empty() {
                eprintln!(
                    "[HIERARCHICAL ROUTING] Found {} existing same-net segments for NetId({}) - attempting tap routing",
                    segments.len(),
                    route.net_id.raw()
                );
                
                // Try routing to tap into existing segments
                if let Ok(result) = self.route_with_tapping(entity_graph, route, segments) {
                    return Ok(result);
                }
                
                eprintln!(
                    "[HIERARCHICAL ROUTING] Tap routing failed, falling back to direct routing"
                );
            }
        }

        // v0.1.8: Fail-Fast — fabrication constraints are MANDATORY.
        // No hardcoded fallbacks. All values come from the PDK profile.
        let fabrication = self.constraints.fabrication.as_ref().ok_or_else(|| {
            RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: "No fabrication constraints loaded from PDK profile. \
                    Ensure a profile with 'trace:' and 'clearance:' constraints \
                    is declared in the space definition."
                    .into(),
            }
        })?;

        let trace_width = fabrication.min_trace_width_nm;
        let max_x = self.bounds.width_nm.saturating_sub(1);
        let max_y = self.bounds.height_nm.saturating_sub(1);
        let max_z = self.bounds.depth_nm.saturating_sub(1);
        let clamp_coord = |p: crate::geometry::Point3D| -> crate::geometry::Point3D {
            crate::geometry::Point3D::new(
                p.x.max(0).min(max_x),
                p.y.max(0).min(max_y),
                p.z.max(0).min(max_z),
            )
        };
        let start = clamp_coord(route.start);
        let goal = clamp_coord(route.goal);

        let board_bounds = BoundingBox::new(
            crate::geometry::Point3D::new(0, 0, 0),
            crate::geometry::Point3D::new(
                self.bounds.width_nm,
                self.bounds.height_nm,
                self.bounds.depth_nm,
            ),
        );

        // Pass route context to exempt active endpoints from obstacles
        let spatial_index = self.build_routing_spatial_index(entity_graph, route);

        let track_pitch = self.resolution_nm; // Use snap-resolution for pitch

        let path = if start.x == goal.x && start.y == goal.y && start.z == goal.z {
            vec![start, goal]
        } else {
            // v0.1.9: Use TopologicalRouter as the single authoritative routing engine
            let topo_router =
                TopologicalRouter::new(trace_width, track_pitch, fabrication.min_trace_spacing_nm);

            // v0.1.9: Use route_with_exemptions to allow routing from/to pads without self-collision
            // Exempt the active net_id so start/goal pads are not treated as obstacles
            let exempt_net_ids = vec![route.net_id.raw() as usize];

            // v0.1.9: TopologicalRouter is the single authoritative routing engine
            // NO FALLBACK - if TopologicalRouter can't find a path, the route fails
            match topo_router.route_with_exemptions(
                start,
                goal,
                &spatial_index,
                &board_bounds,
                &exempt_net_ids,
            ) {
                Some(topo_path) if topo_path.waypoints.len() >= 2 => topo_path.waypoints,
                _ => {
                    return Err(RoutingError::NoPathFound {
                        net_id: route.net_id,
                        start: route.start,
                        goal: route.goal,
                    });
                }
            }
        };

        let detected_vias = self.extract_vias_from_path(&path, route.net_id);

        let unrolled_vias: Vec<_> = detected_vias
            .iter()
            .flat_map(|via| self.unroll_detected_via(via))
            .collect();

        let mut placed_vias = Vec::new();
        for via in unrolled_vias {
            if self.can_place_via(entity_graph, via.position, via.from_z_nm, via.to_z_nm) {
                self.stamp_via(entity_graph, &via);

                self.vias.push(via.clone());
                placed_vias.push(via);
            }
        }

        // Commit the resolved vector route canonically to the EntityGraph
        entity_graph.register_route(
            route.net_id,
            &path,
            self.routing_material_id,
            self.trace_width_nm,
        );

        Ok(RoutedNet {
            net_id: route.net_id,
            paths: vec![path],
            vias: placed_vias,
        })
    }

    /// Localized legalization fallback: when topological routing fails, attempt to
    /// nudge adjacent traces within a bounding window to clear a path.
    pub(crate) fn legalize_local_window(
        &mut self,
        entity_graph: &mut EntityGraph,
        _window: &BoundingBox,
        route: &NetRoute,
    ) -> Result<Vec<crate::geometry::Point3D>, RoutingError> {
        use crate::geometry_router::legalizer::Legalizer;

        // v0.1.8: Fail-Fast — fabrication constraints are MANDATORY.
        let min_clearance = self
            .constraints
            .fabrication
            .as_ref()
            .map(|fab| fab.min_trace_spacing_nm)
            .ok_or_else(|| RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: "Legalization requires fabrication constraints but none are loaded."
                    .into(),
            })?;

        let legalizer = Legalizer::new(min_clearance);

        // Collect all segments and net_ids from the entity graph (the source of truth)
        let all_routes = entity_graph.get_all_routes();
        let mut all_segments = Vec::new();
        let mut all_net_ids = Vec::new();
        for (net_id, segments) in all_routes {
            for seg in segments {
                all_segments.push(seg.clone());
                all_net_ids.push(*net_id);
            }
        }

        if all_segments.is_empty() {
            // No existing traces to nudge — try a direct Manhattan path
            let mut path = Vec::new();
            path.push(route.start);
            if route.start.x != route.goal.x && route.start.y != route.goal.y {
                path.push(crate::geometry::Point3D::new(
                    route.goal.x,
                    route.start.y,
                    route.start.z,
                ));
            }
            path.push(route.goal);
            return Ok(path);
        }

        // Run legalization to nudge existing traces
        let spatial_index = self.build_routing_spatial_index(entity_graph, route);

        // Record original positions to compute displacements for via sliding
        let original_centers: Vec<(crate::netlist::NetId, i64, i64)> = all_segments
            .iter()
            .zip(all_net_ids.iter())
            .map(|(seg, net_id)| {
                let cx = (seg.start.x + seg.end.x) / 2;
                let cy = (seg.start.y + seg.end.y) / 2;
                (*net_id, cx, cy)
            })
            .collect();

        let (legalized_segments, legalized_net_ids) = legalizer.legalize(
            &all_segments,
            &all_net_ids,
            &spatial_index,
            5,
        );

        // Compute per-net displacement from legalization and update via positions
        let mut net_displacements: FxHashMap<crate::netlist::NetId, (i64, i64)> =
            FxHashMap::default();
        let mut net_counts: FxHashMap<crate::netlist::NetId, usize> = FxHashMap::default();
        for (idx, seg) in legalized_segments.iter().enumerate() {
            let net_id = legalized_net_ids[idx];
            let new_cx = (seg.start.x + seg.end.x) / 2;
            let new_cy = (seg.start.y + seg.end.y) / 2;
            if let Some((orig_cx, orig_cy)) =
                original_centers.get(idx).map(|(_, cx, cy)| (*cx, *cy))
            {
                let entry = net_displacements.entry(net_id).or_insert((0, 0));
                entry.0 += new_cx - orig_cx;
                entry.1 += new_cy - orig_cy;
                *net_counts.entry(net_id).or_insert(0) += 1;
            }
        }
        // Average displacement per net
        for (net_id, count) in &net_counts {
            if let Some(disp) = net_displacements.get_mut(net_id) {
                if *count > 0 {
                    disp.0 /= *count as i64;
                    disp.1 /= *count as i64;
                }
            }
        }
        // Slide vias that belong to displaced nets
        for via in &mut self.vias {
            if let Some(&(dx, dy)) = net_displacements.get(&via.net_id) {
                if dx != 0 || dy != 0 {
                    via.position.0 += dx;
                    via.position.1 += dy;
                }
            }
        }

        // Write legalized segments back to EntityGraph (source of truth)
        let net_ids_to_clear: Vec<_> = entity_graph
            .get_all_routes()
            .iter()
            .map(|(net_id, _)| *net_id)
            .collect();
        for net_id in net_ids_to_clear {
            entity_graph.clear_routes_for_net(net_id);
        }
        for (idx, seg) in legalized_segments.iter().enumerate() {
            let net_id = legalized_net_ids[idx];
            entity_graph
                .register_trace_segments(net_id, vec![seg.clone()]);
        }

        // Rebuild spatial index from updated EntityGraph
        let board_bounds = BoundingBox::new(
            crate::geometry::Point3D::new(0, 0, 0),
            crate::geometry::Point3D::new(
                self.bounds.width_nm,
                self.bounds.height_nm,
                self.bounds.depth_nm,
            ),
        );

        let updated_spatial_index = self.build_routing_spatial_index(entity_graph, route);
        let topo_router = TopologicalRouter::new(
            self.constraints
                .fabrication
                .as_ref()
                .expect("fabrication constraints guaranteed by earlier check")
                .min_trace_width_nm,
            self.resolution_nm,
            min_clearance,
        );

        // v0.1.9: Use route_with_exemptions to allow routing from/to pads without self-collision
        let exempt_net_ids = vec![route.net_id.raw() as usize];

        match topo_router.route_with_exemptions(
            route.start,
            route.goal,
            &updated_spatial_index,
            &board_bounds,
            &exempt_net_ids,
        ) {
            Some(topo_path) if topo_path.waypoints.len() >= 2 => Ok(topo_path.waypoints),
            _ => {
                let mut path = vec![route.start];
                if route.start.x != route.goal.x && route.start.y != route.goal.y {
                    path.push(crate::geometry::Point3D::new(
                        route.goal.x,
                        route.start.y,
                        route.start.z,
                    ));
                }
                path.push(route.goal);
                Ok(path)
            }
        }
    }

    pub fn route_net_with_length_constraint(
        &mut self,
        entity_graph: &mut EntityGraph,
        route: &NetRoute,
        target_length_nm: i64,
        pattern: &Option<super::super::super::routing_patterns::RoutingPattern>,
    ) -> Result<RoutedNet, RoutingError> {
        let fabrication = self.constraints.fabrication.as_ref().ok_or_else(|| {
            RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: "No fabrication constraints loaded from PDK profile.".into(),
            }
        })?;

        let trace_width = fabrication.min_trace_width_nm;
        let track_pitch = self.resolution_nm;
        let board_bounds = BoundingBox::new(
            crate::geometry::Point3D::new(0, 0, 0),
            crate::geometry::Point3D::new(
                self.bounds.width_nm,
                self.bounds.height_nm,
                self.bounds.depth_nm,
            ),
        );

        let spatial_index = self.build_routing_spatial_index(entity_graph, route);
        let topo_router =
            TopologicalRouter::new(trace_width, track_pitch, fabrication.min_trace_spacing_nm);

        // v0.1.9: Use route_with_exemptions to allow routing from/to pads without self-collision
        let exempt_net_ids = vec![route.net_id.raw() as usize];

        let path = match topo_router.route_with_exemptions(
            route.start,
            route.goal,
            &spatial_index,
            &board_bounds,
            &exempt_net_ids,
        ) {
            Some(topo_path) if topo_path.waypoints.len() >= 2 => topo_path.waypoints,
            _ => {
                let collision_window = BoundingBox::new(route.start, route.goal);
                if let Ok(legalized_coords) =
                    self.legalize_local_window(entity_graph, &collision_window, route)
                {
                    legalized_coords
                } else {
                    return Err(RoutingError::NoPathFound {
                        net_id: route.net_id,
                        start: route.start,
                        goal: route.goal,
                    });
                }
            }
        };

        // v0.1.9: If a pattern is provided and the straight path is shorter than
        // the target, inject meander at the midpoint of the longest straight segment.
        let final_path = if let Some(pat) = pattern {
            let straight_length: i64 = path
                .windows(2)
                .map(|w| {
                    let dx = (w[0].x - w[1].x).abs();
                    let dy = (w[0].y - w[1].y).abs();
                    let dz = (w[0].z - w[1].z).abs();
                    dx + dy + dz
                })
                .sum();

            let deficit = target_length_nm.saturating_sub(straight_length);
            if deficit > 0 {
                // Find the longest straight segment to place the meander
                let mut best_seg_idx = 0;
                let mut best_seg_len = 0i64;
                for (i, w) in path.windows(2).enumerate() {
                    let dx = (w[0].x - w[1].x).abs();
                    let dy = (w[0].y - w[1].y).abs();
                    let dz = (w[0].z - w[1].z).abs();
                    let seg_len = dx + dy + dz;
                    if seg_len > best_seg_len {
                        best_seg_len = seg_len;
                        best_seg_idx = i;
                    }
                }

                if best_seg_len > trace_width * 4 {
                    // Compute midpoint of the longest segment
                    let p_a = path[best_seg_idx];
                    let p_b = path[best_seg_idx + 1];
                    let mid = crate::geometry::Point3D::new(
                        (p_a.x + p_b.x) / 2,
                        (p_a.y + p_b.y) / 2,
                        (p_a.z + p_b.z) / 2,
                    );

                    // Determine heading from segment direction
                    let heading = if (p_a.x - p_b.x).abs() > (p_a.y - p_b.y).abs() {
                        if p_b.x > p_a.x {
                            0i64
                        } else {
                            180i64
                        }
                    } else if p_b.y > p_a.y {
                        90i64
                    } else {
                        270i64
                    };

                    // Generate meander waypoints from the pattern
                    let step_size = trace_width * 2;
                    let meander_points = pat.generate_moves(mid, heading, step_size);

                    if meander_points.len() > 2 {
                        // Calculate how much length the meander actually adds
                        let meander_length: i64 = meander_points
                            .windows(2)
                            .map(|w| {
                                let dx = (w[0].x - w[1].x).abs();
                                let dy = (w[0].y - w[1].y).abs();
                                dx + dy
                            })
                            .sum();

                        // Scale the meander if it's too long or too short
                        let scale = if meander_length > 0 {
                            (deficit as f64 / meander_length as f64).sqrt()
                        } else {
                            1.0
                        };

                        if (scale - 1.0).abs() > 0.1 {
                            // Re-generate with scaled step size
                            let scaled_step = (step_size as f64 * scale) as i64;
                            let scaled_meander =
                                pat.generate_moves(mid, heading, scaled_step.max(trace_width));

                            // Splice the meander into the path
                            let mut new_path =
                                Vec::with_capacity(path.len() + scaled_meander.len());
                            new_path.extend_from_slice(&path[..=best_seg_idx]);
                            // Skip first point of meander (it's the same as mid)
                            for pt in scaled_meander.iter().skip(1) {
                                new_path.push(*pt);
                            }
                            new_path.extend_from_slice(&path[best_seg_idx + 1..]);
                            new_path
                        } else {
                            // Meander length is close enough, splice it directly
                            let mut new_path =
                                Vec::with_capacity(path.len() + meander_points.len());
                            new_path.extend_from_slice(&path[..=best_seg_idx]);
                            for pt in meander_points.iter().skip(1) {
                                new_path.push(*pt);
                            }
                            new_path.extend_from_slice(&path[best_seg_idx + 1..]);
                            new_path
                        }
                    } else {
                        path
                    }
                } else {
                    path
                }
            } else {
                path
            }
        } else {
            path
        };

        let detected_vias = self.extract_vias_from_path(&final_path, route.net_id);

        let unrolled_vias: Vec<_> = detected_vias
            .iter()
            .flat_map(|via| self.unroll_detected_via(via))
            .collect();

        let mut placed_vias = Vec::new();
        for via in unrolled_vias {
            if self.can_place_via(entity_graph, via.position, via.from_z_nm, via.to_z_nm) {
                self.stamp_via(entity_graph, &via);
                self.vias.push(via.clone());
                placed_vias.push(via);
            }
        }

        entity_graph.register_route(
            route.net_id,
            &final_path,
            self.routing_material_id,
            self.trace_width_nm,
        );

        Ok(RoutedNet {
            net_id: route.net_id,
            paths: vec![final_path],
            vias: placed_vias,
        })
    }
}

impl GeometryRouter {
    /// v0.2.0 Hierarchical Routing: Route by tapping into existing same-net segments.
    ///
    /// This implements the fundamental hierarchical routing pattern where child routes
    /// from lower hierarchy levels become tap points for parent-level routing.
    ///
    /// Algorithm:
    /// 1. Find all tap points on existing segments (sample points along each segment)
    /// 2. Find the closest tap point to start
    /// 3. Find the closest tap point to goal
    /// 4. Route: start → tap_point_1 → (existing segment) → tap_point_2 → goal
    fn route_with_tapping(
        &mut self,
        entity_graph: &mut EntityGraph,
        route: &NetRoute,
        existing_segments: &[hwc_physics::TraceSegment],
    ) -> Result<RoutedNet, RoutingError> {
        use crate::geometry::Point3D;

        eprintln!("[TAP ROUTING] Starting tap routing for NetId({})", route.net_id.raw());
        eprintln!("[TAP ROUTING]   Start: {:?}", route.start);
        eprintln!("[TAP ROUTING]   Goal:  {:?}", route.goal);
        eprintln!("[TAP ROUTING]   {} existing segments", existing_segments.len());

        // Step 1: Extract all tap points from existing segments
        let mut tap_points = Vec::new();
        for (seg_idx, segment) in existing_segments.iter().enumerate() {
            // Add segment endpoints as tap points
            tap_points.push((segment.start, seg_idx, "start"));
            tap_points.push((segment.end, seg_idx, "end"));
            
            // Sample points along the segment for better connectivity
            let segment_length = ((segment.end.x - segment.start.x).pow(2) +
                                  (segment.end.y - segment.start.y).pow(2) +
                                  (segment.end.z - segment.start.z).pow(2)) as f64;
            let segment_length = segment_length.sqrt() as i64;
            
            if segment_length > 1000 {
                // Sample at 1µm intervals for long segments
                let num_samples = (segment_length / 1000).min(10) as usize;
                for i in 1..num_samples {
                    let t = i as f64 / num_samples as f64;
                    let sample_point = Point3D::new(
                        segment.start.x + ((segment.end.x - segment.start.x) as f64 * t) as i64,
                        segment.start.y + ((segment.end.y - segment.start.y) as f64 * t) as i64,
                        segment.start.z + ((segment.end.z - segment.start.z) as f64 * t) as i64,
                    );
                    tap_points.push((sample_point, seg_idx, "sample"));
                }
            }
        }

        eprintln!("[TAP ROUTING] Generated {} tap points", tap_points.len());

        // Step 2: Find closest tap point to start
        let closest_to_start = tap_points.iter()
            .min_by_key(|(point, _, _)| {
                ((point.x - route.start.x).pow(2) +
                 (point.y - route.start.y).pow(2) +
                 (point.z - route.start.z).pow(2)) as i64
            });

        // Step 3: Find closest tap point to goal
        let closest_to_goal = tap_points.iter()
            .min_by_key(|(point, _, _)| {
                ((point.x - route.goal.x).pow(2) +
                 (point.y - route.goal.y).pow(2) +
                 (point.z - route.goal.z).pow(2)) as i64
            });

        if let (Some((tap_start, seg_idx_start, pos_start)), Some((tap_goal, seg_idx_goal, pos_goal))) = 
            (closest_to_start, closest_to_goal) {
            
            eprintln!("[TAP ROUTING] Closest tap to start: {:?} (segment {}, {})", tap_start, seg_idx_start, pos_start);
            eprintln!("[TAP ROUTING] Closest tap to goal:  {:?} (segment {}, {})", tap_goal, seg_idx_goal, pos_goal);

            // Step 4: Route start → tap_start
            let route_to_tap = NetRoute {
                net_id: route.net_id,
                start: route.start,
                goal: *tap_start,
            };
            
            // Use direct routing for tap connections (don't recurse)
            let result_to_tap = self.route_net_direct(entity_graph, &route_to_tap)?;
            
            // Step 5: Route tap_goal → goal
            let route_from_tap = NetRoute {
                net_id: route.net_id,
                start: *tap_goal,
                goal: route.goal,
            };
            
            let result_from_tap = self.route_net_direct(entity_graph, &route_from_tap)?;
            
            // Step 6: Combine paths
            // The existing segment between tap points is already in entity_graph,
            // so we just need to register our new segments
            eprintln!("[TAP ROUTING] Successfully routed via tapping!");
            eprintln!("[TAP ROUTING]   Segment 1: {} waypoints (start → tap)", result_to_tap.paths[0].len());
            eprintln!("[TAP ROUTING]   Segment 2: {} waypoints (tap → goal)", result_from_tap.paths[0].len());
            
            // Return the combined result
            let mut all_paths = result_to_tap.paths;
            all_paths.extend(result_from_tap.paths);
            
            let mut all_vias = result_to_tap.vias;
            all_vias.extend(result_from_tap.vias);
            
            Ok(RoutedNet {
                net_id: route.net_id,
                paths: all_paths,
                vias: all_vias,
            })
        } else {
            Err(RoutingError::NoPathFound {
                net_id: route.net_id,
                start: route.start,
                goal: route.goal,
            })
        }
    }

    /// Direct routing without tap-routing logic (to avoid recursion).
    fn route_net_direct(
        &mut self,
        entity_graph: &mut EntityGraph,
        route: &NetRoute,
    ) -> Result<RoutedNet, RoutingError> {
        use crate::geometry_router::topological_router::TopologicalRouter;
        use crate::geometry::BoundingBox;
        
        let fabrication = self.constraints.fabrication.as_ref().ok_or_else(|| {
            RoutingError::MissingFabricationConstraints {
                net_id: route.net_id,
                message: "No fabrication constraints".into(),
            }
        })?;

        let trace_width = fabrication.min_trace_width_nm;
        let board_bounds = BoundingBox::new(
            crate::geometry::Point3D::new(0, 0, 0),
            crate::geometry::Point3D::new(
                self.bounds.width_nm,
                self.bounds.height_nm,
                self.bounds.depth_nm,
            ),
        );

        let spatial_index = self.build_routing_spatial_index(entity_graph, route);
        let track_pitch = self.resolution_nm;

        let topo_router =
            TopologicalRouter::new(trace_width, track_pitch, fabrication.min_trace_spacing_nm);

        let exempt_net_ids = vec![route.net_id.raw() as usize];

        let path = match topo_router.route_with_exemptions(
            route.start,
            route.goal,
            &spatial_index,
            &board_bounds,
            &exempt_net_ids,
        ) {
            Some(topo_path) if topo_path.waypoints.len() >= 2 => topo_path.waypoints,
            _ => {
                return Err(RoutingError::NoPathFound {
                    net_id: route.net_id,
                    start: route.start,
                    goal: route.goal,
                });
            }
        };

        let detected_vias = self.extract_vias_from_path(&path, route.net_id);
        let unrolled_vias: Vec<_> = detected_vias
            .iter()
            .flat_map(|via| self.unroll_detected_via(via))
            .collect();

        let mut placed_vias = Vec::new();
        for via in unrolled_vias {
            if self.can_place_via(entity_graph, via.position, via.from_z_nm, via.to_z_nm) {
                self.stamp_via(entity_graph, &via);
                self.vias.push(via.clone());
                placed_vias.push(via);
            }
        }

        entity_graph.register_route(
            route.net_id,
            &path,
            self.routing_material_id,
            self.trace_width_nm,
        );

        Ok(RoutedNet {
            net_id: route.net_id,
            paths: vec![path],
            vias: placed_vias,
        })
    }
}
